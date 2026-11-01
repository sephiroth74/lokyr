//! The tests of the notices, the bell and the programs' notifications.

use super::*;

/// A program that rings its bell at each key, twice the second time
/// within a fifth of a second.
fn ringer() -> Program {
    shell(
        "stty raw -echo; echo ready; while :; do head -c 1 >/dev/null; printf '\\a'; sleep 0.05; printf '\\a'; done",
    )
}

/// A program that sends a notification of OSC 9 and one of OSC 777 at
/// each key.
fn notifier() -> Program {
    shell(
        "stty raw -echo; printf '\\033]2;agent\\a'; echo ready; while :; do head -c 1 >/dev/null; printf '\\033]9;waiting\\a'; printf '\\033]777;notify;Done;all tests pass\\a'; done",
    )
}

#[test]
fn an_invalid_config_shows_its_notice_until_one_that_reads() {
    let (mut fake, mut app, path) = with_file("notice", "[font]\nsize = 14\n");
    let window = fake.windows().next().unwrap();
    assert_eq!(app.diagnostics.shown(), None);
    // The error, of its line, and nothing of the file applies.
    std::fs::write(&path, "[font]\nsize = 15\nbogus = 1\n").unwrap();
    fake.send(&mut app, window, WindowEvent::Frame);
    change(&mut fake, &mut app);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(
        (shown.code, &shown.about),
        (Code::InvalidConfig, &About::Config)
    );
    assert!(shown.text.contains("config.toml:3:1: "), "{}", shown.text);
    assert!(shown.text.ends_with("and the one in force stays"));
    assert_eq!(app.config.font.size.points(), 14.0);
    assert!(fake.window(window).unwrap().frame_requested);
    // Another error takes its place; a config that reads takes it away.
    std::fs::write(&path, "[font]\nsize = 1000\n").unwrap();
    change(&mut fake, &mut app);
    let again = app.diagnostics.shown().cloned().expect("a notice");
    assert_ne!(again.text, shown.text);
    std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    assert_eq!(app.config.font.size.points(), 16.0);
    // Back to the config in force, as after an error, takes it away too.
    std::fs::write(&path, "bogus = 1\n").unwrap();
    change(&mut fake, &mut app);
    std::fs::write(&path, "[font]\nsize = 16\n").unwrap();
    change(&mut fake, &mut app);
    assert_eq!(app.diagnostics.shown(), None);
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}

#[test]
fn a_font_that_is_not_installed_shows_its_notice_until_one_that_is() {
    let family = |name: &str| Some(name.to_owned().try_into().unwrap());
    let mut config = Config::default();
    config.font.family = family("No Such Family 42");
    let drawing = Drawing::new(config.font.clone()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), config, None).with_program(quiet());
    fake.start(&mut app);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(
        (shown.code, &shown.about),
        (Code::Unavailable, &About::Font)
    );
    assert!(
        shown
            .text
            .starts_with("the font family `No Such Family 42` is not installed; the text takes `")
    );
    // A family that is there takes it away.
    let mut config = app.config.clone();
    config.font.family = family("Menlo");
    app.apply(&mut fake, config);
    assert_eq!(app.diagnostics.shown(), None);
}

#[test]
fn a_failure_shows_its_notice_and_a_click_on_the_bar_closes_it() {
    let (mut fake, mut app, window, _) = with_tester();
    // A command that the caller named wrongly goes to the log alone.
    app.run(&mut fake, "test.nothing", Args::new());
    assert_eq!(app.diagnostics.shown(), None);
    // A plugin that panics: a fault, which disables it.
    app.run(&mut fake, "test.boom", Args::new());
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(shown.code, Code::Fault);
    assert!(
        shown.text.contains("panicked: on purpose"),
        "{}",
        shown.text
    );
    // Where the bar was drawn, a press on its close closes it; on the
    // rest of it, nothing; neither reaches the program, nor starts a
    // selection.
    let rect = |x, width| Rect {
        x,
        y: 56,
        width,
        height: 40,
    };
    let rects = notice::Rects {
        bar: rect(0, 800),
        close: rect(780, 20),
    };
    app.windows.get_mut(&window).unwrap().notice = Some(rects);
    let scale = fake.window(window).unwrap().size.scale;
    let at = |x: f64, y: f64| Point {
        x: x / scale,
        y: y / scale,
    };
    let press = |position| WindowEvent::MouseButton {
        button: lokyr_platform_core::MouseButton::Left,
        pressed: true,
        clicks: 1,
        position,
        mods: Modifiers::NONE,
    };
    fake.send(&mut app, window, press(at(10.0, 70.0)));
    assert!(app.diagnostics.shown().is_some());
    assert!(app.windows[&window].selecting.is_none());
    fake.send(&mut app, window, press(at(790.0, 70.0)));
    assert_eq!(app.diagnostics.shown(), None);
    assert!(app.windows[&window].selecting.is_none());
    // A failure of a command shows too; a press below the bar leaves it.
    let failed = Diagnostic::new(Code::Failed, "it failed", About::Other);
    app.report(&mut fake, failed);
    fake.send(&mut app, window, press(at(10.0, 120.0)));
    assert!(app.diagnostics.shown().is_some());
    assert!(app.windows[&window].selecting.is_some());
}

