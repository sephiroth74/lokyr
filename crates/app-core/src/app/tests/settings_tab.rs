//! The settings, in a window of their own, one at most, of no pane and no
//! number, their view in the window's room, its title theirs, the panes'
//! actions not applying, and closed as a window, of no question; their
//! sidebar of the plugins' pages, the chosen page's view beside it, which
//! Tab goes through with the sidebar, its plugin told as it shows and
//! hides, and a plugin's ask of its own page.

use lokyr_kernel::plugins::ui::ViewId;
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// The app of a window of two tabs, the first shown, drawn.
fn two_tabs() -> (Fake, App, WindowId) {
    two_tabs_of(false)
}

/// The app of [`two_tabs`], of the sample plugin of two pages if `pages`,
/// the plugin manager turned off, whose page would come first.
fn two_tabs_of(pages: bool) -> (Fake, App, WindowId) {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let config = if pages {
        Config::parse(
            "[plugins.plugin-manager]\nenabled = false\n",
            std::path::Path::new("config.toml"),
        )
        .unwrap()
    } else {
        Config::default()
    };
    let mut app = App::new(Some(drawing), config, None).with_program(idle());
    if pages {
        app.plugins
            .add_builtin(
                crate::sample_pages::MANIFEST,
                Box::new(crate::sample_pages::SamplePages::default()),
            )
            .unwrap();
    }
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    fake.send_app(&mut app, AppEvent::Command("tabs.new".to_owned()));
    let first = tabs_of(&app, window)[0];
    app.workspace.activate_tab(first);
    frames(&mut fake, &mut app, window);
    (fake, app, window)
}

/// The tabs of `window`, in their order.
fn tabs_of(app: &App, window: WindowId) -> Vec<lokyr_kernel::workspace::TabId> {
    let shown = app.windows[&window].window;
    app.workspace.window(shown).unwrap().tabs().to_vec()
}

/// The tab that `window` shows.
fn shown(app: &App, window: WindowId) -> lokyr_kernel::workspace::TabId {
    let shown = app.windows[&window].window;
    app.workspace.window(shown).unwrap().active_tab()
}

/// A few frames of `window`.
fn frames(fake: &mut Fake, app: &mut App, window: WindowId) {
    for _ in 0..3 {
        fake.send(app, window, WindowEvent::Frame);
    }
}

/// The settings' window of their own, if they are open.
fn settings_window(app: &App) -> Option<WindowId> {
    let shown = app.workspace.settings_window()?;
    app.windows
        .iter()
        .find(|(_, state)| state.window == shown)
        .map(|(&window, _)| window)
}

/// Runs `command`, which opens the settings, and their window, drawn, of
/// the keyboard's focus.
fn open(fake: &mut Fake, app: &mut App, command: &str) -> WindowId {
    fake.send_app(app, AppEvent::Command(command.to_owned()));
    let own = settings_window(app).expect("the settings' window");
    let state = app.windows.get_mut(&own).unwrap();
    if state.surface.is_none() {
        state.surface = Some(fakes::Surface::default());
    }
    fake.send(app, own, WindowEvent::Focused(true));
    frames(fake, app, own);
    own
}

/// Closes the settings' window, as ⌘W does, which has the focus.
fn close(fake: &mut Fake, app: &mut App, own: WindowId) {
    fake.send(app, own, WindowEvent::CloseRequested);
}

/// The settings tab's view in `window`, if it shows.
fn settings_view(app: &App, window: WindowId) -> Option<&super::super::chrome::SlotView> {
    app.windows[&window].chrome.iter().find(|view| {
        view.plugin == super::super::chrome::OVERFLOW
            && view.view == super::super::settings_tab::settings_view()
    })
}

