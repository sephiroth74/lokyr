//! The titlebar's buttons beside the window's: a plugin's button of an icon
//! alone right after the window's buttons, a square of the host's size,
//! which a tab bar in the titlebar starts after, its press its own; three
//! at most, the rest in a menu of the app's button; a button of a text
//! refused.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{Anchor, PanelId, PanelOptions, Role, Slot, UiEvent, ViewId};
use lokyr_kernel::plugins::view::{Layout as NodeLayout, Length, Node, UiEventKind};
use lokyr_platform_core::{MenuItem, Modifiers, Point};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::layout::Frame;

use super::*;
use crate::fakes;

/// A plugin of a button in the titlebar, of the name `name`, which keeps
/// the clicks that its nodes take, by its name, and opens a menu's panel
/// anchored to its button at the click; of a button of a text where
/// `worded`; and of a tab bar of a text where `bar`.
struct Button {
    name: &'static str,
    worded: bool,
    bar: bool,
    clicks: Rc<RefCell<Vec<String>>>,
}

/// The manifest of the plugin `lokyr/<name>`, of the titlebar's buttons
/// and, where `bar`, of the tab bar.
fn manifest(name: &str, bar: bool) -> String {
    let slots = if bar {
        "[\"title-buttons\", \"tab-bar\"]"
    } else {
        "[\"title-buttons\"]"
    };
    format!(
        "[plugin]\nname = \"lokyr/{name}\"\nkind = \"builtin\"\nlicense = \"MIT\"\n[contributes]\nslots = {slots}\n"
    )
}

impl plugins::Plugin for Button {
    fn activate(&mut self, _: &mut plugins::Cx<'_>) -> Result<(), Error> {
        Ok(())
    }

    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        if let ViewId::Panel(_) = view {
            return Some(Node::container(vec![Node::button("One").key("one")]));
        }
        match view.slot()? {
            Slot::TitleButtons if self.worded => {
                Some(Node::button("Awake").key("b").tooltip(self.name))
            }
            // Its own size, which the host's replaces.
            Slot::TitleButtons => Some(
                Node::icon_button("circle")
                    .key("b")
                    .tooltip(self.name)
                    .layout(NodeLayout {
                        width: Length::Points(40.0),
                        height: Length::Points(10.0),
                        ..NodeLayout::default()
                    }),
            ),
            Slot::TabBar if self.bar => Some(Node::container(vec![Node::text("tabs")]).layout(
                NodeLayout {
                    grow: 1.0,
                    ..NodeLayout::default()
                },
            )),
            _ => None,
        }
    }

    fn on_ui_event(&mut self, cx: &mut plugins::Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        if matches!(event.kind, UiEventKind::Click) && event.key.0 == "b" {
            self.clicks.borrow_mut().push(self.name.to_owned());
            cx.open_panel(
                "menu",
                PanelOptions {
                    anchor: Anchor::Node {
                        view: event.view.clone(),
                        key: event.key.clone(),
                    },
                    role: Role::Dialog,
                    close_outside: true,
                    close_escape: true,
                    takes_keys: false,
                },
            )?;
        }
        Ok(())
    }
}

