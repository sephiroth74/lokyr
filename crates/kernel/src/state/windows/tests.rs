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

/// Two windows: the first of a tab of three panes, its splits of a ratio
/// and of none, the keyboard on its last pane, then a tab of a title and a
/// color of its own, which it shows; the second of one tab of two panes,
/// the first zoomed.
fn sample() -> Kept {
    let pane = |directory: &str| Layout::Pane {
        directory: Some(directory.to_owned()),
    };
    Kept {
        version: VERSION,
        windows: vec![
            Window {
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
                    },
                    Tab {
                        title: Some("logs".to_owned()),
                        color: Some("#ff8000".to_owned()),
                        layout: pane("/d"),
                        active: 0,
                        zoomed: None,
                    },
                ],
                active: 1,
            },
            Window {
                frame: None,
                tabs: vec![Tab {
                    title: None,
                    color: None,
                    layout: Layout::Split {
                        direction: Direction::Vertical,
                        ratio: Some(0.75),
                        first: Box::new(Layout::Pane { directory: None }),
                        second: Box::new(pane("/e")),
                    },
                    active: 0,
                    zoomed: Some(0),
                }],
                active: 0,
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
            layout: Layout::Pane { directory: None },
            active: 0,
            zoomed: None,
        })
        .collect();
    write(&state, &kept).unwrap();
    let Err(Unread::Invalid(error)) = read(&state) else {
        panic!("a file of too many panes reads");
    };
    assert!(error.contains("257 panes"), "{error}");
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
            .open(&mut workspace, |directory| {
                let pane = pane();
                directories.insert(pane.id(), directory.map(Path::to_path_buf));
                pane
            })
            .unwrap();
        opened.push((id, window.frame));
    }
    // Each pane spawned in its directory, in the layout's order.
    assert_eq!(directories.len(), 6);
    let again = Kept::of(&workspace, &opened, Some(opened[1].0), |pane| {
        directories.get(&pane.id()).cloned().flatten()
    });
    assert_eq!(again, kept);
    // The workspace's own: the second split's ratio is the parts' weights',
    // the keyboard on the third pane, the second tab shown.
    let window = workspace.window(opened[0].0).unwrap();
    assert_eq!(window.tabs().len(), 2);
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
    let tab = workspace.tab(window.tabs()[1]).unwrap();
    assert_eq!(tab.title(), "logs");
    assert_eq!(
        tab.color(),
        Some(Rgb {
            r: 0xff,
            g: 0x80,
            b: 0
        })
    );
    let window = workspace.window(opened[1].0).unwrap();
    let tab = workspace.tab(window.active_tab()).unwrap();
    assert_eq!(tab.zoomed(), Some(tab.layout().panes()[0]));
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
    );
    assert_eq!(kept.windows.len(), 1);
    assert_eq!(kept.front, None);
    let empty = Window {
        frame: None,
        tabs: Vec::new(),
        active: 0,
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