#[test]
fn the_settings_open_in_a_window_of_their_own_once_and_close_as_a_window() {
    let (mut fake, mut app, window) = two_tabs();
    let [first, second] = tabs_of(&app, window)[..] else {
        panic!("two tabs");
    };
    // A window of their own, of their title and of System Settings' size,
    // of no number, the terminal's window as it was.
    let own = open(&mut fake, &mut app, SETTINGS_TAB);
    assert_ne!(own, window);
    let (settings, _) = app.workspace.settings_tab().expect("the settings");
    assert_eq!(tabs_of(&app, own), [settings]);
    assert_eq!(tabs_of(&app, window), [first, second]);
    assert_eq!(fake.window(own).unwrap().title, "Settings");
    let points = fake.window(own).unwrap().size.points;
    assert_eq!((points.width, points.height), (780.0, 560.0));
    let shown = app.windows[&own].window;
    assert_eq!(app.workspace.window(shown).unwrap().number(), None);
    // Their view takes the window's room whole.
    let view = settings_view(&app, own).expect("their view");
    let room = app.windows[&own].panes_room.unwrap();
    let frame = view.frame.expect("its frame");
    assert_eq!(
        (frame.x, frame.y, frame.width, frame.height),
        (
            room.x as f32,
            room.y as f32,
            room.width as f32,
            room.height as f32
        )
    );
    assert!(view.surface.tree.by_key(&"page".into()).is_some());
    assert!(app.windows[&own].views.is_empty(), "no pane shows");
    // The panes' actions do not apply; the text's size does.
    assert!(!fake.command_enabled("pane.select-all"));
    assert!(fake.command_enabled("font.bigger"));
    // None of the plugins' views of the terminals' windows: no slot's, as
    // the tab bar, the progress' strip or the titlebar's buttons, and no
    // panel of a plugin's, as the palette or a tip, but the pages'.
    let foreign = |app: &App| {
        app.windows[&own]
            .chrome
            .iter()
            .filter(|view| {
                view.plugin != super::super::chrome::OVERFLOW
                    && !matches!(view.view, ViewId::Page(_))
            })
            .map(|view| format!("{} {:?}", view.plugin, view.view))
            .collect::<Vec<_>>()
    };
    assert!(foreign(&app).is_empty(), "{:?}", foreign(&app));
    assert!(
        app.windows[&window]
            .chrome
            .iter()
            .any(|view| view.view.slot().is_some()),
        "the terminals' window has its slots"
    );
    for command in ["command-palette.open", "quick-tips.show"] {
        fake.send_app(&mut app, AppEvent::Command(command.to_owned()));
        frames(&mut fake, &mut app, own);
        assert!(foreign(&app).is_empty(), "{command}: {:?}", foreign(&app));
    }
    assert!(fake.alerts().is_empty());
    assert!(app.diagnostics.shown().is_none());
    // A new tab, as ⌘T asks there, opens nowhere: the settings' window
    // takes none.
    fake.send_app(&mut app, AppEvent::Command("tabs.new".to_owned()));
    frames(&mut fake, &mut app, own);
    assert_eq!(tabs_of(&app, own), [settings]);
    assert_eq!(tabs_of(&app, window), [first, second]);
    assert_eq!(fake.windows().count(), 2);
    // Opened again, from the terminal's window: the same, in front.
    fake.send(&mut app, window, WindowEvent::Focused(true));
    assert_eq!(open(&mut fake, &mut app, SETTINGS_TAB), own);
    assert_eq!(fake.windows().count(), 2);
    // Closed as a window, of no question.
    close(&mut fake, &mut app, own);
    assert!(fake.alerts().is_empty());
    assert_eq!(app.workspace.settings_tab(), None);
    assert_eq!(fake.windows().collect::<Vec<_>>(), [window]);
    assert_eq!(tabs_of(&app, window), [first, second]);
}

#[test]
fn the_settings_of_no_window_open_their_own_alone() {
    let (mut fake, mut app, window) = started_running(idle());
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().count(), 0);
    let own = open(&mut fake, &mut app, SETTINGS_TAB);
    let (settings, _) = app.workspace.settings_tab().expect("the settings");
    assert_eq!(tabs_of(&app, own), [settings]);
    assert_eq!(fake.windows().count(), 1);
}

/// The app of [`two_tabs`], of the sample plugin's two pages, `General`
/// and `Terminal`.
fn of_pages() -> (Fake, App, WindowId) {
    two_tabs_of(true)
}

/// The view of the settings tab's page that shows in `window`, if one
/// does.
fn page_view(app: &App, window: WindowId) -> Option<&super::super::chrome::SlotView> {
    app.windows[&window]
        .chrome
        .iter()
        .find(|view| matches!(view.view, ViewId::Page(_)))
}

