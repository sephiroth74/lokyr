//! A layout's file read, written and opened as the app's windows, and its
//! errors, each at its line.

use super::*;

/// The complete example of the page of the layouts.
fn example() -> String {
    let page = include_str!("../../../../docs/layouts.md");
    let start = page
        .find("```toml\n# The session's folder")
        .expect("the page's example");
    let body = &page[start + "```toml\n".len()..];
    body[..body.find("```").expect("the example's end")].to_owned()
}

#[test]
fn the_page_s_example_reads_and_opens_as_its_windows() {
    let layout = Layout::parse(&example()).unwrap();
    assert_eq!(layout.folder, Some(Folder("~/src/api".to_owned())));
    let windows = layout.windows();
    assert_eq!(windows.len(), 2);
    let first = &windows[0];
    assert_eq!(
        first.frame,
        Some(windows::Frame {
            x: 80.0,
            y: 60.0,
            width: 1200.0,
            height: 800.0,
        })
    );
    // The window shows its second tab, the logs'.
    assert_eq!(first.active, 1);
    let server = &first.tabs[0];
    assert_eq!(server.title.as_deref(), Some("server"));
    assert_eq!(server.color.as_deref(), Some("#3478f6"));
    // Its second pane has the keyboard.
    assert_eq!(server.active, 1);
    let windows::Layout::Split {
        direction,
        ratio,
        first: left,
        ..
    } = &server.layout
    else {
        panic!("a split: {:?}", server.layout);
    };
    assert_eq!(*direction, windows::Direction::Horizontal);
    assert_eq!(*ratio, Some(0.6));
    assert_eq!(left.first_pane().command, Some("cargo run"));
    assert_eq!(left.first_pane().directory, Some(Path::new("~/src/api")));
    assert_eq!(
        first.tabs[1].layout.first_pane().command,
        Some("tail -f log/development.log")
    );
    assert!(windows[1].tabs[0].pinned);
    assert_eq!(windows[1].frame, None);
}

#[test]
fn a_session_s_windows_write_a_layout_that_reads_back_the_same() {
    let example = Layout::parse(&example()).unwrap();
    // As the app keeps them, of no command, which a layout's own adds.
    let mut kept = example.windows();
    for window in &mut kept {
        for tab in &mut window.tabs {
            strip_commands(&mut tab.layout);
        }
    }
    let layout = Layout::of(&kept, Some("~/src/api"));
    let text = layout.text();
    assert!(text.starts_with("# A layout of Lokyr's"), "{text}");
    let read = Layout::parse(&text).unwrap();
    assert_eq!(read, layout);
    assert_eq!(read.windows(), kept);
    // Written to a file, and its name listed.
    let folder = std::env::temp_dir().join(format!("lokyr-layouts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&folder);
    layout.write(&path(&folder, "api")).unwrap();
    fs::write(folder.join(".hidden.toml"), "").unwrap();
    fs::write(folder.join("notes.txt"), "").unwrap();
    assert_eq!(names(&folder), ["api"]);
    assert_eq!(Layout::read(&path(&folder, "api")).unwrap(), layout);
    let _ = fs::remove_dir_all(&folder);
}

/// Takes the commands of `layout`'s panes away.
fn strip_commands(layout: &mut windows::Layout) {
    match layout {
        windows::Layout::Pane { command, .. } => *command = None,
        windows::Layout::Split { first, second, .. } => {
            strip_commands(first);
            strip_commands(second);
        }
    }
}

#[test]
fn what_does_not_read_is_told_of_its_line() {
    let line_of = |text: &str| Layout::parse(text).unwrap_err().position.map(|at| at.line);
    let cases = [
        // A key that a layout lacks.
        (
            "[[window]]\n[[window.tab]]\nshape = 1\n[window.tab.pane]\n",
            Some(3),
        ),
        // A ratio out of its range.
        (
            "[[window]]\n[[window.tab]]\n[window.tab.split]\ndirection = \"right\"\nratio = 1.5\n[window.tab.split.first.pane]\n[window.tab.split.second.pane]\n",
            Some(5),
        ),
        // A color that is not one.
        (
            "[[window]]\n[[window.tab]]\ncolor = \"blue\"\n[window.tab.pane]\n",
            Some(3),
        ),
        // A folder of no `/` nor `~`.
        (
            "[[window]]\n[[window.tab]]\n[window.tab.pane]\nfolder = \"src\"\n",
            Some(4),
        ),
        // A command of two lines.
        (
            "[[window]]\n[[window.tab]]\n[window.tab.pane]\ncommand = \"a\\nb\"\n",
            Some(4),
        ),
        // Not TOML at all.
        ("[[window]\n", Some(1)),
    ];
    for (text, line) in cases {
        assert_eq!(line_of(text), line, "{text}");
    }
    // A tab of both a pane and a split, of neither, or a window of no tab.
    for text in [
        "[[window]]\n[[window.tab]]\n[window.tab.pane]\n[window.tab.split]\ndirection = \"down\"\n[window.tab.split.first.pane]\n[window.tab.split.second.pane]\n",
        "[[window]]\n[[window.tab]]\ntitle = \"x\"\n",
        "[[window]]\nframe = { x = 0, y = 0, width = 10, height = 10 }\n",
    ] {
        let error = Layout::parse(text).unwrap_err();
        assert!(error.position.is_some(), "{text}: {error}");
    }
    // Of no window.
    let error = Layout::parse("folder = \"~/src\"\n").unwrap_err();
    assert_eq!(error.message, "a layout has one `[[window]]` at least");
}
