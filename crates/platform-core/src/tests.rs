use std::thread;
use std::time::{Duration, Instant};

use crate::fake::Fake;
use crate::*;

/// An app that opens a window when it starts and another on `new-window`,
/// closes a window that the user asks to close, quits with its last one or
/// on `quit`, and keeps every event that it gets.
#[derive(Default)]
struct App {
    windows: Vec<WindowId>,
    window_events: Vec<(WindowId, WindowEvent)>,
    app_events: Vec<AppEvent>,
    alert: Option<AlertId>,
    pasted: Option<String>,
}

const SIZE: Size = Size {
    width: 800.0,
    height: 600.0,
};

fn menu() -> MainMenu {
    MainMenu {
        menus: vec![Menu {
            title: "Lokyr".to_owned(),
            items: vec![
                MenuItem::Command {
                    title: "New Window".to_owned(),
                    command: "new-window".to_owned(),
                    shortcut: Some(Shortcut {
                        key: ShortcutKey::Char('n'),
                        command: true,
                        shift: false,
                        alt: false,
                        control: false,
                    }),
                },
                MenuItem::Separator,
                MenuItem::Command {
                    title: "Quit Lokyr".to_owned(),
                    command: "quit".to_owned(),
                    shortcut: None,
                },
            ],
            role: None,
        }],
    }
}

impl App {
    fn open(&mut self, platform: &mut dyn Platform) {
        let window = platform
            .create_window(&WindowAttributes {
                title: "shell".to_owned(),
                size: SIZE,
                remember: false,
                restore: false,
                restore_size: false,
            })
            .unwrap();
        self.windows.push(window);
    }
}

impl Handler for App {
    fn started(&mut self, platform: &mut dyn Platform) {
        platform.set_main_menu(&menu());
        self.open(platform);
    }

    fn app_event(&mut self, platform: &mut dyn Platform, event: AppEvent) {
        if let AppEvent::Command(command) = &event {
            match command.as_str() {
                "new-window" => self.open(platform),
                "quit" => platform.quit(),
                "confirm" => {
                    let alert = Alert {
                        style: AlertStyle::Warning,
                        message: "Close the window?".to_owned(),
                        detail: "vim is running.".to_owned(),
                        buttons: vec!["Close".to_owned(), "Cancel".to_owned()],
                        suppression: None,
                        input: None,
                    };
                    self.alert = Some(platform.show_alert(self.windows.first().copied(), &alert));
                }
                "copy" => platform.set_clipboard_text("echo hi"),
                "paste" => self.pasted = platform.clipboard_text(),
                _ => {}
            }
        }
        self.app_events.push(event);
    }

    fn window_event(&mut self, platform: &mut dyn Platform, window: WindowId, event: WindowEvent) {
        if event == WindowEvent::CloseRequested {
            platform.close_window(window);
            self.windows.retain(|&open| open != window);
            if self.windows.is_empty() {
                platform.quit();
            }
        }
        self.window_events.push((window, event));
    }
}

/// A fake with the app started, and the app's first window.
fn started() -> (Fake, App, WindowId) {
    let mut fake = Fake::default();
    let mut app = App::default();
    fake.start(&mut app);
    let window = app.windows[0];
    (fake, app, window)
}

fn command(name: &str) -> AppEvent {
    AppEvent::Command(name.to_owned())
}

#[test]
fn starting_opens_a_window_and_sets_the_menu() {
    let (fake, _, window) = started();
    assert_eq!(fake.windows().collect::<Vec<_>>(), [window]);
    let state = fake.window(window).unwrap();
    assert_eq!(state.title, "shell");
    assert_eq!(state.size.points, SIZE);
    assert_eq!(
        state.size.pixels,
        PixelSize {
            width: 1600,
            height: 1200
        }
    );
    assert_eq!(fake.main_menu(), Some(&menu()));
    assert!(fake.window_handle(window).is_none());
    assert!(!fake.has_quit());
}

