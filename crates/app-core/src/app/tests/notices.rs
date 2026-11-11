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
fn a_term_that_no_terminfo_entry_describes_shows_its_notice_until_one_that_is() {
    let root = std::env::temp_dir().join(format!("lokyr-term-notice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("78")).unwrap();
    std::fs::write(root.join("78").join("xterm-256color"), b"entry").unwrap();
    let term = |name: &str| Term::try_from(name.to_owned()).unwrap();
    let mut config = Config::default();
    config.terminal.term = term("xtermxterm-256color");
    let mut fake = Fake::default();
    let mut app = app(config, quiet());
    app.terminfo_dirs = vec![root.clone()];
    app.environment.terminfo = None;
    fake.start(&mut app);
    let shown = app.diagnostics.shown().cloned().expect("a notice");
    assert_eq!(
        (shown.code, &shown.about),
        (Code::Unavailable, &About::Term)
    );
    assert!(
        shown
            .text
            .starts_with("no terminfo entry describes `xtermxterm-256color`, "),
        "{}",
        shown.text
    );
    // A name that an entry describes takes it away.
    let mut retermed = |app: &mut App, name: &str| {
        let mut config = app.config.clone();
        config.terminal.term = term(name);
        app.apply(&mut fake, config);
        app.diagnostics.shown().map(|shown| shown.about.clone())
    };
    assert_eq!(retermed(&mut app, "xterm-256color"), None);
    // The terminal's own entry, where the app has it, as its programs find
    // it through `TERMINFO`.
    assert_eq!(retermed(&mut app, "lokyr"), Some(About::Term));
    app.environment.terminfo = Some(root.clone());
    assert_eq!(retermed(&mut app, "xterm-lokyr"), None);
    assert_eq!(retermed(&mut app, "nothing-of-the-kind"), Some(About::Term));
    let _ = std::fs::remove_dir_all(&root);
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
fn a_failure_shows_its_notice() {
    let (mut fake, mut app, _, _) = with_tester();
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
}

/// An app that draws, of a window of 28 points of titlebar, drawn.
fn drawn() -> (Fake, App, WindowId) {
    let drawing = Drawing::new(lokyr_text_core::FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(quiet());
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(crate::fakes::Surface::default());
    for _ in 0..2 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    (fake, app, window)
}

/// The card of `window`'s notice, if it shows: its frame, and the frame of
/// its node of `key`, in the window's pixels.
fn card(app: &App, window: WindowId, key: &str) -> Option<(Rect, Rect)> {
    let view = app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view == super::super::notice_card::notice_view())?;
    let frame = view.frame?;
    let node = view
        .surface
        .tree
        .by_key(&lokyr_kernel::plugins::view::Key::from(key))?;
    let at = view.surface.layout.frame(&view.surface.tree, node)?;
    let rect = |x: f32, y: f32, width: f32, height: f32| Rect {
        x: x as i32,
        y: y as i32,
        width: width as u32,
        height: height as u32,
    };
    Some((
        rect(frame.x, frame.y, frame.width, frame.height),
        rect(frame.x + at.x, frame.y + at.y, at.width, at.height),
    ))
}

/// The texts of the card's nodes, in their order.
fn card_texts(app: &App, window: WindowId) -> Vec<String> {
    let Some(view) = app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view == super::super::notice_card::notice_view())
    else {
        return Vec::new();
    };
    let tree = &view.surface.tree;
    let mut texts = Vec::new();
    let mut left: Vec<_> = tree.root().into_iter().collect();
    while let Some(node) = left.pop() {
        if let Some(lokyr_kernel::plugins::view::Kind::Text(text)) = tree.kind(node) {
            texts.push(text.clone());
        }
        left.extend(tree.children(node).iter().rev().copied());
    }
    texts
}

/// Clicks `window` in the middle of `rect`, in its pixels.
fn click_rect(fake: &mut Fake, app: &mut App, window: WindowId, rect: Rect) {
    let scale = fake.window(window).unwrap().size.scale;
    let at = Point {
        x: (f64::from(rect.x) + f64::from(rect.width) / 2.0) / scale,
        y: (f64::from(rect.y) + f64::from(rect.height) / 2.0) / scale,
    };
    let press = |pressed| WindowEvent::MouseButton {
        button: lokyr_platform_core::MouseButton::Left,
        pressed,
        clicks: 1,
        position: at,
        mods: Modifiers::NONE,
    };
    fake.send(app, window, press(true));
    fake.send(app, window, press(false));
}

#[test]
fn a_notice_shows_as_a_card_at_the_panes_top_right_which_its_buttons_copy_and_close() {
    let (mut fake, mut app, window) = drawn();
    let size = fake.window(window).unwrap().size;
    let width = size.pixels.width as i32;
    let long = "unknown key `fonts` at line 12, column 1 of config.toml, which the config does not know, as no section of it holds such a key at all";
    app.report(
        &mut fake,
        Diagnostic::new(Code::InvalidConfig, long, About::Config),
    );
    for _ in 0..3 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    let (frame, card_rect) = card(&app, window, "card").expect("a card");
    // At the right, not as wide as the window, below the titlebar.
    assert!(
        card_rect.width < size.pixels.width / 2 + 400,
        "{card_rect:?}"
    );
    assert!(
        frame.x + frame.width as i32 <= width && frame.x > 0,
        "{frame:?}"
    );
    assert!(frame.y as u32 >= size.bar_pixels(), "{frame:?}");
    // Its title and its text, in two lines at most, the second cut.
    let texts = card_texts(&app, window);
    assert_eq!(texts[0], "The config file does not apply", "{texts:?}");
    assert!(texts.len() <= 3, "{texts:?}");
    assert!(texts[2].ends_with('…'), "{texts:?}");
    // Its copy puts the whole notice on the clipboard, of its title.
    let (_, copy) = card(&app, window, "copy").unwrap();
    click_rect(&mut fake, &mut app, window, copy);
    let copied = fake.clipboard_text().expect("copied");
    assert!(
        copied.starts_with("The config file does not apply\n\n"),
        "{copied}"
    );
    assert!(copied.contains(long), "{copied}");
    // A config's notice stays; its close closes it, and the card goes.
    assert_eq!(app.notice.timer, None);
    let (_, close) = card(&app, window, "close").unwrap();
    click_rect(&mut fake, &mut app, window, close);
    assert_eq!(app.diagnostics.shown(), None);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert!(card(&app, window, "card").is_none());
    assert!(
        app.windows[&window].selecting.is_none(),
        "no selection under it"
    );
}

#[test]
fn a_failure_s_card_goes_by_itself_unless_the_pointer_rests_on_it() {
    let (mut fake, mut app, window) = drawn();
    app.report(
        &mut fake,
        Diagnostic::new(Code::Failed, "it failed", About::Other),
    );
    fake.send(&mut app, window, WindowEvent::Frame);
    let (_, at) = app.notice.timer.expect("its going timed");
    // The pointer rests on it: it stays past its time.
    let card = super::super::notice_card::notice_view();
    let hover = |kind| lokyr_kernel::plugins::ui::UiEvent {
        view: card.clone(),
        key: lokyr_kernel::plugins::view::Key::from("card"),
        kind,
    };
    app.notice_event(
        &mut fake,
        &hover(lokyr_kernel::plugins::view::UiEventKind::HoverEnter),
    );
    assert_eq!(app.notice.timer, None);
    fake.fire_timers(&mut app, at + Duration::from_secs(1));
    assert!(app.diagnostics.shown().is_some());
    // It leaves: the time starts again, and the card goes.
    app.notice_event(
        &mut fake,
        &hover(lokyr_kernel::plugins::view::UiEventKind::HoverLeave),
    );
    let (_, again) = app.notice.timer.expect("timed again");
    assert!(again >= at, "from the leave");
    fake.fire_timers(&mut app, again + Duration::from_millis(10));
    assert_eq!(app.diagnostics.shown(), None);
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
fn a_long_command_s_end_shows_while_its_window_has_no_focus() {
    // At a key, a short command, then one of 1.2 s that fails; at the next,
    // another of 1.2 s that succeeds.
    let program = shell(concat!(
        "m() { printf '\\033]133;%s\\007' \"$1\"; }; stty raw -echo; echo ready; ",
        "head -c 1 >/dev/null; ",
        "m A; printf '$ '; m B; echo true; m C; m 'D;0'; ",
        "m A; printf '$ '; m B; echo 'make all'; m C; sleep 1.2; m 'D;2'; ",
        "head -c 1 >/dev/null; ",
        "m A; printf '$ '; m B; echo 'sleep 1.2'; m C; sleep 1.2; m 'D;0'; echo done; sleep 10",
    ));
    let (mut fake, mut app, window) = configured("[notifications]\nlong-command = 1\n", program);
    fake.send(&mut app, window, WindowEvent::Focused(false));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |fake, _| {
        !fake.notifications().is_empty()
    });
    // The long one alone, of its line and its status, its time from its
    // marks.
    let shown = fake.notifications();
    assert_eq!(shown.len(), 1, "{shown:?}");
    assert_eq!(
        (shown[0].window, shown[0].title.as_str()),
        (window, "make all")
    );
    assert!(
        shown[0].body.starts_with("Failed with status 2 after ") && shown[0].body.ends_with(" s"),
        "{}",
        shown[0].body
    );
    // Nothing while the window has the focus.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send(&mut app, window, key());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "done"));
    assert_eq!(fake.notifications().len(), 1);
}

#[test]
fn a_command_s_time_tells_seconds_minutes_and_hours() {
    let (_, app) = started();
    for (seconds, wanted) in [
        (0, "0 s"),
        (59, "59 s"),
        (65, "1 min 5 s"),
        (3600, "1 h 0 min"),
        (3725, "1 h 2 min"),
    ] {
        assert_eq!(app.duration_text(Duration::from_secs(seconds)), wanted);
    }
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
    let pane = app.focused(window).unwrap();
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
