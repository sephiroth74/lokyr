//! The plugin against the SDK's fake host, which opens its quick window as
//! the app would, of a pane that takes the focus, and shows and hides it.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::PaneId;
use lokyr_plugin_sdk::message::Request;

use super::*;

/// The plugin's name, as its calls give it.
const NAME: &str = "lokyr/quick-terminal";

/// A host of a window of pane 1, which has the focus, and the plugin,
/// activated of `config`, its section.
fn host(config: &[(&str, Data)]) -> (FakeHost, QuickTerminal) {
    let mut host = FakeHost::new(NAME);
    host.focus(Some(PaneId(1)));
    host.set_config(section(config));
    let mut plugin = QuickTerminal::default();
    host.activate(&mut plugin).unwrap();
    (host, plugin)
}

/// A section of the config of `pairs`.
fn section(pairs: &[(&str, Data)]) -> Data {
    Data::Map(
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn text(value: &str) -> Data {
    Data::Text(value.to_owned())
}

/// Runs the command, and returns the requests of the quick window that it
/// made.
fn toggle(host: &mut FakeHost, plugin: &mut QuickTerminal) -> Vec<Request> {
    host.take_done();
    host.command(plugin, TOGGLE, Args::new()).unwrap();
    quick_requests(host)
}

/// The requests of the quick window that the plugin made since the last
/// look.
fn quick_requests(host: &mut FakeHost) -> Vec<Request> {
    host.take_done()
        .into_iter()
        .filter(|request| {
            matches!(
                request,
                Request::OpenQuickWindow { .. } | Request::ShowQuickWindow { .. }
            )
        })
        .collect()
}

/// The quick window, which the plugin opened.
fn quick(plugin: &QuickTerminal) -> WindowId {
    plugin.window.expect("a quick window")
}

#[test]
fn the_command_registers_with_its_title_and_the_global_key_s_capability() {
    let (host, _) = host(&[]);
    let [command] = host.commands() else {
        panic!("one command: {:?}", host.commands());
    };
    assert_eq!(
        (command.name.as_str(), command.category),
        (TOGGLE, Category::Windows)
    );
    assert_eq!(command.capabilities, [Capability::GlobalHotkey]);
    assert_eq!(host.subscriptions(), [WINDOW_FOCUS]);
    // It binds no key: the user's config binds a global one.
    assert!(host.bindings().is_empty());
    for (_, catalog) in CATALOGS {
        assert!(catalog.contains("command-window-toggle-quick-terminal = "));
    }
}

#[test]
fn it_opens_shows_and_hides_its_window_as_the_focus_says() {
    let (mut host, mut plugin) = host(&[]);
    // Opened, at the top, of half the screen, in the folder of a new
    // window's.
    assert_eq!(
        toggle(&mut host, &mut plugin),
        [Request::OpenQuickWindow {
            place: QuickPlace::default(),
            directory: None,
        }]
    );
    let window = quick(&plugin);
    // Its pane has the keyboard.
    let pane = host.keyboard().expect("a pane with the keyboard");
    assert_eq!(host.pane(pane).window, window);
    // With the focus, it hides.
    assert_eq!(
        toggle(&mut host, &mut plugin),
        [Request::ShowQuickWindow {
            window,
            place: None
        }]
    );
    // Hidden, it shows.
    let shown = Request::ShowQuickWindow {
        window,
        place: Some(QuickPlace::default()),
    };
    assert_eq!(toggle(&mut host, &mut plugin), std::slice::from_ref(&shown));
    // Shown without the focus, it takes it.
    host.focus(Some(PaneId(1)));
    assert_eq!(toggle(&mut host, &mut plugin), [shown]);
    assert_eq!(quick(&plugin), window);
}

#[test]
fn it_hides_as_it_loses_the_focus_as_autohide_says() {
    let (mut host, mut plugin) = host(&[]);
    toggle(&mut host, &mut plugin);
    let window = quick(&plugin);
    let lost = |window| Event::WindowFocus {
        window,
        focused: false,
    };
    // Another window's loss, and a gain, leave it.
    host.events(&mut plugin, &[lost(WindowId(77))]).unwrap();
    host.events(
        &mut plugin,
        &[Event::WindowFocus {
            window,
            focused: true,
        }],
    )
    .unwrap();
    assert!(quick_requests(&mut host).is_empty());
    host.events(&mut plugin, &[lost(window)]).unwrap();
    assert_eq!(
        quick_requests(&mut host),
        [Request::ShowQuickWindow {
            window,
            place: None
        }]
    );
    // Hidden already, it asks nothing.
    host.events(&mut plugin, &[lost(window)]).unwrap();
    assert!(quick_requests(&mut host).is_empty());
    // Without autohide, it stays.
    let (mut host, mut plugin) = self::host(&[("autohide", Data::Boolean(false))]);
    toggle(&mut host, &mut plugin);
    let window = quick(&plugin);
    host.events(&mut plugin, &[lost(window)]).unwrap();
    assert!(quick_requests(&mut host).is_empty());
}

#[test]
fn its_config_gives_its_place_and_a_change_applies_as_it_shows_next() {
    let (mut host, mut plugin) = host(&[
        ("position", text("right")),
        ("size", text("30%,400px")),
        ("screen", text("mouse")),
        ("animation", Data::Integer(0)),
        ("spaces", text("remain")),
    ]);
    let place = QuickPlace {
        edge: QuickEdge::Right,
        primary: QuickLength::Percent(30.0),
        secondary: Some(QuickLength::Points(400.0)),
        screen: QuickScreen::Pointer,
        animation: 0,
        every_space: false,
    };
    assert_eq!(
        toggle(&mut host, &mut plugin),
        [Request::OpenQuickWindow {
            place,
            directory: None
        }]
    );
    let window = quick(&plugin);
    toggle(&mut host, &mut plugin);
    // A change, as the file reloads, shows it at its new place.
    host.set_config(section(&[
        ("position", text("center")),
        ("size", text("800px")),
    ]));
    host.deliver(&mut plugin).unwrap();
    assert_eq!(
        toggle(&mut host, &mut plugin),
        [Request::ShowQuickWindow {
            window,
            place: Some(QuickPlace {
                edge: QuickEdge::Center,
                primary: QuickLength::Points(800.0),
                ..QuickPlace::default()
            }),
        }]
    );
}

#[test]
fn a_size_that_does_not_read_takes_half_the_screen_and_tells_why() {
    for size in [
        "0%", "101%", "300", "px", "50%,", "50%,0px", "1e9px", "abc%",
    ] {
        let (settings, error) = Settings::of(&section(&[("size", text(size))]));
        assert_eq!(settings.place.primary, QuickLength::Percent(50.0), "{size}");
        let error = error.unwrap_or_else(|| panic!("an error for `{size}`"));
        assert_eq!(error.code, Code::Failed);
        assert!(
            error.message.contains(&format!("`{size}`")),
            "{}",
            error.message
        );
    }
    for (size, primary, secondary) in [
        ("100%", QuickLength::Percent(100.0), None),
        (" 12.5% ", QuickLength::Percent(12.5), None),
        (
            "300px, 20%",
            QuickLength::Points(300.0),
            Some(QuickLength::Percent(20.0)),
        ),
    ] {
        assert_eq!(size_of(size), Some((primary, secondary)), "{size}");
    }
    // The plugin starts all the same, of the default's size.
    let (mut host, mut plugin) = host(&[("size", text("big"))]);
    assert_eq!(
        toggle(&mut host, &mut plugin),
        [Request::OpenQuickWindow {
            place: QuickPlace::default(),
            directory: None
        }]
    );
}

#[test]
fn a_closed_quick_window_gives_way_to_a_new_one() {
    let (mut host, mut plugin) = host(&[]);
    toggle(&mut host, &mut plugin);
    let window = quick(&plugin);
    host.close_window(window);
    let done = toggle(&mut host, &mut plugin);
    assert!(
        matches!(done[..], [Request::OpenQuickWindow { .. }]),
        "{done:?}"
    );
    assert_ne!(quick(&plugin), window);
}

#[test]
fn the_guide_tells_every_key_of_the_schema_in_its_example_and_its_section() {
    let guide = include_str!("../../../docs/CONFIGURATION.md");
    let schema = include_str!("../quick-terminal.schema.json");
    let keys: Vec<&str> = schema
        .lines()
        .filter_map(|line| line.strip_prefix("    \"")?.split_once("\": {"))
        .map(|(key, _)| key)
        .collect();
    assert_eq!(
        keys,
        [
            "position",
            "size",
            "screen",
            "animation",
            "autohide",
            "spaces"
        ]
    );
    let example = guide
        .split_once("\n[plugins.quick-terminal]\n")
        .and_then(|(_, rest)| rest.split_once("\n\n"))
        .map(|(example, _)| example)
        .expect("the complete example's section");
    let section = guide
        .split_once("\n## `[plugins.quick-terminal]`\n")
        .map(|(_, rest)| {
            rest.split_once("\n## ")
                .map_or(rest, |(section, _)| section)
        })
        .expect("the guide's section");
    for key in keys {
        assert!(
            example
                .lines()
                .any(|line| line.starts_with(&format!("{key} = "))),
            "`{key}` in the example"
        );
        assert!(
            section.contains(&format!("\n### `{key}`\n")),
            "`{key}` in the section"
        );
    }
}