/// The id of the page that the settings tab shows, of its plugin's view,
/// once laid out.
fn page_shown(app: &App, window: WindowId) -> String {
    let view = page_view(app, window).expect("its page's view");
    assert!(view.frame.is_some(), "its page laid out");
    match &view.view {
        ViewId::Page(page) => page.clone(),
        _ => unreachable!(),
    }
}

/// The text of the node `key` of the page's view in `window`.
fn page_text(app: &App, window: WindowId, key: &str) -> String {
    let tree = &page_view(app, window)
        .expect("its page's view")
        .surface
        .tree;
    match tree.by_key(&key.into()).and_then(|node| tree.kind(node)) {
        Some(lokyr_kernel::plugins::view::Kind::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

/// The key of the node with the focus in `window`, and whether it is of
/// the page's view, or `pane` for none.
fn focused_key(app: &App, window: WindowId) -> (String, bool) {
    let state = &app.windows[&window];
    match state.focus.target() {
        lokyr_ui_core::focus::Target::Node { panel, node } => {
            let view = state.chrome.iter().find(|view| view.id == panel).unwrap();
            let key = view.surface.tree.key(node).unwrap().0.clone();
            (key, matches!(view.view, ViewId::Page(_)))
        }
        lokyr_ui_core::focus::Target::Pane => ("pane".to_owned(), false),
    }
}

/// Clicks the settings' tab of the page `index` in `window`.
fn click_tab(fake: &mut Fake, app: &mut App, window: WindowId, index: usize) {
    let view = settings_view(app, window).unwrap();
    let frame = view.frame.unwrap();
    let node = view.surface.tree.by_key(&"pages".into()).unwrap();
    let segment = view.surface.layout.segments(&view.surface.tree, node)[index];
    let scale = fake.window(window).unwrap().size.scale;
    let at = Point {
        x: f64::from(frame.x + segment.x + segment.width / 2.0) / scale,
        y: f64::from(frame.y + segment.y + segment.height / 2.0) / scale,
    };
    fake.send(app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(app, window, click(false, at, Modifiers::NONE, 1));
    frames(fake, app, window);
}

/// Presses and lets go of `key` in `window`, of `mods`.
fn tap(fake: &mut Fake, app: &mut App, window: WindowId, key: KeyCode, mods: Modifiers) {
    fake.send(app, window, key_event(key, "", mods, true));
    fake.send(app, window, key_event(key, "", mods, false));
}

#[test]
fn the_settings_pages_are_tabs_across_the_top_chosen_by_a_click_and_the_keys() {
    let (mut fake, mut app, _) = of_pages();
    let window = open(&mut fake, &mut app, SETTINGS_TAB);
    assert_eq!(page_shown(&app, window), "general");
    // The page's view, of its plugin, takes the room below the tabs, in
    // their box, its plugin told that it shows.
    let room = settings_view(&app, window).unwrap().frame.unwrap();
    let page = page_view(&app, window).unwrap().frame.unwrap();
    assert!(page.y > room.y + 20.0, "{page:?} of {room:?}");
    assert!(page.x > room.x && page.x + page.width < room.x + room.width);
    assert!(page.y + page.height < room.y + room.height);
    assert!(page.height > room.height / 2.0, "{page:?} of {room:?}");
    assert_eq!(page_text(&app, window, "title"), "General");
    assert_eq!(page_text(&app, window, "shown"), "Shown 1 times");
    // The page's first field has the keyboard, as the settings open.
    assert_eq!(focused_key(&app, window), ("name".to_owned(), true));
    // A click on the second tab chooses it, the first again.
    click_tab(&mut fake, &mut app, window, 1);
    assert_eq!(app.settings.chosen, 1);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "title"), "Terminal");
    click_tab(&mut fake, &mut app, window, 0);
    assert_eq!(page_shown(&app, window), "general");
    assert_eq!(page_text(&app, window, "shown"), "Shown 2 times");
    // The arrows, as the tabs have the keyboard.
    let state = app.windows.get_mut(&window).unwrap();
    let view = state
        .chrome
        .iter()
        .find(|view| view.view == super::super::settings_tab::settings_view())
        .unwrap();
    let node = view.surface.tree.by_key(&"pages".into()).unwrap();
    assert!(state.focus.focus(view.id, &view.surface.tree, node));
    tap(
        &mut fake,
        &mut app,
        window,
        KeyCode::ArrowRight,
        Modifiers::NONE,
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "terminal");
    // Closed and opened again, they keep their page.
    close(&mut fake, &mut app, window);
    let window = open(&mut fake, &mut app, SETTINGS_TAB);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "shown"), "Shown 3 times");
    let view = settings_view(&app, window).unwrap();
    let node = view.surface.tree.by_key(&"pages".into()).unwrap();
    match view.surface.tree.kind(node) {
        Some(lokyr_kernel::plugins::view::Kind::Tabs(tabs)) => assert_eq!(tabs.chosen, 1),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_settings_tab_s_tree_goes_to_a_client_of_accessibility() {
    use accesskit::Role;
    let (mut fake, mut app, _) = of_pages();
    let window = open(&mut fake, &mut app, SETTINGS_TAB);
    fake.ask_accessibility(&mut app, window);
    let trees = &fake.window(window).unwrap().accessibility;
    let tree = trees.last().expect("a tree");
    let roles: Vec<Role> = tree.nodes.iter().map(|(_, node)| node.role()).collect();
    assert!(roles.contains(&Role::TabList), "{roles:?}");
    assert_eq!(roles.iter().filter(|role| **role == Role::Tab).count(), 2);
    assert!(!roles.contains(&Role::Terminal), "{roles:?}");
    let named = |name: &str| {
        tree.nodes
            .iter()
            .any(|(_, node)| node.label().is_some_and(|label| label.contains(name)))
    };
    assert!(named("General") && named("Terminal"));
    // The page's nodes too, its field among them.
    assert!(named("Name"), "{roles:?}");
    assert!(tree.focus != accesskit::NodeId(0));
}

#[test]
fn tab_goes_through_the_page_s_fields_and_the_tabs_as_the_system_s_keyboard_navigation_has_it() {
    let (mut fake, mut app, _) = of_pages();
    let window = open(&mut fake, &mut app, SETTINGS_TAB);
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    // Of its fields alone, as by default: the page's one field.
    assert_eq!(focused_key(&app, window), ("name".to_owned(), true));
    tap(&mut fake, &mut app, window, KeyCode::Tab, Modifiers::NONE);
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("name".to_owned(), true));
    // Of every control, as macOS's Keyboard navigation has it: the page's
    // toggle and button, then the tabs, then the field again.
    fake.set_keyboard_navigation(true);
    for wanted in [
        ("tips", true),
        ("show-terminal", true),
        ("pages", false),
        ("name", true),
    ] {
        tap(&mut fake, &mut app, window, KeyCode::Tab, Modifiers::NONE);
        frames(&mut fake, &mut app, window);
        assert_eq!(focused_key(&app, window), (wanted.0.to_owned(), wanted.1));
    }
    tap(&mut fake, &mut app, window, KeyCode::Tab, shift);
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("pages".to_owned(), false));
}

