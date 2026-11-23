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
        command: None,
    };
    Kept {
        sessions: Vec::new(),
        left: Left::default(),
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
                        floating: Vec::new(),
                        floating_shown: true,
                    },
                    Tab {
                        title: Some("logs".to_owned()),
                        color: Some("#ff8000".to_owned()),
                        layout: pane("/d"),
                        active: 0,
                        zoomed: None,
                        pinned: false,
                        group: Some(0),
                        floating: Vec::new(),
                        floating_shown: true,
                    },
                    Tab {
                        title: None,
                        color: None,
                        layout: pane("/f"),
                        active: 0,
                        zoomed: None,
                        pinned: false,
                        group: Some(0),
                        floating: Vec::new(),
                        floating_shown: true,
                    },
                    Tab {
                        title: None,
                        color: None,
                        layout: pane("/g"),
                        active: 0,
                        zoomed: None,
                        pinned: false,
                        group: Some(1),
                        floating: Vec::new(),
                        floating_shown: true,
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
                            command: None,
                        }),
                        second: Box::new(pane("/e")),
                    },
                    active: 0,
                    zoomed: Some(0),
                    pinned: false,
                    group: None,
                    floating: Vec::new(),
                    floating_shown: true,
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
                command: None,
            },
            active: 0,
            zoomed: None,
            pinned: false,
            group: None,
            floating: Vec::new(),
            floating_shown: true,
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
    // More sessions than windows may open.
    let mut kept = sample();
    kept.sessions = vec![Session::default(); MOST_WINDOWS + 1];
    write(&state, &kept).unwrap();
    let Err(Unread::Invalid(error)) = read(&state) else {
        panic!("a file of too many sessions reads");
    };
    assert!(error.contains("101 sessions"), "{error}");
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
            scrollback: None,
            command: None,
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
    let window = Window {
        session: None,
        frame: None,
        tabs: vec![Tab {
            title: None,
            color: None,
            layout: Layout::Pane {
                directory: None,
                scrollback: Some("7.scrollback".to_owned()),
                command: None,
            },
            active: 0,
            zoomed: None,
            pinned: false,
            group: None,
            floating: Vec::new(),
            floating_shown: true,
        }],
        active: 0,
        groups: Vec::new(),
    };
    let mut index = kept.panes();
    kept.append(&workspace, vec![(later, window)], |window| {
        window.rename_scrollbacks(|name| {
            index += 1;
            name.map(|_| super::super::scrollback::name(index - 1))
        });
    });
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

/// A pane of a program that does not start, of no process, as a test of
/// hundreds of panes makes them.
fn idle() -> Pane {
    let program = Program::command("/lokyr/no-such-program", ["a"]);
    Pane::spawn(
        &program,
        &Environment::default(),
        &Settings::default(),
        Grid {
            columns: 20,
            rows: 4,
            cell_width: 8,
            cell_height: 16,
        },
        &Colors::default(),
        &Texts::new("en", &crate::l10n::KERNEL),
        || {},
    )
}

/// A window of `workspace` of `panes` tabs, each of one pane.
fn window_of(workspace: &mut Workspace, panes: usize) -> WindowId {
    let window = workspace.open_window(idle());
    for index in 1..panes {
        workspace.open_tab(window, idle(), index).unwrap();
    }
    window
}