#[test]
fn a_resize_and_a_change_of_scale_reach_the_window_and_the_app() {
    let (mut fake, mut app, window) = started();
    let larger = WindowSize::new(
        Size {
            width: 1000.5,
            height: 700.0,
        },
        2.0,
    );
    assert!(fake.send(&mut app, window, WindowEvent::Resized(larger)));
    assert_eq!(fake.window_size(window), Some(larger));
    assert_eq!(
        larger.pixels,
        PixelSize {
            width: 2001,
            height: 1400
        }
    );

    let external = WindowSize::new(larger.points, 1.0);
    assert!(fake.send(&mut app, window, WindowEvent::ScaleChanged(external)));
    assert_eq!(
        fake.window_size(window).unwrap().pixels,
        PixelSize {
            width: 1001,
            height: 700
        }
    );
    assert_eq!(
        app.window_events,
        [
            (window, WindowEvent::Resized(larger)),
            (window, WindowEvent::ScaleChanged(external))
        ]
    );
}

#[test]
fn the_focus_and_the_occlusion_follow_the_events() {
    let (mut fake, mut app, first) = started();
    fake.send_app(&mut app, command("new-window"));
    let second = app.windows[1];
    assert_eq!(fake.focused_window(), None);

    fake.send(&mut app, first, WindowEvent::Focused(true));
    assert_eq!(fake.focused_window(), Some(first));
    // The focus moves: the first window loses it, then the second takes it.
    fake.send(&mut app, first, WindowEvent::Focused(false));
    fake.send(&mut app, second, WindowEvent::Focused(true));
    assert_eq!(fake.focused_window(), Some(second));
    // A window that loses a focus that it did not have changes nothing.
    fake.send(&mut app, first, WindowEvent::Focused(false));
    assert_eq!(fake.focused_window(), Some(second));

    fake.send(&mut app, second, WindowEvent::Occluded(true));
    assert!(fake.window(second).unwrap().occluded);
    fake.send(&mut app, second, WindowEvent::Occluded(false));
    assert!(!fake.window(second).unwrap().occluded);
}

#[test]
fn input_reaches_the_app_as_it_came() {
    let (mut fake, mut app, window) = started();
    let option = Modifiers {
        left_alt: true,
        ..Modifiers::NONE
    };
    let events = [
        WindowEvent::Key(KeyEvent {
            code: KeyCode::Semicolon,
            state: KeyState::Pressed,
            mods: option,
            text: "@".to_owned(),
            unmodified: "ò".to_owned(),
            plain: "ò".to_owned(),
            time: None,
        }),
        WindowEvent::Ime(Ime::Preedit {
            text: "にほ".to_owned(),
            selection: 6..6,
        }),
        WindowEvent::Ime(Ime::Commit("日本".to_owned())),
        WindowEvent::MouseButton {
            button: MouseButton::Left,
            pressed: true,
            position: Point { x: 12.0, y: 40.5 },
            mods: Modifiers::NONE,
            clicks: 2,
        },
        WindowEvent::MouseMoved {
            position: Point { x: 80.0, y: 41.0 },
            mods: Modifiers::NONE,
        },
        WindowEvent::Scroll {
            delta: ScrollDelta::Points { x: 0.0, y: -12.5 },
            phase: Some(ScrollPhase::Moved),
            momentum: true,
            position: Point { x: 80.0, y: 41.0 },
            mods: Modifiers::NONE,
        },
    ];
    for event in &events {
        assert!(fake.send(&mut app, window, event.clone()));
    }
    let got: Vec<_> = app
        .window_events
        .iter()
        .map(|(_, event)| event.clone())
        .collect();
    assert_eq!(got, events);
}

#[test]
fn closing_the_last_window_quits() {
    let (mut fake, mut app, first) = started();
    fake.send_app(&mut app, command("new-window"));
    let second = app.windows[1];

    fake.send(&mut app, first, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().collect::<Vec<_>>(), [second]);
    assert!(!fake.has_quit());
    // A closed window takes no event.
    assert!(!fake.send(&mut app, first, WindowEvent::Focused(true)));

    fake.send(&mut app, second, WindowEvent::CloseRequested);
    assert!(fake.has_quit());
    // Nothing comes once the app has quit.
    let seen = app.app_events.len();
    assert!(!fake.send_app(&mut app, command("new-window")));
    assert_eq!(app.app_events.len(), seen);
}