#[test]
fn a_plugin_s_ask_of_its_page_opens_the_settings_tab_there_and_the_page_hidden_is_told() {
    let (mut fake, mut app, _) = of_pages();
    // Its command opens the settings at its second page.
    let window = open(&mut fake, &mut app, "sample-pages.show-terminal");
    let (settings, _) = app.workspace.settings_tab().expect("the settings");
    assert_eq!(shown(&app, window), settings);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "shown"), "Shown 1 times");
    // Their window closed hides it, as its plugin hears; shown again, it
    // hears it shown.
    close(&mut fake, &mut app, window);
    let window = open(&mut fake, &mut app, "sample-pages.show-terminal");
    assert_eq!(page_text(&app, window, "shown"), "Shown 2 times");
    // The first page chosen among the tabs shows it, its button then the
    // other.
    click_tab(&mut fake, &mut app, window, 0);
    assert_eq!(page_shown(&app, window), "general");
    let view = page_view(&app, window).unwrap();
    let frame = view.frame.unwrap();
    let button = view.surface.tree.by_key(&"show-terminal".into()).unwrap();
    let at = view
        .surface
        .layout
        .frame(&view.surface.tree, button)
        .unwrap();
    let scale = fake.window(window).unwrap().size.scale;
    let middle = Point {
        x: f64::from(frame.x + at.x + at.width / 2.0) / scale,
        y: f64::from(frame.y + at.y + at.height / 2.0) / scale,
    };
    fake.send(&mut app, window, click(true, middle, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, middle, Modifiers::NONE, 1));
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "shown"), "Shown 3 times");
}

