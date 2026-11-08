//! The windows kept as the app quits, and opened again as it starts: their
//! frames, tabs, splits and directories, on a screen that is there.

use lokyr_kernel::config::RestoreWindows;
use lokyr_kernel::state::windows::{self, Kept};
use lokyr_kernel::workspace::Ratio;
use lokyr_platform_core::ScreenRect;
use lokyr_platform_core::fake::SCREEN_AREA;
use lokyr_vt_core::page::Rgb;

use super::*;

/// The config that keeps the windows, whatever the system's settings say.
fn restoring() -> Config {
    let mut config = Config::default();
    config.app.restore_windows = RestoreWindows::Always;
    config
}

/// A program that waits in the directory that it started in.
fn waiting() -> Program {
    shell("exec sleep 30")
}

/// The panes of the tab that `window` shows, in its layout's order.
fn shown_panes(app: &App, window: WindowId) -> Vec<PaneId> {
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    app.workspace
        .tab(shown.active_tab())
        .unwrap()
        .layout()
        .panes()
}

/// Splits the pane that has `window`'s keyboard, towards `direction`.
fn split(fake: &mut Fake, app: &mut App, window: WindowId, direction: &str) {
    fake.send(app, window, WindowEvent::Focused(true));
    let before = shown_panes(app, window).len();
    let args = Args::from([(
        "direction".to_owned(),
        lokyr_kernel::commands::Value::String(direction.to_owned()),
    )]);
    app.run(fake, "splits.split", args);
    run_until(fake, app, |_, app| {
        shown_panes(app, window).len() == before + 1
    });
}

/// Waits until each pane's program works in a directory, which the app
/// keeps of it.
fn all_working(fake: &mut Fake, app: &mut App) {
    run_until(fake, app, |_, app| {
        app.workspace
            .panes()
            .all(|pane| pane.working_directory().is_some())
    });
}

/// The windows that `state` keeps.
fn kept(state: &std::path::Path) -> Kept {
    windows::read(state).unwrap().expect("the windows kept")
}

