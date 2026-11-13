//! The settings tab: a tab of no pane, one at most, opened after the tab
//! shown, its view in the panes' room, its window's title its own, the
//! panes' actions not applying, and closed as a tab, of no question; its
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

/// The settings tab's view in `window`, if it shows.
fn settings_view(app: &App, window: WindowId) -> Option<&super::super::chrome::SlotView> {
    app.windows[&window].chrome.iter().find(|view| {
        view.plugin == super::super::chrome::OVERFLOW
            && view.view == super::super::settings_tab::settings_view()
    })
}

#[test]
fn the_settings_tab_opens_once_after_the_tab_shown_shows_its_page_and_closes_as_a_tab() {
    let (mut fake, mut app, window) = two_tabs();
    let [first, second] = tabs_of(&app, window)[..] else {
        panic!("two tabs");
    };
    // After the tab shown, which it shows, of its own title.
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    let (settings, _) = app.workspace.settings_tab().expect("the settings tab");
    assert_eq!(tabs_of(&app, window), [first, settings, second]);
    assert_eq!(shown(&app, window), settings);
    assert_eq!(fake.window(window).unwrap().title, "Settings");
    // Its view takes the panes' room whole, of its page.
    let view = settings_view(&app, window).expect("its view");
    let room = app.windows[&window].panes_room.unwrap();
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
    assert!(app.windows[&window].views.is_empty(), "no pane shows");
    // The panes' actions do not apply; the text's size does.
    assert!(!fake.command_enabled("pane.select-all"));
    assert!(fake.command_enabled("font.bigger"));
    // Opened again, from another tab: the one there shows.
    app.workspace.activate_tab(first);
    frames(&mut fake, &mut app, window);
    assert!(settings_view(&app, window).is_none());
    assert_ne!(fake.window(window).unwrap().title, "Settings");
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    assert_eq!(tabs_of(&app, window), [first, settings, second]);
    assert_eq!(shown(&app, window), settings);
    assert!(settings_view(&app, window).is_some());
    // Closed as a tab, by the tabs' close, of no question: the tab after
    // it shows.
    fake.send_app(&mut app, AppEvent::Command("tabs.close".to_owned()));
    frames(&mut fake, &mut app, window);
    assert!(fake.alerts().is_empty());
    assert_eq!(app.workspace.settings_tab(), None);
    assert_eq!(tabs_of(&app, window), [first, second]);
    assert_eq!(shown(&app, window), second);
    assert!(settings_view(&app, window).is_none());
}

#[test]
fn the_settings_tab_of_no_window_opens_one() {
    let (mut fake, mut app, window) = started_running(idle());
    fake.send(&mut app, window, WindowEvent::CloseRequested);
    assert_eq!(fake.windows().count(), 0);
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    let (settings, shown) = app.workspace.settings_tab().expect("the settings tab");
    let open = app.workspace.window(shown).expect("a window of it");
    assert_eq!(open.active_tab(), settings);
    assert_eq!(
        open.tabs().len(),
        2,
        "a new window's tab, then the settings tab"
    );
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

/// Presses and lets go of `key` in `window`, of `mods`.
fn tap(fake: &mut Fake, app: &mut App, window: WindowId, key: KeyCode, mods: Modifiers) {
    fake.send(app, window, key_event(key, "", mods, true));
    fake.send(app, window, key_event(key, "", mods, false));
}

#[test]
fn the_settings_tab_s_sidebar_chooses_its_page_by_the_keys_and_a_click_and_has_the_keyboard() {
    let (mut fake, mut app, window) = of_pages();
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "general");
    // The page's view, of its plugin, takes the room beside the sidebar,
    // its plugin told that it shows.
    let room = settings_view(&app, window).unwrap().frame.unwrap();
    let page = page_view(&app, window).unwrap().frame.unwrap();
    let scale = fake.window(window).unwrap().size.scale as f32;
    assert!(page.x >= room.x + 220.0 * scale, "{page:?} of {room:?}");
    assert!((page.x + page.width - (room.x + room.width)).abs() < 1.0);
    assert!((page.height - room.height).abs() < 1.0);
    assert_eq!(page_text(&app, window, "title"), "General");
    assert_eq!(page_text(&app, window, "shown"), "Shown 1 times");
    // The sidebar's list has the keyboard, as the tab shows.
    let view = settings_view(&app, window).unwrap();
    let list = view.surface.tree.by_key(&"pages".into()).unwrap();
    assert_eq!(
        app.windows[&window].focus.target(),
        lokyr_ui_core::focus::Target::Node {
            panel: view.id,
            node: list
        }
    );
    // The arrow down chooses the next page, the arrow up the first again.
    tap(
        &mut fake,
        &mut app,
        window,
        KeyCode::ArrowDown,
        Modifiers::NONE,
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(app.settings.chosen, 1);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "title"), "Terminal");
    tap(
        &mut fake,
        &mut app,
        window,
        KeyCode::ArrowUp,
        Modifiers::NONE,
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "general");
    assert_eq!(page_text(&app, window, "shown"), "Shown 2 times");
    // A click on the second row chooses it.
    let view = settings_view(&app, window).unwrap();
    let frame = view.frame.unwrap();
    let at = view.surface.layout.frame(&view.surface.tree, list).unwrap();
    let scale = fake.window(window).unwrap().size.scale;
    let second = Point {
        x: f64::from(frame.x + at.x + 40.0) / scale,
        y: f64::from(frame.y + at.y) / scale + 28.0 * 1.5,
    };
    fake.send(&mut app, window, click(true, second, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, second, Modifiers::NONE, 1));
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "terminal");
    // Another tab shown: the keyboard goes back to its pane; the settings
    // tab again keeps its page.
    let first = tabs_of(&app, window)[0];
    app.workspace.activate_tab(first);
    frames(&mut fake, &mut app, window);
    assert!(app.windows[&window].pane_has_focus());
    assert!(page_view(&app, window).is_none());
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "shown"), "Shown 3 times");
    let view = settings_view(&app, window).unwrap();
    let selected = view
        .surface
        .lists
        .state(&"pages".into())
        .and_then(lokyr_ui_core::list::ListState::selected);
    assert_eq!(selected, Some(1));
}