#[test]
fn lokyr_s_plugins_item_opens_the_settings_tab_at_the_plugin_manager_s_page_of_every_plugin() {
    let (mut fake, mut app, _) = two_tabs();
    // The plugin manager waits for its page or its command.
    assert_eq!(
        app.plugins.status("lokyr/plugin-manager"),
        Some(lokyr_kernel::plugins::Status::Inactive)
    );
    let window = open(&mut fake, &mut app, "plugin-manager.open");
    let (settings, _) = app.workspace.settings_tab().expect("the settings");
    assert_eq!(shown(&app, window), settings);
    assert_eq!(page_shown(&app, window), "plugins");
    assert_eq!(app.settings.pages[0].title, "Plugins");
    assert_eq!(app.settings.pages[0].icon, "extensions");
    // Its built-in plugins, each of a button that turns it off, but its
    // own, and of no removal.
    let tree = &page_view(&app, window).unwrap().surface.tree;
    let has = |key: &str| tree.by_key(&key.into()).is_some();
    assert!(has("plugin:lokyr/tabs"));
    assert!(has("toggle:lokyr/tabs"));
    assert!(!has("remove:lokyr/tabs"));
    assert!(has("plugin:lokyr/plugin-manager"));
    assert!(!has("toggle:lokyr/plugin-manager"));
    assert!(has("none-installed"));
    // Narrow, each plugin's buttons stay in its row, at its right, and its
    // description takes more lines.
    let height = fake.window(window).unwrap().size.points.height;
    fake.send(
        &mut app,
        window,
        WindowEvent::Resized(WindowSize::new(
            Size {
                width: 420.0,
                height,
            },
            2.0,
        )),
    );
    frames(&mut fake, &mut app, window);
    let view = page_view(&app, window).unwrap();
    let (tree, layout) = (&view.surface.tree, &view.surface.layout);
    let frame = |key: &str| {
        let node = tree.by_key(&key.into()).unwrap_or_else(|| panic!("{key}"));
        layout.frame(tree, node).unwrap()
    };
    let row = frame("plugin:lokyr/tabs");
    let actions = frame("actions:lokyr/tabs");
    assert!(
        actions.x + actions.width <= row.x + row.width + 0.5,
        "{actions:?} past {row:?}"
    );
    assert!(
        actions.x > row.x + row.width / 2.0,
        "{actions:?} of {row:?}"
    );
    let description = frame("description:lokyr/tabs");
    let line = frame("state:lokyr/tabs").height;
    assert!(
        description.height > line * 1.5,
        "{description:?}, a line {line}"
    );
    assert!(
        description.x + description.width <= actions.x,
        "{description:?}"
    );
    // The wheel scrolls the page down, past the window's height.
    let width = fake.window(window).unwrap().size.points.width;
    fake.send(
        &mut app,
        window,
        WindowEvent::Resized(WindowSize::new(
            Size {
                width,
                height: 300.0,
            },
            2.0,
        )),
    );
    frames(&mut fake, &mut app, window);
    let view = page_view(&app, window).unwrap();
    let frame = view.frame.unwrap();
    let scroll = view.surface.tree.by_key(&"plugins-scroll".into()).unwrap();
    assert!(view.surface.layout.scroll_room(&view.surface.tree, scroll) > 0.0);
    let scale = fake.window(window).unwrap().size.scale;
    let middle = Point {
        x: f64::from(frame.x + frame.width / 2.0) / scale,
        y: f64::from(frame.y + frame.height / 2.0) / scale,
    };
    fake.send(
        &mut app,
        window,
        WindowEvent::Scroll {
            delta: ScrollDelta::Lines { x: 0.0, y: -3.0 },
            phase: None,
            momentum: false,
            position: middle,
            mods: Modifiers::NONE,
        },
    );
    frames(&mut fake, &mut app, window);
    let view = page_view(&app, window).unwrap();
    let scroll = view.surface.tree.by_key(&"plugins-scroll".into()).unwrap();
    assert!(view.surface.layout.scrolled(scroll) > 0.0);
}
