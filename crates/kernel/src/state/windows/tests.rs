//! The kept windows' file, and a workspace kept and opened again.

use std::collections::BTreeMap;

use lokyr_pty::Program;
use lokyr_vt_core::terminal::Colors;

use super::*;
use crate::l10n::Texts;
use crate::workspace::{Environment, Grid, PaneId, Settings, WindowId};

/// A state's directory of its own for the test `name`, empty.
fn state(name: &str) -> PathBuf {
    let state = std::env::temp_dir().join(format!("lokyr-windows-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&state);
    state
}

/// A pane whose program waits, and does nothing.
fn pane() -> Pane {
    let program = Program::command("/bin/sh", ["-c", "exec sleep 60"]);
    Pane::spawn(
        &program,
        &Environment::default(),
        &Settings::default(),
        Grid {
            columns: 80,
            rows: 24,
            cell_width: 8,
            cell_height: 16,
        },
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        || {},
    )
}

/// Two windows: the first of a pinned tab of three panes, its splits of a
/// ratio and of none, the keyboard on its last pane, then a tab of a title
/// and a color of its own, which it shows, in a group of a name and a
/// color with the tab after it, and a last tab in a folded group of its
/// own; the second of one tab of two panes, the first zoomed.
fn sample() -> Kept {
    let pane = |directory: &str| Layout::Pane {
        directory: Some(directory.to_owned()),
        scrollback: None,
    };
    Kept {
        sessions: Vec::new(),
        of_sessions: Vec::new(),
        version: VERSION,
        windows: vec![
            Window {
                session: None,
                frame: Some(Frame {
                    x: 40.0,
                    y: 60.0,
                    width: 800.0,
                    height: 600.0,
                }),
                tabs: vec![
                    Tab {
                        title: None,
                        color: None,
                        layout: Layout::Split {
                            direction: Direction::Horizontal,
                            ratio: Some(0.3),
                            first: Box::new(pane("/a")),
                            second: Box::new(Layout::Split {
                                direction: Direction::Vertical,
                                ratio: None,
                                first: Box::new(pane("/b")),
                                second: Box::new(pane("/c")),
                            }),
                        },
                        active: 2,
                        zoomed: None,
                        pinned: true,
                        group: None,
                    },
                    Tab {
                        title: Some("logs".to_owned()),
                        color: Some("#ff8000".to_owned()),
                        layout: pane("/d"),
                        active: 0,
                        zoomed: None,
                        pinned: false,
                        group: Some(0),
                    },
                    Tab {
                        title: None,
                        color: None,
                        layout: pane("/f"),
                        active: 0,
                        zoomed: None,
                        pinned: false,
                        group: Some(0),
                    },
                    Tab {
                        title: None,
                        color: None,
                        layout: pane("/g"),
                        active: 0,
                        zoomed: None,
                        pinned: false,
                        group: Some(1),
                    },
                ],
                active: 1,
                groups: vec![
                    Group {
                        name: "work".to_owned(),
                        color: Some("#3478f6".to_owned()),
                        folded: false,
                    },
                    Group {
                        name: String::new(),
                        color: None,
                        folded: true,
                    },
                ],
            },
            Window {
                session: None,
                frame: None,
                tabs: vec![Tab {
                    title: None,
                    color: None,
                    layout: Layout::Split {
                        direction: Direction::Vertical,
                        ratio: Some(0.75),
                        first: Box::new(Layout::Pane {
                            directory: None,
                            scrollback: None,
                        }),
                        second: Box::new(pane("/e")),
                    },
                    active: 0,
                    zoomed: Some(0),
                    pinned: false,
                    group: None,
                }],
                active: 0,
                groups: Vec::new(),
            },
        ],
        front: Some(1),
    }
}

#[test]
fn the_kept_windows_read_back_as_they_were_written() {
    let state = state("back");
    assert!(matches!(read(&state), Ok(None)));
    let kept = sample();
    write(&state, &kept).unwrap();
    assert_eq!(read(&state).unwrap(), Some(kept));
    // Nothing of the write is left past it.
    let names: Vec<_> = fs::read_dir(&state)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names, [FILE]);
    forget(&state).unwrap();
    assert!(matches!(read(&state), Ok(None)));
    // Forgetting nothing is no error.
    forget(&state).unwrap();
    let _ = fs::remove_dir_all(&state);
}

#[test]
fn the_file_is_json_of_its_version_first() {
    let text = serde_json::to_string(&sample()).unwrap();
    assert!(text.starts_with(r#"{"version":1,"windows":[{"frame":{"x":40.0,"#));
    assert!(text.contains(r##""groups":[{"name":"work","color":"#3478f6"},{"folded":true}]"##));
    assert!(text.contains(
        r#""layout":{"split":{"direction":"horizontal","ratio":0.3,"first":{"pane":{"directory":"/a"}}"#
    ));
    // What is not set is not written.
    assert!(text.contains(r#"{"pane":{}}"#), "{text}");
    assert!(!text.contains("null"), "{text}");
}

#[test]
fn a_newer_or_unreadable_file_is_left_aside() {
    let state = state("aside");
    let unread = |text: &str| {
        fs::create_dir_all(&state).unwrap();
        fs::write(state.join(FILE), text).unwrap();
        read(&state).unwrap_err()
    };
    assert!(matches!(
        unread(r#"{"version":2,"windows":[{"tabs":"of another shape"}]}"#),
        Unread::Newer(2)
    ));
    assert!(matches!(unread("{not json"), Unread::Invalid(_)));
    assert!(matches!(unread(r#"{"windows":[]}"#), Unread::Invalid(_)));
    assert!(matches!(
        unread(r#"{"version":1,"windows":[{"tabs":[{"layout":{"leaf":1}}]}]}"#),
        Unread::Invalid(_)
    ));
    // An older version's file, and fields that this one does not know,
    // read.
    fs::write(
        state.join(FILE),
        r#"{"version":0,"windows":[],"later":true}"#,
    )
    .unwrap();
    assert!(read(&state).unwrap().is_some());
    // More panes than any user's.
    let mut kept = sample();
    kept.windows.truncate(1);
    kept.windows[0].tabs = (0..=MOST_PANES)
        .map(|_| Tab {
            title: None,
            color: None,
            layout: Layout::Pane {
                directory: None,
                scrollback: None,
            },
            active: 0,
            zoomed: None,
            pinned: false,
            group: None,
        })
        .collect();
    write(&state, &kept).unwrap();
    let Err(Unread::Invalid(error)) = read(&state) else {
        panic!("a file of too many panes reads");
    };
    assert!(error.contains("257 panes"), "{error}");
    // More windows than any user's, though of no pane.
    let mut kept = sample();
    kept.windows = (0..=MOST_WINDOWS)
        .map(|_| Window {
            session: None,
            frame: None,
            tabs: Vec::new(),
            active: 0,
            groups: Vec::new(),
        })
        .collect();
    write(&state, &kept).unwrap();
    let Err(Unread::Invalid(error)) = read(&state) else {
        panic!("a file of too many windows reads");
    };
    assert!(error.contains("101 windows"), "{error}");
    let _ = fs::remove_dir_all(&state);
}

#[test]
fn a_workspace_kept_opens_again_as_it_was() {
    let kept = sample();
    // Each pane of the directory that its spawn was asked for.
    let mut directories: BTreeMap<PaneId, Option<PathBuf>> = BTreeMap::new();
    let mut workspace = Workspace::new();
    let mut opened: Vec<(WindowId, Option<Frame>)> = Vec::new();
    for window in &kept.windows {
        let id = window
            .open(&mut workspace, |kept| {
                let pane = pane();
                directories.insert(pane.id(), kept.directory.map(Path::to_path_buf));
                pane
            })
            .unwrap();
        opened.push((id, window.frame));
    }
    // Each pane spawned in its directory, in the layout's order.
    assert_eq!(directories.len(), 8);
    let again = Kept::of(
        &workspace,
        &opened,
        Some(opened[1].0),
        |pane| directories.get(&pane.id()).cloned().flatten(),
        |_, _| None,
    );
    assert_eq!(again, kept);
    // The workspace's own: the second split's ratio is the parts' weights',
    // the keyboard on the third pane, the second tab shown.
    let window = workspace.window(opened[0].0).unwrap();
    assert_eq!(window.tabs().len(), 4);
    assert_eq!(window.active_tab(), window.tabs()[1]);
    let tab = workspace.tab(window.tabs()[0]).unwrap();
    let panes = tab.layout().panes();
    assert_eq!(tab.active_pane(), panes[2]);
    let workspace::Layout::Split { ratio, second, .. } = tab.layout() else {
        panic!("{:?}", tab.layout());
    };
    assert_eq!(*ratio, Some(Ratio::new(0.3)));
    assert!(matches!(
        **second,
        workspace::Layout::Split { ratio: None, .. }
    ));
    // The first pinned, the second not.
    assert!(workspace.tab(window.tabs()[0]).unwrap().marks().pinned);
    let tab = workspace.tab(window.tabs()[1]).unwrap();
    assert!(!tab.marks().pinned);
    assert_eq!(tab.title(), "logs");
    assert_eq!(
        tab.color(),
        Some(Rgb {
            r: 0xff,
            g: 0x80,
            b: 0
        })
    );
    // The groups, the last folded.
    let groups = workspace.groups_of(opened[0].0);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name(), "work");
    assert_eq!(workspace.group_tabs(groups[0].id()), window.tabs()[1..3]);
    assert!(groups[1].folded());
    let window = workspace.window(opened[1].0).unwrap();
    let tab = workspace.tab(window.active_tab()).unwrap();
    assert_eq!(tab.zoomed(), Some(tab.layout().panes()[0]));
}

#[test]
fn a_kept_group_s_name_of_a_hidden_character_opens_of_none() {
    let mut kept = sample();
    // A name that no plugin could write, as a hand's edit of the file may.
    kept.windows[0].groups[0].name = "w\u{202E}ork".to_owned();
    let mut workspace = Workspace::new();
    let window = kept.windows[0].open(&mut workspace, |_| pane()).unwrap();
    let groups = workspace.groups_of(window);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].name(), "");
}

#[test]
fn the_settings_tab_is_not_kept_and_its_window_keeps_the_tab_before_it() {
    let kept = sample();
    let mut workspace = Workspace::new();
    let mut opened: Vec<(WindowId, Option<Frame>)> = Vec::new();
    for window in &kept.windows {
        let id = window.open(&mut workspace, |_| pane()).unwrap();
        opened.push((id, window.frame));
    }
    // The first window shows its second tab; the settings tab after it,
    // which it shows.
    let first = opened[0].0;
    let tabs = workspace.window(first).unwrap().tabs().to_vec();
    workspace.open_settings(first, "Settings").unwrap();
    let again = Kept::of(&workspace, &opened, None, |_| None, |_, _| None);
    assert_eq!(again.windows[0].tabs.len(), 4);
    assert_eq!(again.windows[0].active, 1);
    // The settings tab, between two tabs of a group, splits it not.
    assert_eq!(again.windows[0].groups, kept.windows[0].groups);
    // Shown from before the settings tab: the first tab's place.
    workspace.activate_tab(tabs[0]);
    let again = Kept::of(&workspace, &opened, None, |_| None, |_, _| None);
    assert_eq!(again.windows[0].active, 0);
    // Moved first: the tab that the window shows keeps its place.
    let (settings, _) = workspace.settings_tab().unwrap();
    assert!(workspace.move_tab(settings, first, 0));
    workspace.activate_tab(tabs[1]);
    let again = Kept::of(&workspace, &opened, None, |_| None, |_, _| None);
    assert_eq!(again.windows[0].active, 1);
}

#[test]
fn a_window_of_another_workspace_or_of_no_tab_is_left_out() {
    let mut workspace = Workspace::new();
    let shown = workspace.open_window(pane());
    let kept = Kept::of(
        &workspace,
        &[(WindowId::of(u64::MAX), None), (shown, None)],
        Some(WindowId::of(u64::MAX)),
        |_| None,
        |_, _| None,
    );
    assert_eq!(kept.windows.len(), 1);
    assert_eq!(kept.front, None);
    let empty = Window {
        session: None,
        frame: None,
        tabs: Vec::new(),
        active: 0,
        groups: Vec::new(),
    };
    assert_eq!(empty.open(&mut workspace, |_| pane()), None);
}

#[test]
fn a_color_is_kept_as_six_hex_digits() {
    let color = Rgb {
        r: 1,
        g: 0xab,
        b: 0xff,
    };
    assert_eq!(hex(color), "#01abff");
    assert_eq!(rgb("#01abff"), Some(color));
    assert_eq!(rgb("#01ABFF"), Some(color));
    for wrong in [
        "01abff", "#01abf", "#01abfff", "#01abfg", "#+1abff", "#é1abf",
    ] {
        assert_eq!(rgb(wrong), None, "{wrong}");
    }
}

#[test]
fn each_pane_names_its_scrollback_s_file_in_the_layout_s_order() {
    let kept = sample();
    let mut workspace = Workspace::new();
    let mut opened: Vec<(WindowId, Option<Frame>)> = Vec::new();
    for window in &kept.windows {
        let id = window.open(&mut workspace, |_| pane()).unwrap();
        opened.push((id, window.frame));
    }
    // Every pane but the fourth keeps a scrollback.
    let again = Kept::of(
        &workspace,
        &opened,
        None,
        |_| None,
        |_, index| (index != 3).then(|| super::super::scrollback::name(index)),
    );
    let mut names = Vec::new();
    fn collect(layout: &Layout, names: &mut Vec<Option<String>>) {
        match layout {
            Layout::Pane { scrollback, .. } => names.push(scrollback.clone()),
            Layout::Split { first, second, .. } => {
                collect(first, names);
                collect(second, names);
            }
        }
    }
    for tab in again.windows.iter().flat_map(|window| &window.tabs) {
        collect(&tab.layout, &mut names);
    }
    let expected: Vec<Option<String>> = (0..8)
        .map(|index| (index != 3).then(|| format!("{index}.scrollback")))
        .collect();
    assert_eq!(names, expected);
    // Written, it names the file; a pane of none, and a file of an earlier
    // Lokyr, read none.
    let text = serde_json::to_string(&again).unwrap();
    assert!(text.contains(r#""scrollback":"0.scrollback""#));
    let earlier: Layout = serde_json::from_str(r#"{"pane":{"directory":"/tmp"}}"#).unwrap();
    assert_eq!(
        earlier,
        Layout::Pane {
            directory: Some("/tmp".to_owned()),
            scrollback: None
        }
    );
}

#[test]
fn the_windows_keep_their_sessions_in_the_order_of_their_use() {
    let mut workspace = Workspace::new();
    let home = workspace.active_session();
    let first = workspace.open_window(pane());
    let api = workspace.new_session("api").unwrap();
    workspace.set_session_folder(api, Some("/src/api".to_owned()));
    let second = workspace.open_window(pane());
    let web = workspace.new_session("web").unwrap();
    let third = workspace.open_window(pane());
    assert!(workspace.rename_session(home, "home"));
    let shown = [(first, None), (second, None), (third, None)];
    let kept = Kept::of(&workspace, &shown, Some(third), |_| None, |_, _| None);
    // The active one first, then the one before it.
    assert_eq!(
        kept.sessions,
        [
            Session {
                name: "web".to_owned(),
                folder: None
            },
            Session {
                name: "api".to_owned(),
                folder: Some("/src/api".to_owned())
            },
            Session {
                name: "home".to_owned(),
                folder: None
            },
        ]
    );
    let of: Vec<Option<usize>> = kept.windows.iter().map(|window| window.session).collect();
    assert_eq!(of, [Some(2), Some(1), None]);
    assert_eq!(kept.windows[1].session_index(3), 1);
    assert_eq!(kept.windows[2].session_index(3), 0);
    // Written and read, the same.
    let state = state("sessions");
    write(&state, &kept).unwrap();
    assert_eq!(read(&state).unwrap(), Some(kept.clone()));
    let _ = fs::remove_dir_all(&state);
    // Of one session of no name nor folder, none, as before sessions.
    let mut plain = Workspace::new();
    let alone = plain.open_window(pane());
    let kept = Kept::of(&plain, &[(alone, None)], None, |_| None, |_, _| None);
    assert!(kept.sessions.is_empty() && kept.windows[0].session.is_none());
    assert_eq!(kept.sessions_to_open(), [Session::default()]);
    let _ = (web, api);
}

#[test]
fn a_session_that_never_showed_keeps_its_windows_and_their_files_new_names() {
    let mut workspace = Workspace::new();
    let first = workspace.open_window(pane());
    let later = workspace.add_session("later");
    assert_eq!(workspace.active_session(), workspace.sessions()[0].id());
    let mut kept = Kept::of(&workspace, &[(first, None)], None, |_| None, |_, _| None);
    // A window kept of a pane of a file, whose name the window takes anew.
    let mut window = Window {
        session: None,
        frame: None,
        tabs: vec![Tab {
            title: None,
            color: None,
            layout: Layout::Pane {
                directory: None,
                scrollback: Some("7.scrollback".to_owned()),
            },
            active: 0,
            zoomed: None,
            pinned: false,
            group: None,
        }],
        active: 0,
        groups: Vec::new(),
    };
    let mut index = kept.panes();
    window.rename_scrollbacks(|name| {
        index += 1;
        name.map(|_| super::super::scrollback::name(index - 1))
    });
    kept.append(&workspace, vec![(later, window)]);
    assert_eq!(kept.sessions.len(), 2);
    assert_eq!(kept.sessions[1].name, "later");
    assert_eq!(kept.windows[1].session, Some(1));
    assert_eq!(
        kept.windows[1].tabs[0].layout.first_pane().scrollback,
        Some("1.scrollback")
    );
    // A name that the file holds as no plugin could write it goes.
    kept.sessions[1].name = "a\nb".to_owned();
    assert_eq!(kept.sessions_to_open()[1].name, "");
}
