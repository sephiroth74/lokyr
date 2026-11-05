//! The tests of the windows' numbers, and of the keys that bring them
//! forward, which their titlebars show.

use super::*;

/// The number of `window` and its keys, as the workspace has them.
fn numbered(app: &App, window: WindowId) -> (Option<u32>, Option<String>) {
    let shown = app.workspace.window(app.windows[&window].window).unwrap();
    (shown.number(), shown.keys().map(str::to_owned))
}

/// Option and Command.
const OPTION_COMMAND: Modifiers = Modifiers {
    left_alt: true,
    ..CMD
};

#[test]
fn each_window_takes_the_lowest_number_that_no_other_has_and_its_keys() {
    let (mut fake, mut app) = started();
    for _ in 0..2 {
        fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    }
    let windows: Vec<WindowId> = fake.windows().collect();
    let numbers: Vec<(Option<u32>, Option<String>)> = windows
        .iter()
        .map(|&window| numbered(&app, window))
        .collect();
    let keys = |number: u32| (Some(number), Some(format!("⌥⌘{number}")));
    assert_eq!(numbers, [keys(1), keys(2), keys(3)]);
    // Each titlebar shows its own, with no frame of its own.
    for (&window, number) in windows.iter().zip(1..) {
        assert_eq!(
            fake.window(window).unwrap().title_accessory,
            Some(format!("⌥⌘{number}"))
        );
    }
    // The second closes: the next window takes its number, the third
    // keeping its own.
    app.close(&mut fake, windows[1]);
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let opened = fake
        .windows()
        .find(|window| !windows.contains(window))
        .unwrap();
    assert_eq!(numbered(&app, opened), keys(2));
    assert_eq!(numbered(&app, windows[2]), keys(3));
    // The quick terminal has none.
    toggle(&mut fake, &mut app);
    let quick = quick_window(&fake).unwrap();
    assert_eq!(numbered(&app, quick), (None, None));
    assert_eq!(fake.window(quick).unwrap().title_accessory, None);
}

#[test]
fn a_window_s_keys_bring_it_forward() {
    let (mut fake, mut app) = started();
    let first = fake.windows().next().unwrap();
    fake.send_app(&mut app, AppEvent::Command(NEW_WINDOW.to_owned()));
    let second = fake.windows().find(|&window| window != first).unwrap();
    let digit = |code, text| key_event(code, text, OPTION_COMMAND, true);
    fake.send(&mut app, second, digit(KeyCode::Digit1, "1"));
    assert_eq!(fake.focus_requests().last(), Some(&first));
    fake.send(&mut app, first, digit(KeyCode::Digit2, "2"));
    assert_eq!(fake.focus_requests().last(), Some(&second));
    // A number of no window brings nothing forward.
    let asked = fake.focus_requests().len();
    fake.send(&mut app, first, digit(KeyCode::Digit5, "5"));
    assert_eq!(fake.focus_requests().len(), asked);
}

#[test]
fn the_config_hides_the_keys_and_a_binding_of_the_user_s_shows_its_own() {
    let (mut fake, mut app) = started();
    let window = fake.windows().next().unwrap();
    let accessory = |fake: &Fake| fake.window(window).unwrap().title_accessory.clone();
    assert_eq!(accessory(&fake).as_deref(), Some("⌥⌘1"));
    let config = |text: &str| Config::parse(text, std::path::Path::new("config.toml")).unwrap();
    // Hidden, the keys stay the plugins'.
    app.apply(&mut fake, config("[window]\ntitle-shortcut = false\n"));
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(accessory(&fake), None);
    assert_eq!(numbered(&app, window).1.as_deref(), Some("⌥⌘1"));
    // The user's binding of the window's number, the default's taken away.
    app.apply(
        &mut fake,
        config(concat!(
            "[keys]\n",
            "\"alt+cmd+1\" = false\n",
            "\"ctrl+cmd+1\" = { command = \"window.goto\", args = { index = 1 } }\n",
        )),
    );
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(numbered(&app, window).1.as_deref(), Some("⌃⌘1"));
    assert_eq!(accessory(&fake).as_deref(), Some("⌃⌘1"));
    // Of no binding, none.
    app.apply(&mut fake, config("[keys]\n\"alt+cmd+1\" = false\n"));
    assert_eq!(numbered(&app, window), (Some(1), None));
    assert_eq!(accessory(&fake), None);
}