/// An app of the buttons of `names`, the first of the tab bar too, and of
/// a text where `worded`, of a titlebar of 28 points, its window drawn:
/// the fake, the app, its window and the clicks that the buttons took.
fn app_of(names: &[&'static str], worded: bool) -> (Fake, App, WindowId, Rc<RefCell<Vec<String>>>) {
    let clicks = Rc::new(RefCell::new(Vec::new()));
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut config = unpadded();
    config.slots.tab_bar = Some(format!("lokyr/{}", names[0]));
    let mut app = App::new(Some(drawing), config, None).with_program(quiet());
    for (at, name) in names.iter().enumerate() {
        let plugin = Button {
            name,
            worded,
            bar: at == 0,
            clicks: Rc::clone(&clicks),
        };
        app = app.with_builtin(&manifest(name, at == 0), Box::new(plugin));
    }
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    for _ in 0..4 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    (fake, app, window, clicks)
}

/// The frames of `window`'s titlebar buttons that show, by their plugins,
/// the app's of the rest `…`, in their order.
fn buttons(app: &App, window: WindowId) -> Vec<(String, Frame)> {
    let mut shown: Vec<(String, Frame)> = app.windows[&window]
        .chrome
        .iter()
        .filter(|view| matches!(view.view, ViewId::TitleButtons(_)))
        .filter_map(|view| {
            let name = match view.plugin.as_str() {
                "" => "…".to_owned(),
                plugin => plugin.trim_start_matches("lokyr/").to_owned(),
            };
            Some((name, view.frame?))
        })
        .collect();
    shown.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    shown
}

/// Clicks `window` in the middle of `frame`.
fn click_in(fake: &mut Fake, app: &mut App, window: WindowId, frame: Frame, scale: f32) {
    let at = Point {
        x: f64::from((frame.x + frame.width / 2.0) / scale),
        y: f64::from((frame.y + frame.height / 2.0) / scale),
    };
    fake.send(app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(app, window, click(false, at, Modifiers::NONE, 1));
}

#[test]
fn a_title_button_sits_after_the_window_s_buttons_and_takes_its_press() {
    let (mut fake, mut app, window, clicks) = app_of(&["sleepless"], false);
    let size = fake.window(window).unwrap().size;
    let scale = size.scale as f32;
    let shown = buttons(&app, window);
    assert_eq!(shown.len(), 1, "{shown:?}");
    let button = shown[0].1;
    let tab_bar = app.windows[&window]
        .chrome
        .iter()
        .find(|view| matches!(view.view, ViewId::TabBar(_)))
        .and_then(|view| view.frame)
        .expect("a frame");
    // Right after the window's buttons, a square of 24 points in the
    // middle of its room of 28 and of the titlebar's height, whatever its
    // plugin's size; the tab bar after its room.
    let after = (size.buttons * size.scale) as f32;
    assert_eq!(button.x, after + 2.0 * scale);
    assert_eq!(button.y, 2.0 * scale);
    assert_eq!((button.width, button.height), (24.0 * scale, 24.0 * scale));
    assert_eq!(tab_bar.x, after + 28.0 * scale);
    // Its button's press its own, of the titlebar's areas.
    let areas = &fake.window(window).unwrap().titlebar_areas;
    let at = Point {
        x: f64::from((button.x + button.width / 2.0) / scale),
        y: f64::from((button.y + button.height / 2.0) / scale),
    };
    assert!(areas.iter().any(|area| area.contains(at)), "{areas:?}");
    click_in(&mut fake, &mut app, window, button, scale);
    assert_eq!(*clicks.borrow(), ["sleepless"]);
}

#[test]
fn three_title_buttons_show_and_of_more_two_and_a_menu_of_the_rest() {
    let (_, app, window, _) = app_of(&["a", "b", "c"], false);
    let names: Vec<String> = buttons(&app, window)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert_eq!(names, ["a", "b", "c"]);
    let (mut fake, mut app, window, clicks) = app_of(&["a", "b", "c", "d", "e"], false);
    let scale = fake.window(window).unwrap().size.scale as f32;
    let shown = buttons(&app, window);
    let names: Vec<&str> = shown.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["a", "b", "…"]);
    // The app's button lists the rest by their names, below it.
    let more = shown[2].1;
    click_in(&mut fake, &mut app, window, more, scale);
    assert!(clicks.borrow().is_empty());
    let (over, at, menu) = fake.context_menus().last().cloned().expect("its menu");
    assert_eq!(over, window);
    assert_eq!(
        (at.x, at.y),
        (
            f64::from(more.x / scale),
            f64::from((more.y + more.height) / scale)
        )
    );
    let items: Vec<(String, String)> = menu
        .items
        .iter()
        .filter_map(|item| match item {
            MenuItem::Command { title, command, .. } => Some((title.clone(), command.clone())),
            _ => None,
        })
        .collect();
    let titles: Vec<&str> = items.iter().map(|(title, _)| title.as_str()).collect();
    assert_eq!(titles, ["c", "d", "e"]);
    // A choice is its button's click, its panel below the app's button.
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand(items[2].1.clone()),
    );
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].chrome.iter().any(|view| {
            view.view == ViewId::Panel(PanelId("menu".to_owned())) && view.frame.is_some()
        })
    });
    assert_eq!(*clicks.borrow(), ["e"]);
    let panel = app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view == ViewId::Panel(PanelId("menu".to_owned())))
        .and_then(|view| view.frame)
        .unwrap();
    assert!(panel.y >= more.y + more.height, "{panel:?} below {more:?}");
    assert!(
        (panel.x - more.x).abs() < panel.width + more.width,
        "{panel:?} near {more:?}"
    );
}

#[test]
fn a_title_button_of_a_text_shows_nothing() {
    let (_, app, window, _) = app_of(&["worded"], true);
    assert!(buttons(&app, window).is_empty());
    let shows = app.windows[&window]
        .chrome
        .iter()
        .any(|view| matches!(view.view, ViewId::TitleButtons(_)) && view.shows());
    assert!(!shows);
}