#[test]
fn sessions_past_the_bounds_keep_within_them_the_active_one_whole() {
    let mut workspace = Workspace::new();
    let home = workspace.active_session();
    assert!(workspace.rename_session(home, "home"));
    let h = window_of(&mut workspace, 10);
    let api = workspace.new_session("api").unwrap();
    let (a1, a2) = (
        window_of(&mut workspace, 200),
        window_of(&mut workspace, 50),
    );
    let web = workspace.new_session("web").unwrap();
    let (w1, w2) = (
        window_of(&mut workspace, 100),
        window_of(&mut workspace, 100),
    );
    let shown: Vec<_> = [h, a1, a2, w1, w2]
        .into_iter()
        .map(|window| (window, None))
        .collect();
    let mut names = Vec::new();
    let kept = Kept::of(
        &workspace,
        &shown,
        Some(w2),
        |_| None,
        |_, index| {
            names.push(index);
            None
        },
    );
    // The active session's windows whole, then the one before it's that
    // fit, of 256 panes; the oldest's left out, and its session with it.
    let panes: Vec<usize> = kept
        .windows
        .iter()
        .map(|window| window.tabs.len())
        .collect();
    assert_eq!(panes, [50, 100, 100]);
    assert_eq!(kept.panes(), MOST_PANES - 6);
    assert_eq!(
        kept.left,
        Left {
            windows: 2,
            panes: 210
        }
    );
    let sessions: Vec<&str> = kept
        .sessions
        .iter()
        .map(|session| session.name.as_str())
        .collect();
    assert_eq!(sessions, ["web", "api"]);
    assert_eq!(kept.front, Some(2));
    // The panes' files named of the kept panes alone, in their order.
    assert_eq!(names, (0..kept.panes()).collect::<Vec<_>>());
    // A session that never showed: its windows while they fit.
    let later = workspace.add_session("later");
    let pending = |panes: usize| Window {
        session: None,
        frame: None,
        tabs: (0..panes)
            .map(|_| Tab {
                title: None,
                color: None,
                layout: Layout::Pane {
                    directory: None,
                    scrollback: None,
                    command: None,
                },
                active: 0,
                zoomed: None,
                pinned: false,
                group: None,
                floating: Vec::new(),
                floating_shown: true,
            })
            .collect(),
        active: 0,
        groups: Vec::new(),
    };
    let mut kept = kept;
    let mut taken = Vec::new();
    kept.append(
        &workspace,
        vec![(later, pending(10)), (later, pending(6))],
        |window| taken.push(window.tabs.len()),
    );
    assert_eq!(taken, [6]);
    assert_eq!(kept.panes(), MOST_PANES);
    assert_eq!(
        kept.left,
        Left {
            windows: 3,
            panes: 220
        }
    );
    // Written and read, what was left out too, which the next start tells.
    let state = state("bounds");
    write(&state, &kept).unwrap();
    assert_eq!(read(&state).unwrap(), Some(kept.clone()));
    let _ = fs::remove_dir_all(&state);
    // Past the windows' bound, the oldest session's go first.
    let mut many = Workspace::new();
    let old = window_of(&mut many, 1);
    many.new_session("new").unwrap();
    let mut shown = vec![(old, None)];
    shown.extend((0..MOST_WINDOWS).map(|_| (window_of(&mut many, 1), None)));
    let kept = Kept::of(&many, &shown, None, |_| None, |_, _| None);
    assert_eq!(kept.windows.len(), MOST_WINDOWS);
    assert_eq!(
        kept.left,
        Left {
            windows: 1,
            panes: 1
        }
    );
    assert_eq!(kept.sessions.len(), 1);
    let _ = (api, web);
}

#[test]
fn a_tab_keeps_its_floating_panes_their_frames_and_whether_they_show() {
    let mut workspace = Workspace::new();
    let window = workspace.open_window(pane());
    let tab = workspace.window(window).unwrap().active_tab();
    let floating = workspace
        .float_pane(
            tab,
            pane(),
            workspace::FloatingFrame::new(0.1, 0.2, 0.5, 0.4),
        )
        .unwrap();
    let mut names = Vec::new();
    let kept = Kept::of(
        &workspace,
        &[(window, None)],
        Some(window),
        |_| None,
        |_, index| {
            names.push(index);
            Some(super::super::scrollback::name(index))
        },
    );
    // Its layout's pane, then the floating one, as the files are named and
    // the keyboard's index counts.
    assert_eq!(names, [0, 1]);
    let held = &kept.windows[0].tabs[0];
    assert_eq!(held.active, 1);
    assert!(held.floating_shown);
    assert_eq!(held.floating.len(), 1);
    let kept_floating = &held.floating[0];
    assert_eq!(
        (
            kept_floating.x,
            kept_floating.y,
            kept_floating.width,
            kept_floating.height
        ),
        (0.1, 0.2, 0.5, 0.4)
    );
    assert_eq!(kept.panes(), 2);
    // Read back the same, and opened as it was, its floating pane hidden
    // as the tab kept it.
    let state = state("floating");
    let mut hidden = kept.clone();
    hidden.windows[0].tabs[0].floating_shown = false;
    hidden.windows[0].tabs[0].active = 0;
    write(&state, &hidden).unwrap();
    let read = read(&state).unwrap().unwrap();
    assert_eq!(read, hidden);
    let _ = fs::remove_dir_all(&state);
    let mut again = Workspace::new();
    let opened = read.windows[0].open(&mut again, |_| pane()).unwrap();
    let tab = again.window(opened).unwrap().active_tab();
    let held = again.tab(tab).unwrap();
    assert_eq!(held.panes().len(), 2);
    assert_eq!(held.floating().len(), 1);
    assert!(!held.floating_shown());
    let frame = held.floating()[0].frame;
    assert_eq!(
        (frame.x(), frame.y(), frame.width(), frame.height()),
        (0.1, 0.2, 0.5, 0.4)
    );
    assert_eq!(held.active_pane(), held.layout().panes()[0]);
    let _ = floating;
}