#[test]
fn two_windows_of_tabs_and_splits_open_again_as_they_were() {
    let base = real_directory("restore");
    let state = base.join("state");
    let directory = |name: &str| {
        let made = base.join(name);
        std::fs::create_dir(&made).unwrap();
        made
    };
    let (a, b, c, d, e) = (
        directory("a"),
        directory("b"),
        directory("c"),
        directory("d"),
        directory("e"),
    );
    let mut config = restoring();
    config.window.start_directory = StartDirectory::Path(a.display().to_string());
    let mut fake = Fake::default();
    let mut app = app(config.clone(), waiting()).with_state(Some(state.clone()));
    fake.start(&mut app);
    let first = fake.windows().next().unwrap();
    // The first window's tab: `a` beside `b`, of a third of the room.
    app.config.window.new_directory = NewDirectory::Path(b.display().to_string());
    split(&mut fake, &mut app, first, "right");
    let tab = app
        .workspace
        .window(app.windows[&first].window)
        .unwrap()
        .active_tab();
    app.workspace.set_ratio(tab, &[], Some(Ratio::new(0.3)));
    // Its second tab, in `c`, of a title and a color of its own.
    app.config.window.new_directory = NewDirectory::Path(c.display().to_string());
    app.run(&mut fake, "tabs.new", Args::new());
    let tabs = |app: &App| {
        app.workspace
            .window(app.windows[&first].window)
            .unwrap()
            .tabs()
            .to_vec()
    };
    run_until(&mut fake, &mut app, |_, app| tabs(app).len() == 2);
    let second_tab = tabs(&app)[1];
    app.workspace
        .set_tab_title(second_tab, Some("logs".to_owned()));
    let orange = Rgb {
        r: 0xff,
        g: 0x80,
        b: 0,
    };
    app.workspace.set_tab_color(second_tab, Some(orange));
    // The second window: `d` over `e`, `d` zoomed; it has the focus as
    // the app quits.
    app.config.window.new_directory = NewDirectory::Path(d.display().to_string());
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let second = fake.windows().find(|&window| window != first).unwrap();
    app.config.window.new_directory = NewDirectory::Path(e.display().to_string());
    split(&mut fake, &mut app, second, "down");
    let zoomed = shown_panes(&app, second)[0];
    let second_shown = app.windows[&second].window;
    let tab = app.workspace.window(second_shown).unwrap().active_tab();
    app.workspace.zoom(tab, Some(zoomed));
    fake.send(&mut app, second, WindowEvent::Focused(true));
    all_working(&mut fake, &mut app);
    let frames = (
        fake.window_frame(first).unwrap(),
        fake.window_frame(second).unwrap(),
    );
    app.quit(&mut fake);
    assert!(fake.has_quit());

    // What the file keeps: the windows in the order of their numbers.
    let before = kept(&state);
    assert_eq!(before.windows.len(), 2);
    assert_eq!(before.front, Some(1));
    let [one, two] = &before.windows[..] else {
        panic!("{before:?}");
    };
    let frame_of = |frame: ScreenRect| windows::Frame {
        x: frame.x,
        y: frame.y,
        width: frame.width,
        height: frame.height,
    };
    assert_eq!(one.frame, Some(frame_of(frames.0)));
    assert_eq!(two.frame, Some(frame_of(frames.1)));
    assert_eq!((one.tabs.len(), one.active), (2, 1));
    let place = |path: &PathBuf| Some(path.display().to_string());
    let windows::Layout::Split {
        direction,
        ratio,
        first: left,
        second: right,
    } = &one.tabs[0].layout
    else {
        panic!("{one:?}");
    };
    assert_eq!(*direction, windows::Direction::Horizontal);
    assert_eq!(*ratio, Some(0.3));
    assert_eq!(
        **left,
        windows::Layout::Pane {
            directory: place(&a)
        }
    );
    assert_eq!(
        **right,
        windows::Layout::Pane {
            directory: place(&b)
        }
    );
    assert_eq!(one.tabs[1].title.as_deref(), Some("logs"));
    assert_eq!(one.tabs[1].color.as_deref(), Some("#ff8000"));
    assert_eq!(
        one.tabs[1].layout,
        windows::Layout::Pane {
            directory: place(&c)
        }
    );
    assert_eq!(two.tabs[0].zoomed, Some(0));

    // The next start opens them again: each at its frame, the second in
    // front, each pane in its directory, which the app keeps again as it
    // quits.
    let mut fake = Fake::default();
    let mut app = app_of(config, waiting()).with_state(Some(state.clone()));
    fake.start(&mut app);
    assert_eq!(fake.windows().count(), 2);
    let restored: Vec<WindowId> = fake.windows().collect();
    assert_eq!(fake.window_frame(restored[0]), Some(frames.0));
    assert_eq!(fake.window_frame(restored[1]), Some(frames.1));
    assert_eq!(fake.focus_requests().last(), Some(&restored[1]));
    fake.send(&mut app, restored[1], WindowEvent::Focused(true));
    all_working(&mut fake, &mut app);
    app.quit(&mut fake);
    assert_eq!(kept(&state), before);
    let _ = std::fs::remove_dir_all(base);
}

/// The app, of `config`, whose windows run `program`, as [`app`] makes
/// it, where a local of that name hides it.
fn app_of(config: Config, program: Program) -> App {
    app(config, program)
}

/// Keeps, in `state`, windows of a pane each at `frames`.
fn keep_frames(state: &std::path::Path, frames: &[Option<windows::Frame>]) {
    let kept = Kept {
        version: windows::VERSION,
        windows: frames
            .iter()
            .map(|&frame| windows::Window {
                frame,
                tabs: vec![windows::Tab {
                    title: None,
                    color: None,
                    layout: windows::Layout::Pane { directory: None },
                    active: 0,
                    zoomed: None,
                }],
                active: 0,
            })
            .collect(),
        front: None,
    };
    windows::write(state, &kept).unwrap();
}