#[test]
fn a_frame_answers_the_request_for_it() {
    let (mut fake, mut app, window) = started();
    assert!(!fake.window(window).unwrap().frame_requested);
    fake.request_frame(window);
    fake.request_frame(window);
    assert!(fake.window(window).unwrap().frame_requested);
    assert!(fake.send(&mut app, window, WindowEvent::Frame));
    assert!(!fake.window(window).unwrap().frame_requested);
    assert_eq!(
        app.window_events.last(),
        Some(&(window, WindowEvent::Frame))
    );
    // A window that is not open asks for nothing.
    fake.request_frame(WindowId::new(99));
}

#[test]
fn quitting_from_the_menu_ends_the_application() {
    let (mut fake, mut app, window) = started();
    fake.send_app(&mut app, command("quit"));
    assert!(fake.has_quit());
    assert!(!fake.send(&mut app, window, WindowEvent::CloseRequested));
}

#[test]
fn a_window_takes_a_number_that_no_other_took() {
    let (mut fake, mut app, first) = started();
    fake.send_app(&mut app, command("new-window"));
    fake.send(&mut app, first, WindowEvent::CloseRequested);
    fake.send_app(&mut app, command("new-window"));
    let ids: Vec<u64> = app.windows.iter().map(|window| window.get()).collect();
    assert_eq!(ids.len(), 2);
    assert!(!ids.contains(&first.get()));
    assert_ne!(ids[0], ids[1]);
}

#[test]
fn wakes_from_other_threads_reach_the_main_thread_as_one() {
    let (mut fake, mut app, _) = started();
    assert!(!fake.deliver_wake(&mut app));
    let waker = fake.waker();
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let waker = waker.clone();
            thread::spawn(move || waker.wake())
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert!(fake.deliver_wake(&mut app));
    assert!(!fake.deliver_wake(&mut app));
    assert_eq!(app.app_events, [AppEvent::Wake]);
}

#[test]
fn an_alert_closes_with_the_button_chosen() {
    let (mut fake, mut app, window) = started();
    fake.send_app(&mut app, command("confirm"));
    let alert = app.alert.unwrap();
    assert_eq!(fake.alerts().len(), 1);
    assert_eq!(fake.alerts()[0].window, Some(window));
    assert_eq!(fake.alerts()[0].alert.buttons, ["Close", "Cancel"]);

    assert!(fake.answer_alert(&mut app, alert, 1));
    assert!(fake.alerts().is_empty());
    assert_eq!(
        app.app_events.last(),
        Some(&AppEvent::AlertClosed {
            alert,
            button: 1,
            suppressed: false,
            text: None
        })
    );
    // An alert closes once.
    assert!(!fake.answer_alert(&mut app, alert, 0));
    // A ticked checkbox counts only on an alert that has one.
    fake.send_app(&mut app, command("confirm"));
    let alert = app.alert.unwrap();
    fake.answer_alert_with(&mut app, alert, 0, true);
    assert_eq!(
        app.app_events.last(),
        Some(&AppEvent::AlertClosed {
            alert,
            button: 0,
            suppressed: false,
            text: None
        })
    );
    let checked = fake.show_alert(
        Some(window),
        &Alert {
            style: AlertStyle::Warning,
            message: "Paste 2 lines?".to_owned(),
            detail: String::new(),
            buttons: vec!["Paste".to_owned(), "Cancel".to_owned()],
            suppression: Some("Don't ask again".to_owned()),
            input: None,
        },
    );
    fake.answer_alert_with(&mut app, checked, 0, true);
    assert_eq!(
        app.app_events.last(),
        Some(&AppEvent::AlertClosed {
            alert: checked,
            button: 0,
            suppressed: true,
            text: None
        })
    );
    // A field's text, as typed, or as it opened.
    let asking = |fake: &mut Fake| {
        fake.show_alert(
            Some(window),
            &Alert {
                style: AlertStyle::Informational,
                message: "The title".to_owned(),
                detail: String::new(),
                buttons: vec!["Set".to_owned()],
                suppression: None,
                input: Some("old".to_owned()),
            },
        )
    };
    let typed = asking(&mut fake);
    fake.answer_alert_typed(&mut app, typed, (0, false), Some("new"));
    let untouched = asking(&mut fake);
    fake.answer_alert(&mut app, untouched, 0);
    let texts: Vec<Option<&str>> = app.app_events[app.app_events.len() - 2..]
        .iter()
        .map(|event| match event {
            AppEvent::AlertClosed { text, .. } => text.as_deref(),
            _ => None,
        })
        .collect();
    assert_eq!(texts, [Some("new"), Some("old")]);
}

