use std::thread;

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
            title: "Bolt".to_owned(),
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
                    title: "Quit Bolt".to_owned(),
                    command: "quit".to_owned(),
                    shortcut: None,
                },
            ],
        }],
    }
}

impl App {
    fn open(&mut self, platform: &mut dyn Platform) {
        let window = platform
            .create_window(&WindowAttributes {
                title: "shell".to_owned(),
                size: SIZE,
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
        Some(&AppEvent::AlertClosed { alert, button: 1 })
    );
    // An alert closes once.
    assert!(!fake.answer_alert(&mut app, alert, 0));
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