#[test]
fn a_bell_flashes_a_window_with_the_focus_and_marks_one_without() {
    let (mut fake, mut app, window) = configured("", ringer());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].flash_until.is_some()
    });
    // The flash ends with its timer.
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].flash_until.is_none()
    });
    assert_eq!((fake.alert_sounds(), fake.attention_requests()), (0, 0));
    assert!(!fake.window(window).unwrap().title.starts_with(BELL_MARK));
    // Without the focus: a bounce and the mark, once for the two bells,
    // until the window takes the focus. The bells of the first key are
    // a fifth of a second behind.
    let pause = Instant::now() + Duration::from_millis(250);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, app| {
        fake.window(window).unwrap().title == format!("🔔 {}", app.title_of(window))
    });
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(fake.attention_requests(), 1);
    assert!(app.windows[&window].flash_until.is_none());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert_eq!(fake.window(window).unwrap().title, app.title_of(window));
}

#[test]
fn the_bell_s_config_chooses_what_it_does() {
    let config = "[bell]\nsound = true\nflash = false\nbounce = false\nmark-title = false\n";
    let (mut fake, mut app, window) = configured(config, ringer());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| fake.alert_sounds() == 1);
    assert!(app.windows[&window].flash_until.is_none());
    fake.send(&mut app, window, WindowEvent::Focused(false));
    let pause = Instant::now() + Duration::from_millis(250);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| fake.alert_sounds() == 2);
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert_eq!(fake.alert_sounds(), 2, "one sound for the two bells");
    assert_eq!(fake.attention_requests(), 0);
    assert!(!fake.window(window).unwrap().title.starts_with(BELL_MARK));
    assert!(app.windows[&window].flash_until.is_none());
}

#[test]
fn a_notification_shows_for_a_window_without_the_focus() {
    let (mut fake, mut app, window) = configured("", notifier());
    // Not while the window has the focus.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    assert!(fake.notifications().is_empty());
    // Without it, the first of the two, its title the window's, as
    // OSC 9 gives none; the second within the pause shows nothing.
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    let pause = Instant::now() + Duration::from_millis(300);
    run_until(&mut fake, &mut app, |_, _| Instant::now() > pause);
    let shown = fake.notifications();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(
        (
            shown[0].window,
            shown[0].title.as_str(),
            shown[0].body.as_str()
        ),
        (window, "agent", "waiting")
    );
    // A click brings the window forward.
    fake.send_app(&mut app, AppEvent::NotificationClicked(window));
    assert_eq!(fake.focus_requests(), [window]);
    // A window that closed is not asked for.
    fake.send_app(&mut app, AppEvent::NotificationClicked(WindowId::new(999)));
    assert_eq!(fake.focus_requests(), [window]);
}

#[test]
fn a_notification_without_a_title_takes_the_user_s() {
    let (mut fake, mut app, window) = configured("", notifier());
    app.windows.get_mut(&window).unwrap().user_title = Some("build".to_owned());
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    assert_eq!(fake.notifications()[0].title, "build");
}

#[test]
fn a_notification_s_title_and_text_show_their_start() {
    let long = "x".repeat(5000);
    let program = shell(&format!(
        "stty raw -echo; echo ready; head -c 1 >/dev/null; printf '\\033]777;notify;{long};{long}\\a'; sleep 10"
    ));
    let (mut fake, mut app, window) = configured("", program);
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    let shown = &fake.notifications()[0];
    assert_eq!(
        (shown.title.chars().count(), shown.body.chars().count()),
        (NOTIFICATION_TITLE, NOTIFICATION_BODY)
    );
}

#[test]
fn a_stream_of_bells_wakes_the_main_thread_at_the_frame_s_pace() {
    let script = "echo ready; read line; while :; do printf '\\a\\a\\a\\a'; done";
    let (mut fake, mut app, window) = started_running(shell(script));
    let pane = app.windows[&window].pane;
    let wake = Arc::clone(&app.pane_wakes[&pane]);
    fake.send(&mut app, window, typed(KeyCode::Enter, "\r", false));
    // The main thread as it runs, which takes each wake at once, and a
    // display of 60 Hz, which draws the window while it shows.
    let run = |fake: &mut Fake, app: &mut App, drawn: bool| {
        let before = wake.wakes.load(Ordering::Relaxed);
        let start = Instant::now();
        let mut frames = 0;
        let mut frame_at = start;
        while start.elapsed() < Duration::from_millis(500) {
            fake.deliver_wake(app);
            fake.fire_timers(app, Instant::now());
            if drawn
                && frame_at.elapsed() >= TOLD_PACE
                && fake.window(window).unwrap().frame_requested
            {
                fake.send(app, window, WindowEvent::Frame);
                frames += 1;
                frame_at = Instant::now();
            }
        }
        (wake.wakes.load(Ordering::Relaxed) - before, frames)
    };
    // Drawn: a wake a frame for the grid, and one a pace for the bells,
    // at most.
    let (wakes, frames) = run(&mut fake, &mut app, true);
    assert!(app.windows[&window].rang_at.is_some(), "the bell rang");
    assert!(wakes <= frames + 35, "{wakes} wakes for {frames} frames");
    // Not drawn: one a pace for the bells.
    fake.send(&mut app, window, WindowEvent::Occluded(true));
    let (wakes, _) = run(&mut fake, &mut app, false);
    assert!(wakes <= 35, "{wakes} wakes hidden");
}