#[test]
fn an_alert_closes_with_its_window() {
    let (mut fake, mut app, window) = started();
    fake.send_app(&mut app, command("new-window"));
    fake.send_app(&mut app, command("confirm"));
    let alert = app.alert.unwrap();
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert!(fake.alerts().is_empty());
    assert!(!fake.answer_alert(&mut app, alert, 0));
}

#[test]
fn timers_fire_once_in_their_order_when_their_time_comes() {
    let (mut fake, mut app, _) = started();
    let now = Instant::now();
    let after = |millis| now + Duration::from_millis(millis);
    let later = fake.start_timer(after(20));
    let first = fake.start_timer(after(10));
    let cancelled = fake.start_timer(after(15));
    fake.cancel_timer(cancelled);
    assert_eq!(fake.fire_timers(&mut app, now), 0);
    assert_eq!(fake.fire_timers(&mut app, after(30)), 2);
    assert_eq!(fake.fire_timers(&mut app, after(40)), 0);
    assert_eq!(
        app.app_events,
        [AppEvent::Timer(first), AppEvent::Timer(later)]
    );
    assert_eq!(fake.timers().count(), 0);
    // A timer that fired, or was cancelled, takes nothing.
    fake.cancel_timer(first);
    assert!(first != later && later != cancelled);
}

#[test]
fn the_clipboard_gives_back_its_text() {
    let (mut fake, mut app, _) = started();
    fake.send_app(&mut app, command("paste"));
    assert_eq!(app.pasted, None);
    fake.send_app(&mut app, command("copy"));
    fake.send_app(&mut app, command("paste"));
    assert_eq!(app.pasted.as_deref(), Some("echo hi"));
}

#[test]
fn the_title_and_the_appearance_change() {
    let (mut fake, mut app, window) = started();
    fake.set_title(window, "vim");
    assert_eq!(fake.window(window).unwrap().title, "vim");
    // A closed window keeps no title.
    fake.close_window(WindowId::new(99));
    fake.set_title(WindowId::new(99), "nothing");
    assert_eq!(fake.window_size(WindowId::new(99)), None);

    assert_eq!(fake.appearance(), Appearance::Light);
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    assert_eq!(fake.appearance(), Appearance::Dark);
}

#[test]
fn a_key_names_what_it_types_on_the_us_layout() {
    assert_eq!(KeyCode::KeyQ.base_layout(), Some('q'));
    assert_eq!(KeyCode::Digit0.base_layout(), Some('0'));
    assert_eq!(KeyCode::BracketLeft.base_layout(), Some('['));
    assert_eq!(KeyCode::Quote.base_layout(), Some('\''));
    assert_eq!(KeyCode::Space.base_layout(), Some(' '));
    // Keys that type nothing, or that PC-101 lacks.
    assert_eq!(KeyCode::Enter.base_layout(), None);
    assert_eq!(KeyCode::Numpad1.base_layout(), None);
    assert_eq!(KeyCode::IntlBackslash.base_layout(), None);
    assert_eq!(KeyCode::Unidentified(0x7f).base_layout(), None);
    // And back, each key that types a character.
    for character in "abcdefghijklmnopqrstuvwxyz0123456789`-=[]\\;',./ ".chars() {
        let key = KeyCode::from_base_layout(character).unwrap();
        assert_eq!(key.base_layout(), Some(character), "{character}");
    }
    assert_eq!(KeyCode::from_base_layout('Q'), Some(KeyCode::KeyQ));
    assert_eq!(KeyCode::from_base_layout('è'), None);
}

#[test]
fn a_size_in_pixels_rounds_to_the_nearest() {
    let size = WindowSize::new(
        Size {
            width: 100.25,
            height: 0.0,
        },
        2.0,
    );
    assert_eq!(
        size.pixels,
        PixelSize {
            width: 201,
            height: 0
        }
    );
    let size = WindowSize::new(
        Size {
            width: -4.0,
            height: f64::NAN,
        },
        2.0,
    );
    assert_eq!(
        size.pixels,
        PixelSize {
            width: 0,
            height: 0
        }
    );
}

