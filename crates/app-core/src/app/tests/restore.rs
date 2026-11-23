//! The windows kept as the app quits, and opened again as it starts: their
//! frames, tabs, splits and directories, on a screen that is there.

use lokyr_kernel::config::RestoreWindows;
use lokyr_kernel::state::scrollback;
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
            directory: place(&a),
            scrollback: None,
            command: None,
        }
    );
    assert_eq!(
        **right,
        windows::Layout::Pane {
            directory: place(&b),
            scrollback: None,
            command: None,
        }
    );
    assert_eq!(one.tabs[1].title.as_deref(), Some("logs"));
    assert_eq!(one.tabs[1].color.as_deref(), Some("#ff8000"));
    assert_eq!(
        one.tabs[1].layout,
        windows::Layout::Pane {
            directory: place(&c),
            scrollback: None,
            command: None,
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
    let kept = Kept::new(
        frames
            .iter()
            .map(|&frame| windows::Window {
                frame,
                tabs: vec![windows::Tab {
                    title: None,
                    color: None,
                    layout: windows::Layout::Pane {
                        directory: None,
                        scrollback: None,
                        command: None,
                    },
                    active: 0,
                    zoomed: None,
                    pinned: false,
                    group: None,
                }],
                active: 0,
                groups: Vec::new(),
                session: None,
            })
            .collect(),
        None,
    );
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
    // Mostly on a screen that went, and of a size that no screen has:
    // each within the main screen. A window of no tab opens none.
    let mostly_off = windows::Frame {
        x: 1400.0,
        y: 100.0,
        width: 800.0,
        height: 600.0,
    };
    let huge = windows::Frame {
        x: -1.0e308,
        y: 100.0,
        width: 1.7e308,
        height: 1.0e300,
    };
    keep_frames(&state, &[Some(mostly_off), Some(huge)]);
    let mut kept = windows::read(&state).unwrap().unwrap();
    kept.windows.push(windows::Window {
        frame: None,
        tabs: Vec::new(),
        active: 0,
        groups: Vec::new(),
        session: None,
    });
    windows::write(&state, &kept).unwrap();
    let mut fake = Fake::default();
    let mut app = app_of(restoring(), quiet()).with_state(Some(state.clone()));
    fake.start(&mut app);
    let windows: Vec<WindowId> = fake.windows().collect();
    assert_eq!(windows.len(), 2);
    let within = |rect: ScreenRect| {
        rect.x >= SCREEN_AREA.x
            && rect.y >= SCREEN_AREA.y
            && rect.x + rect.width <= SCREEN_AREA.x + SCREEN_AREA.width
            && rect.y + rect.height <= SCREEN_AREA.y + SCREEN_AREA.height
    };
    for window in windows {
        let frame = fake.window_frame(window).unwrap();
        assert!(within(frame), "{frame:?}");
    }
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
        let notice = app.diagnostics.shown_diagnostic().expect("a notice");
        assert!(notice.text.contains("left aside"), "{}", notice.text);
        // Of its own heading, not a command's.
        assert_eq!(notice.about, About::KeptWindows);
        assert_eq!(
            app.texts.text("notice-kept-windows"),
            "The windows of the last run do not open again"
        );
        // This version's own, as it quits.
        app.quit(&mut fake);
        assert_eq!(kept(&state).version, windows::VERSION);
    }
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_windows_that_the_last_run_left_out_are_told() {
    let base = real_directory("restore-left");
    let state = base.join("state");
    keep_frames(&state, &[None]);
    let mut file = kept(&state);
    file.left = windows::Left {
        windows: 3,
        panes: 40,
    };
    windows::write(&state, &file).unwrap();
    let mut fake = Fake::default();
    let mut app = app(restoring(), quiet()).with_state(Some(state.clone()));
    fake.start(&mut app);
    // The window kept opens, and a notice tells those that do not.
    assert_eq!(fake.windows().count(), 1);
    let notice = app.diagnostics.shown_diagnostic().expect("a notice");
    assert!(
        notice
            .text
            .starts_with("3 windows of the last run, of 40 panes, do not open again"),
        "{}",
        notice.text
    );
    assert_eq!(notice.about, About::KeptWindows);
    // Kept anew as it quits, of none left out.
    app.quit(&mut fake);
    assert!(kept(&state).left.is_none());
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
    // A bench of the user's keys, which ends as its last window closes,
    // leaves them kept too, its measure's end whatever.
    let mut fake = Fake::default();
    let mut bench = app_of(restoring(), quiet())
        .with_state(Some(state.clone()))
        .with_latency_bench(0, false);
    fake.start(&mut bench);
    let window = fake.windows().next().unwrap();
    bench.close(&mut fake, window);
    assert!(fake.has_quit());
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

#[test]
fn each_pane_s_scrollback_is_kept_at_the_quit_of_its_owner_alone() {
    let base = real_directory("restore-scrollback");
    let state = base.join("state");
    for keeps in [true, false] {
        let mut config = restoring();
        config.app.restore_scrollback = keeps;
        let mut fake = Fake::default();
        let mut app = app(config, shell("printf 'a line kept\\n'; exec sleep 30"))
            .with_state(Some(state.clone()));
        fake.start(&mut app);
        run_until(&mut fake, &mut app, |_, app| {
            app.workspace.panes().any(|pane| {
                pane.terminal()
                    .is_some_and(|terminal| terminal.row_text(0).contains("a line kept"))
            })
        });
        app.quit(&mut fake);
        let kept = kept(&state);
        let windows::Layout::Pane {
            scrollback: name, ..
        } = &kept.windows[0].tabs[0].layout
        else {
            panic!("{:?}", kept.windows[0].tabs[0].layout);
        };
        if keeps {
            assert_eq!(name.as_deref(), Some("0.scrollback"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = |path: &std::path::Path| {
                    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
                };
                let folder = scrollback::folder(&state);
                assert_eq!(mode(&folder), 0o700);
                assert_eq!(mode(&folder.join("0.scrollback")), 0o600);
            }
            let kept = scrollback::take(&state, "0.scrollback").unwrap().unwrap();
            assert!(String::from_utf8_lossy(&kept.rows).contains("a line kept"));
        } else {
            // Off, the next quit keeps none, and takes those kept away.
            assert_eq!(name, &None);
            assert!(!scrollback::folder(&state).exists());
        }
    }
    let _ = std::fs::remove_dir_all(base);
}

/// The config that keeps the windows and their panes' scrollback.
fn restoring_scrollback() -> Config {
    let mut config = restoring();
    config.app.restore_scrollback = true;
    config
}

/// The rows of the screen of the first pane of `app`.
fn first_rows(app: &App) -> Vec<String> {
    let pane = app.workspace.panes().next().unwrap();
    let terminal = pane.terminal().unwrap();
    (0..terminal.rows()).map(|y| terminal.row_text(y)).collect()
}

#[test]
fn a_pane_s_scrollback_shows_again_above_its_new_program() {
    let base = real_directory("restore-shows");
    let state = base.join("state");
    // A run whose pane printed a line, kept as it quits.
    let kept_run = |config: Config| {
        let mut fake = Fake::default();
        let mut app = app(config, shell("printf 'a line kept\\n'; exec sleep 30"))
            .with_state(Some(state.clone()));
        fake.start(&mut app);
        run_until(&mut fake, &mut app, |_, app| {
            first_rows(app)
                .iter()
                .any(|row| row.contains("a line kept"))
        });
        app.quit(&mut fake);
    };
    kept_run(restoring_scrollback());
    // The next start shows it, then the line of when it was kept, then the
    // new program's output.
    let mut fake = Fake::default();
    let mut app = app(
        restoring_scrollback(),
        shell("printf 'the new run\\n'; exec sleep 30"),
    )
    .with_state(Some(state.clone()));
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |_, app| {
        first_rows(app)
            .iter()
            .any(|row| row.contains("the new run"))
    });
    let rows = first_rows(&app);
    assert_eq!(rows[0], "a line kept");
    assert!(rows[1].starts_with("── Restored from 20"), "{rows:?}");
    assert!(rows[1].ends_with('─'), "{rows:?}");
    assert_eq!(rows[2], "the new run");
    // Read once: its file went.
    assert!(!scrollback::folder(&state).join("0.scrollback").exists());
    app.quit(&mut fake);
    // Off as the app starts, what was kept goes at once, and shows not.
    kept_run(restoring_scrollback());
    let config = restoring();
    let mut fake = Fake::default();
    let mut app = app_of(config, shell("printf 'the new run\\n'; exec sleep 30"))
        .with_state(Some(state.clone()));
    fake.start(&mut app);
    assert!(!scrollback::folder(&state).exists());
    run_until(&mut fake, &mut app, |_, app| {
        first_rows(app)
            .iter()
            .any(|row| row.contains("the new run"))
    });
    assert!(
        !first_rows(&app)
            .iter()
            .any(|row| row.contains("a line kept"))
    );
    let _ = std::fs::remove_dir_all(base);
}

/// The bytes of the files in `folder`, none of a folder that is not there.
fn folder_bytes(folder: &std::path::Path) -> u64 {
    std::fs::read_dir(folder).map_or(0, |entries| {
        entries
            .flatten()
            .filter_map(|entry| entry.metadata().ok())
            .map(|metadata| metadata.len())
            .sum()
    })
}

/// The panes' scrollback's cost at the quit and at the next start, of 1,
/// 10 and 50 tabs of a pane each, empty, half full and full, of the
/// default limit and budget, `restore-scrollback` off and on, as
/// `cargo test --release -p lokyr-app-core --lib scrollback_costs --
/// --ignored --nocapture --test-threads 1` prints them: the windows' keep,
/// the next start's, the time until every pane shows its rows again, read
/// on the panes' own threads, and the files' bytes.
#[test]
#[ignore = "a measure, of a release build"]
fn scrollback_costs() {
    use lokyr_vt_core::parser::Parser;
    let base = real_directory("scrollback-costs");
    // Lines of 94 cells, of the panes' 100 columns, so that each takes a
    // row: some 12 000 fill a pane of the default limit.
    let line = |n: usize| {
        format!(
            "{n:06} \x1b[32mgreen\x1b[0m plain \x1b[1mbold\x1b[0m {}\r\n",
            "text ".repeat(14)
        )
    };
    println!("panes\tfill\tkept\tquit ms\tstart ms\trows ms\tfiles MB");
    for panes in [1_usize, 10, 50] {
        for (fill, lines) in [("empty", 0_usize), ("half", 6_000), ("full", 20_000)] {
            for keep in [false, true] {
                let state = base.join(format!("{panes}-{fill}-{keep}"));
                let mut config = restoring();
                config.app.restore_scrollback = keep;
                // A run of `panes` tabs, each of `lines` lines, kept.
                let mut fake = Fake::default();
                let mut app = app_of(config.clone(), quiet()).with_state(Some(state.clone()));
                fake.start(&mut app);
                let window = fake.windows().next().unwrap();
                fake.send(&mut app, window, WindowEvent::Focused(true));
                for _ in 1..panes {
                    app.run(&mut fake, "tabs.new", Args::new());
                }
                run_until(&mut fake, &mut app, |_, app| {
                    app.workspace.panes().count() == panes
                });
                let mut parser = Parser::new();
                for pane in app.workspace.panes() {
                    let mut terminal = pane.terminal().unwrap();
                    for n in 0..lines {
                        parser.advance(&mut *terminal, line(n).as_bytes());
                    }
                }
                let started = Instant::now();
                app.keep_windows(&fake);
                let quit = started.elapsed();
                drop(app);
                let files = folder_bytes(&scrollback::folder(&state));
                // The next start, of the windows kept.
                let mut fake = Fake::default();
                let mut app = app_of(config, quiet()).with_state(Some(state.clone()));
                let started = Instant::now();
                fake.start(&mut app);
                let start = started.elapsed();
                assert_eq!(app.workspace.panes().count(), panes);
                // Each pane's rows, then the line of when they were kept.
                let shown = |app: &App| {
                    app.workspace.panes().all(|pane| {
                        let terminal = pane.terminal().unwrap();
                        (0..terminal.rows())
                            .any(|y| terminal.row_text(y).starts_with("── Restored from"))
                    })
                };
                let rows = if keep && lines > 0 {
                    while !shown(&app) {
                        assert!(started.elapsed().as_secs() < 60, "the rows never came");
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    format!("{:.1}", started.elapsed().as_secs_f64() * 1000.0)
                } else {
                    "-".to_owned()
                };
                println!(
                    "{panes}\t{fill}\t{keep}\t{:.1}\t{:.1}\t{rows}\t{:.2}",
                    quit.as_secs_f64() * 1000.0,
                    start.as_secs_f64() * 1000.0,
                    files as f64 / 1e6
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(base);
}

/// The rows of the screen of the pane of `window`, a window of a pane.
fn rows_of(app: &App, window: WindowId) -> Vec<String> {
    let shown = app.windows[&window].window;
    let pane = app.workspace.focused(shown).unwrap();
    let terminal = pane.terminal().unwrap();
    (0..terminal.rows()).map(|y| terminal.row_text(y)).collect()
}

/// The names of `app`'s sessions, in the order of their use.
fn session_names(app: &App) -> Vec<String> {
    app.workspace
        .sessions()
        .iter()
        .map(|session| session.name().to_owned())
        .collect()
}

/// The session of `app` of the name `name`.
fn session_of(app: &App, name: &str) -> workspace::SessionId {
    app.workspace
        .sessions()
        .iter()
        .find(|session| session.name() == name)
        .map(|session| session.id())
        .unwrap()
}

#[test]
fn every_session_comes_back_the_hidden_ones_opening_as_they_first_show() {
    let base = real_directory("restore-sessions");
    let state = base.join("state");
    // A run of three sessions: home of two windows, api, then web, active,
    // each window's pane of a line of its own.
    let mut fake = Fake::default();
    let mut app = app(
        restoring_scrollback(),
        shell("printf 'kept in %s\\n' \"$LOKYR_TEST_MARK\"; exec sleep 30"),
    )
    .with_state(Some(state.clone()));
    fake.start(&mut app);
    let home = app.workspace.active_session();
    app.workspace.rename_session(home, "home");
    fake.send_app(&mut app, AppEvent::Command("window.new".to_owned()));
    for name in ["api", "web"] {
        fake.send_app(&mut app, AppEvent::Command("window.new-session".to_owned()));
        let session = app.workspace.active_session();
        app.workspace.rename_session(session, name);
    }
    assert_eq!(fake.windows().count(), 4);
    run_until(&mut fake, &mut app, |_, app| {
        app.windows.keys().all(|&window| {
            rows_of(app, window)
                .iter()
                .any(|row| row.starts_with("kept"))
        })
    });
    app.quit(&mut fake);
    // The next start: every session, the one that was active shown, of its
    // window alone; the others' windows open as each first shows.
    let started = |state: &std::path::Path| {
        let mut fake = Fake::default();
        let mut app = super::app(restoring_scrollback(), shell("exec sleep 30"))
            .with_state(Some(state.to_path_buf()));
        fake.start(&mut app);
        (fake, app)
    };
    let (mut fake, mut app) = started(&state);
    assert_eq!(session_names(&app), ["web", "api", "home"]);
    assert_eq!(fake.windows().count(), 1);
    let api = session_of(&app, "api");
    assert!(app.workspace.session_windows(api).is_empty());
    let named = |name: &str| {
        lokyr_kernel::commands::Args::from([(
            "name".to_owned(),
            lokyr_kernel::commands::Value::String(name.to_owned()),
        )])
    };
    app.run(&mut fake, "window.switch-session", named("home"));
    assert_eq!(fake.windows().count(), 3);
    let home = session_of(&app, "home");
    assert_eq!(app.workspace.session_windows(home).len(), 2);
    for window in app.workspace.session_windows(home) {
        let (&shown, _) = app
            .windows
            .iter()
            .find(|(_, state)| state.window == window)
            .unwrap();
        assert!(rows_of(&app, shown)[0].starts_with("kept in"));
        assert!(!fake.window(shown).unwrap().hidden);
    }
    // Quit with api never shown: its window is kept as it was, of its
    // scrollback, and shows it as api first shows after the next start.
    app.quit(&mut fake);
    let (mut fake, mut app) = started(&state);
    assert_eq!(session_names(&app), ["home", "web", "api"]);
    assert_eq!(fake.windows().count(), 2);
    app.run(&mut fake, "window.switch-session", named("api"));
    let api = session_of(&app, "api");
    let [window] = app.workspace.session_windows(api)[..] else {
        panic!("one window of api");
    };
    let (&shown, _) = app
        .windows
        .iter()
        .find(|(_, state)| state.window == window)
        .unwrap();
    assert!(rows_of(&app, shown)[0].starts_with("kept in"));
    app.quit(&mut fake);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn a_split_s_panes_show_their_restored_lines_on_one_row() {
    let base = real_directory("restore-split-lines");
    let state = base.join("state");
    // A window of two panes side by side, each of its file.
    let mut kept_rows =
        lokyr_vt_core::terminal::Terminal::new(40, 10, lokyr_vt_core::terminal::Options::default());
    lokyr_vt_core::parser::Parser::new().advance(&mut kept_rows, b"a line kept\r\n");
    let bytes = kept_rows.archive(usize::MAX);
    scrollback::write(
        &state,
        &[
            (scrollback::name(0), bytes.clone()),
            (scrollback::name(1), bytes),
        ],
    )
    .unwrap();
    let pane = |name: usize| windows::Layout::Pane {
        directory: None,
        scrollback: Some(scrollback::name(name)),
        command: None,
    };
    let kept = Kept::new(
        vec![windows::Window {
            frame: None,
            tabs: vec![windows::Tab {
                title: None,
                color: None,
                layout: windows::Layout::Split {
                    direction: windows::Direction::Horizontal,
                    ratio: None,
                    first: Box::new(pane(0)),
                    second: Box::new(pane(1)),
                },
                active: 0,
                zoomed: None,
                pinned: false,
                group: None,
            }],
            active: 0,
            groups: Vec::new(),
            session: None,
        }],
        None,
    );
    windows::write(&state, &kept).unwrap();
    let mut fake = Fake::default();
    let mut app = app(restoring_scrollback(), shell("exec sleep 30")).with_state(Some(state));
    fake.start(&mut app);
    let line_of = |app: &App, pane: PaneId| {
        let terminal = app.workspace.pane(pane)?.terminal()?;
        let rows: Vec<String> = (0..terminal.rows()).map(|y| terminal.row_text(y)).collect();
        let at = rows
            .iter()
            .position(|row| row.starts_with("── Restored from"))?;
        Some((
            rows[at].clone(),
            rows.get(at + 1).cloned().unwrap_or_default(),
        ))
    };
    let panes: Vec<PaneId> = app.workspace.pane_ids();
    assert_eq!(panes.len(), 2);
    run_until(&mut fake, &mut app, |_, app| {
        panes.iter().all(|&pane| line_of(app, pane).is_some())
    });
    for &pane in &panes {
        let (line, next) = line_of(&app, pane).unwrap();
        // Of its own length, its row alone, whatever the pane's width.
        assert!(line.ends_with(" ──"), "{line:?}");
        assert!(!next.starts_with('─'), "{next:?}");
    }
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn the_panes_files_go_of_a_windows_file_that_cannot_be_written() {
    let base = real_directory("restore-failed-keep");
    let state = base.join("state");
    // A folder where the windows' file goes, which no write replaces.
    std::fs::create_dir_all(state.join("windows.json")).unwrap();
    let mut fake = Fake::default();
    let mut app = app(
        restoring_scrollback(),
        shell("printf 'a line kept\\n'; exec sleep 30"),
    )
    .with_state(Some(state.clone()));
    fake.start(&mut app);
    run_until(&mut fake, &mut app, |_, app| {
        first_rows(app)
            .iter()
            .any(|row| row.contains("a line kept"))
    });
    app.quit(&mut fake);
    assert!(!scrollback::folder(&state).exists());
    let _ = std::fs::remove_dir_all(base);
}