#[test]
fn a_window_of_a_display_that_went_opens_on_one_that_is_there() {
    let base = real_directory("restore-display");
    let state = base.join("state");
    // The first on the main screen; the second on one to its right, which
    // went; the third of no size that a window can take.
    let on_main = windows::Frame {
        x: 300.0,
        y: 200.0,
        width: 640.0,
        height: 480.0,
    };
    let on_gone = windows::Frame {
        x: 2000.0,
        y: 100.0,
        width: 800.0,
        height: 600.0,
    };
    let empty = windows::Frame {
        width: 0.0,
        ..on_main
    };
    keep_frames(&state, &[Some(on_main), Some(on_gone), Some(empty)]);
    let mut fake = Fake::default();
    let mut app = app(restoring(), quiet()).with_state(Some(state.clone()));
    fake.start(&mut app);
    let windows: Vec<WindowId> = fake.windows().collect();
    assert_eq!(windows.len(), 3);
    let frame = |window| fake.window_frame(window).unwrap();
    assert_eq!(
        frame(windows[0]),
        ScreenRect {
            x: 300.0,
            y: 200.0,
            width: 640.0,
            height: 480.0,
        }
    );
    // At the main screen's right edge, of its height.
    assert_eq!(
        frame(windows[1]),
        ScreenRect {
            x: SCREEN_AREA.width - 800.0,
            y: 100.0,
            width: 800.0,
            height: 600.0,
        }
    );
    // Where the platform places a new one, of the config's grid.
    assert_ne!(frame(windows[2]).width, 0.0);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_newer_or_unreadable_file_is_left_aside_with_a_notice() {
    let base = real_directory("restore-aside");
    let state = base.join("state");
    for text in [r#"{"version":99,"windows":"another shape"}"#, "{not json"] {
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("windows.json"), text).unwrap();
        let mut fake = Fake::default();
        let mut app = app(restoring(), quiet()).with_state(Some(state.clone()));
        fake.start(&mut app);
        // A new window, as without the file, and a notice of why.
        assert_eq!(fake.windows().count(), 1, "{text}");
        let notice = app.diagnostics.shown().expect("a notice");
        assert!(notice.text.contains("left aside"), "{}", notice.text);
        // Of its own heading, not a command's.
        assert_eq!(notice.about, About::KeptWindows);
        assert_eq!(
            app.texts.text("notice-kept-windows"),
            "The windows of the last run do not open again:"
        );
        // This version's own, as it quits.
        app.quit(&mut fake);
        assert_eq!(kept(&state).version, windows::VERSION);
    }
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_windows_open_again_as_the_config_and_the_system_s_setting_say() {
    let base = real_directory("restore-setting");
    let state = base.join("state");
    let frame = Some(windows::Frame {
        x: 300.0,
        y: 200.0,
        width: 640.0,
        height: 480.0,
    });
    for (restore, system, opened) in [
        (RestoreWindows::System, false, 1),
        (RestoreWindows::System, true, 2),
        (RestoreWindows::Always, false, 2),
        (RestoreWindows::Never, true, 1),
    ] {
        keep_frames(&state, &[frame, frame]);
        let mut config = Config::default();
        config.app.restore_windows = restore;
        let mut fake = Fake::default();
        fake.set_keeps_windows(system);
        let mut app = app(config, quiet()).with_state(Some(state.clone()));
        fake.start(&mut app);
        assert_eq!(fake.windows().count(), opened, "{restore:?} {system}");
        // As it quits, it keeps them, or forgets those kept.
        app.quit(&mut fake);
        let kept = windows::read(&state).unwrap();
        assert_eq!(
            kept.map(|kept| kept.windows.len()),
            (opened == 2).then_some(2),
            "{restore:?} {system}"
        );
    }
    // Quitting without windows forgets them too.
    keep_frames(&state, &[frame]);
    let mut fake = Fake::default();
    let mut app = app(restoring(), quiet()).with_state(Some(state.clone()));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.close(&mut fake, window);
    app.quit(&mut fake);
    assert!(windows::read(&state).unwrap().is_none());
    // A bench's window is its own: it opens none of them, and leaves
    // them kept as it quits.
    keep_frames(&state, &[frame, frame]);
    let mut fake = Fake::default();
    let mut bench = app_of(restoring(), quiet())
        .with_state(Some(state.clone()))
        .with_latency_bench(10, false);
    fake.start(&mut bench);
    assert_eq!(fake.windows().count(), 1);
    bench.quit(&mut fake);
    assert_eq!(
        windows::read(&state)
            .unwrap()
            .map(|kept| kept.windows.len()),
        Some(2)
    );
    // The last window's close that quits, as the config says, forgets
    // them too.
    let mut config = restoring();
    config.app.quit_after_last_window_closed = true;
    let mut fake = Fake::default();
    let mut app = app_of(config, quiet()).with_state(Some(state.clone()));
    fake.start(&mut app);
    for window in fake.windows().collect::<Vec<_>>() {
        app.close(&mut fake, window);
    }
    assert!(fake.has_quit());
    assert!(windows::read(&state).unwrap().is_none());
    let _ = std::fs::remove_dir_all(base);
}