#[test]
fn the_fake_passes_the_platforms_checks() {
    let directory =
        std::env::temp_dir().join(format!("lokyr-fake-conformance-{}", std::process::id()));
    let failed = crate::conformance::check(&mut Fake::default(), &directory);
    assert!(failed.is_empty(), "{failed:#?}");
    assert!(!directory.exists());
}

#[test]
fn the_fake_reads_targets_as_a_backend_does() {
    let directory = std::env::temp_dir().join(format!("lokyr-fake-targets-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("x.jar"), "").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(directory.join("x.jar"), directory.join("link.txt")).unwrap();
    let path = std::fs::canonicalize(&directory)
        .unwrap()
        .display()
        .to_string();
    let mut fake = Fake::default();
    // A file of an extension that runs, by its URL and through a link.
    let jar = fake
        .resolve_target(&format!("file://{path}/x.jar"))
        .unwrap();
    assert_eq!(jar.kind, TargetKind::Program);
    assert_eq!(jar.location, format!("{path}/x.jar"));
    #[cfg(unix)]
    assert_eq!(
        fake.resolve_target(&format!("{path}/link.txt")).unwrap(),
        jar
    );
    // A URL's case of `file`, and an escape that is no UTF-8's.
    assert_eq!(
        fake.resolve_target(&format!("FILE://localhost{path}/x.jar"))
            .unwrap(),
        jar
    );
    assert!(fake.resolve_target("file://localhost/%ZZ").is_err());
    // What opens is recorded by its location; a program is not.
    assert!(fake.open(&jar).is_err());
    fake.reveal(&jar).unwrap();
    let folder = fake.resolve_target(&path).unwrap();
    fake.open(&folder).unwrap();
    assert_eq!(fake.opened(), std::slice::from_ref(&path));
    assert_eq!(fake.revealed(), [format!("{path}/x.jar")]);
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn a_quick_window_opens_hidden_and_takes_its_place_s_size_as_it_shows() {
    let mut fake = Fake::new(2.0);
    let quick = fake.create_quick_window("Lokyr").unwrap();
    let state = fake.window(quick).unwrap();
    assert_eq!(state.quick, Some(None));
    assert!(state.occluded);
    let place = QuickPlace {
        primary: Length::Percent(40.0),
        ..QuickPlace::default()
    };
    fake.show_quick_window(quick, Some(&place));
    let state = fake.window(quick).unwrap();
    assert_eq!(state.quick, Some(Some(place)));
    assert!(!state.occluded);
    // Shown, it keeps its place until it hides.
    let other = QuickPlace {
        primary: Length::Percent(80.0),
        ..place
    };
    fake.show_quick_window(quick, Some(&other));
    let state = fake.window(quick).unwrap();
    assert_eq!(state.quick, Some(Some(place)));
    // 40% of the area under the menu bar, the screen's width.
    assert_eq!(
        state.size,
        WindowSize::new(
            Size {
                width: 1440.0,
                height: 350.0
            },
            2.0
        )
    );
    fake.focus_window(quick);
    fake.show_quick_window(quick, None);
    assert_eq!(fake.window(quick).unwrap().quick, Some(None));
    // Hidden, the focus does not reach it.
    fake.focus_window(quick);
    assert_eq!(fake.focus_requests(), [quick]);
    // Another window is no quick one.
    let window = fake
        .create_window(&WindowAttributes {
            title: "shell".to_owned(),
            size: SIZE,
            remember: false,
            restore: false,
            restore_size: false,
        })
        .unwrap();
    fake.show_quick_window(window, Some(&place));
    assert_eq!(fake.window(window).unwrap().quick, None);
}

#[test]
fn the_global_keys_that_another_app_takes_are_refused() {
    let mut fake = Fake::new(2.0);
    let key = |code| GlobalKey {
        code,
        command: false,
        control: true,
        alt: false,
        shift: false,
    };
    fake.take_global_key(key(KeyCode::KeyT));
    let keys = [key(KeyCode::Backquote), key(KeyCode::KeyT)];
    assert_eq!(fake.set_global_keys(&keys), [1]);
    assert_eq!(fake.global_keys(), keys);
    assert_eq!(fake.set_global_keys(&[]), Vec::<usize>::new());
    assert!(fake.global_keys().is_empty());
}
