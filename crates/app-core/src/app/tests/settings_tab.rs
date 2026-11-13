//! The settings tab: a tab of no pane, one at most, opened after the tab
//! shown, its view in the panes' room, its window's title its own, the
//! panes' actions not applying, and closed as a tab, of no question.

use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// The app of a window of two tabs, the first shown, drawn.
fn two_tabs() -> (Fake, App, WindowId) {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), Config::default(), None).with_program(idle());
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

/// The app of [`two_tabs`], of two pages, `General` and `Plugins`.
fn of_pages() -> (Fake, App, WindowId) {
    let (fake, mut app, window) = two_tabs();
    app.settings.pages = ["General", "Plugins"]
        .into_iter()
        .map(|title| super::super::settings_tab::Page {
            id: title.to_lowercase(),
            title: title.to_owned(),
            icon: "gear".to_owned(),
        })
        .collect();
    (fake, app, window)
}

/// The title that the settings tab's page shows.
fn page_shown(app: &App, window: WindowId) -> String {
    let view = settings_view(app, window).expect("its view");
    let tree = &view.surface.tree;
    let page = tree.by_key(&"page".into()).expect("its page");
    match tree
        .children(page)
        .first()
        .and_then(|&text| tree.kind(text))
    {
        Some(lokyr_kernel::plugins::view::Kind::Text(text)) => text.clone(),
        _ => String::new(),
    }
}

#[test]
fn the_settings_tab_s_sidebar_chooses_its_page_by_the_keys_and_a_click_and_has_the_keyboard() {
    let (mut fake, mut app, window) = of_pages();
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "General");
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
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowDown, "", Modifiers::NONE, true),
    );
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowDown, "", Modifiers::NONE, false),
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(app.settings.chosen, 1);
    assert_eq!(page_shown(&app, window), "Plugins");
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowUp, "", Modifiers::NONE, true),
    );
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::ArrowUp, "", Modifiers::NONE, false),
    );
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "General");
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
    assert_eq!(page_shown(&app, window), "Plugins");
    // Another tab shown: the keyboard goes back to its pane; the settings
    // tab again keeps its page.
    let first = tabs_of(&app, window)[0];
    app.workspace.activate_tab(first);
    frames(&mut fake, &mut app, window);
    assert!(app.windows[&window].pane_has_focus());
    fake.send_app(&mut app, AppEvent::Command(SETTINGS_TAB.to_owned()));
    frames(&mut fake, &mut app, window);
    assert_eq!(page_shown(&app, window), "Plugins");
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
    assert!(named("General") && named("Plugins"));
    assert!(tree.focus != accesskit::NodeId(0));
}