#[test]
fn the_settings_tab_s_tree_goes_to_a_client_of_accessibility() {
    use accesskit::Role;
    let (mut fake, mut app, window) = of_pages();
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    fake.ask_accessibility(&mut app, window);
    let trees = &fake.window(window).unwrap().accessibility;
    let tree = trees.last().expect("a tree");
    let roles: Vec<Role> = tree.nodes.iter().map(|(_, node)| node.role()).collect();
    assert!(roles.contains(&Role::ListBox), "{roles:?}");
    assert_eq!(
        roles
            .iter()
            .filter(|role| **role == Role::ListBoxOption)
            .count(),
        2
    );
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
fn tab_goes_from_the_sidebar_to_the_page_s_fields_and_round() {
    let (mut fake, mut app, window) = of_pages();
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("pages".to_owned(), false));
    // The page's field, past the sidebar's list; past the page's last
    // field, the list again, the toggle and the button not reached.
    tap(&mut fake, &mut app, window, KeyCode::Tab, Modifiers::NONE);
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("name".to_owned(), true));
    tap(&mut fake, &mut app, window, KeyCode::Tab, Modifiers::NONE);
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("pages".to_owned(), false));
    // Shift-Tab the other way, and Escape back to the list.
    let shift = Modifiers {
        shift: true,
        ..Modifiers::NONE
    };
    tap(&mut fake, &mut app, window, KeyCode::Tab, shift);
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("name".to_owned(), true));
    tap(
        &mut fake,
        &mut app,
        window,
        KeyCode::Escape,
        Modifiers::NONE,
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(focused_key(&app, window), ("pages".to_owned(), false));
}

#[test]
fn a_plugin_s_ask_of_its_page_opens_the_settings_tab_there_and_the_page_hidden_is_told() {
    let (mut fake, mut app, window) = of_pages();
    let first = tabs_of(&app, window)[0];
    // Its command opens the tab at its second page.
    fake.send_app(
        &mut app,
        AppEvent::Command("sample-pages.show-terminal".to_owned()),
    );
    frames(&mut fake, &mut app, window);
    let (settings, _) = app.workspace.settings_tab().expect("the settings tab");
    assert_eq!(shown(&app, window), settings);
    assert_eq!(page_shown(&app, window), "terminal");
    assert_eq!(page_text(&app, window, "shown"), "Shown 1 times");
    // Another tab shown hides it, as its plugin hears; shown again, it
    // hears it shown.
    app.workspace.activate_tab(first);
    frames(&mut fake, &mut app, window);
    assert!(page_view(&app, window).is_none());
    fake.send_app(
        &mut app,
        AppEvent::Command("sample-pages.show-terminal".to_owned()),
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(page_text(&app, window, "shown"), "Shown 2 times");
    // The page's button shows the same page; the first page chosen in the
    // sidebar shows it, its button then the other.
    tap(
        &mut fake,
        &mut app,
        window,
        KeyCode::ArrowUp,
        Modifiers::NONE,
    );
    frames(&mut fake, &mut app, window);
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
    let (mut fake, mut app, window) = two_tabs();
    // The plugin manager waits for its page or its command.
    assert_eq!(
        app.plugins.status("lokyr/plugin-manager"),
        Some(lokyr_kernel::plugins::Status::Inactive)
    );
    fake.send_app(
        &mut app,
        AppEvent::Command("plugin-manager.open".to_owned()),
    );
    frames(&mut fake, &mut app, window);
    let (settings, _) = app.workspace.settings_tab().expect("the settings tab");
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
