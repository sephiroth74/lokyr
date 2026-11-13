//! The titlebar's buttons beside the window's: a plugin's button of an icon
//! alone right after the window's buttons, a square of the host's size,
//! which a tab bar in the titlebar starts after, its press its own; three
//! at most, the rest in a menu of the app's button; a button of a text
//! refused.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{Anchor, PanelId, PanelOptions, Role, Slot, UiEvent, ViewId};
use lokyr_kernel::plugins::view::{Layout as NodeLayout, Length, Node, UiEventKind};
use lokyr_platform_core::{MenuItem, Modifiers, Point, Titlebar};
use lokyr_text_core::FontConfig;
use lokyr_ui_core::layout::Frame;

use super::*;
use crate::fakes;

/// A plugin of a button in the titlebar, of the name `name`, which keeps
/// the clicks that its nodes take, by its name, and opens a menu's panel
/// anchored to its button at the click; of a button of a text where
/// `worded`; and of a tab bar of a text where `bar`, at `place` where it
/// gives one, as a title across the titlebar, of a room of the window's
/// buttons first, its text keyed `t`.
struct Button {
    name: &'static str,
    worded: bool,
    bar: bool,
    place: Option<plugins::host::BarPlace>,
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
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        if let Some(place) = self.place {
            cx.set_tab_bar_place(place)?;
        }
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
            Slot::TabBar if self.bar && self.place.is_some() => {
                let room = Node::container(Vec::new()).layout(NodeLayout {
                    width: Length::Buttons,
                    min_width: Length::Buttons,
                    height: Length::Points(10.0),
                    ..NodeLayout::default()
                });
                Some(
                    Node::container(vec![room, Node::text("tabs").key("t")]).layout(NodeLayout {
                        grow: 1.0,
                        ..NodeLayout::default()
                    }),
                )
            }
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
                    close_typing: false,
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
    app_titled(names, worded, false)
}

/// As [`app_of`], the tab bar a title across the titlebar where `titled`.
fn app_titled(
    names: &[&'static str],
    worded: bool,
    titled: bool,
) -> (Fake, App, WindowId, Rc<RefCell<Vec<String>>>) {
    app_placed(
        names,
        worded,
        titled.then_some(plugins::host::BarPlace::Title),
    )
}

/// As [`app_of`], the tab bar at `place` where it gives one.
fn app_placed(
    names: &[&'static str],
    worded: bool,
    place: Option<plugins::host::BarPlace>,
) -> (Fake, App, WindowId, Rc<RefCell<Vec<String>>>) {
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
            place,
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

#[test]
fn a_title_button_over_a_title_across_the_titlebar_takes_its_hover_and_press() {
    let (mut fake, mut app, window, clicks) = app_titled(&["over"], false, true);
    let scale = fake.window(window).unwrap().size.scale as f32;
    let button = buttons(&app, window)[0].1;
    // The title's view across the whole titlebar, under the button.
    let title = app.windows[&window]
        .chrome
        .iter()
        .find(|view| matches!(view.view, ViewId::TabBar(_)))
        .and_then(|view| view.frame)
        .expect("the title's frame");
    assert_eq!(title.x, 0.0);
    assert!(title.x + title.width >= button.x + button.width);
    // The pointer over the button is the button's.
    let at = Point {
        x: f64::from((button.x + button.width / 2.0) / scale),
        y: f64::from((button.y + button.height / 2.0) / scale),
    };
    fake.send(&mut app, window, moved(at, Modifiers::NONE));
    let over = app.windows[&window].over;
    let button_id = app.windows[&window]
        .chrome
        .iter()
        .find(|view| matches!(view.view, ViewId::TitleButtons(_)))
        .map(|view| view.id);
    assert_eq!(over, button_id);
    click_in(&mut fake, &mut app, window, button, scale);
    assert_eq!(*clicks.borrow(), ["over"]);
}

/// A plugin on a thread of its own of a titlebar button, whose icon its
/// command flips, as a tab's title of `flip` does, of the event of its
/// change, as of a mode and a state that change: each asks for its render.
#[derive(Default)]
struct Flipper {
    flipped: bool,
    titled: bool,
    views: Vec<ViewId>,
}

impl Flipper {
    fn render_again(&self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for view in &self.views {
            cx.request_render(view.clone())?;
        }
        Ok(())
    }
}

impl plugins::Plugin for Flipper {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        cx.register_command(lokyr_kernel::commands::Command {
            name: "flipper.flip".to_owned(),
            title: "Flip".to_owned(),
            category: lokyr_kernel::commands::Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.subscribe(&["tab-changed"])
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        self.flipped = !self.flipped;
        self.render_again(cx)
    }

    fn on_event(&mut self, cx: &mut plugins::Cx<'_>, _: &[plugins::Event]) -> Result<(), Error> {
        let mut titled = false;
        for window in cx.windows()? {
            titled |= cx.tabs(window.id)?.iter().any(|tab| tab.title == "flip");
        }
        if titled != self.titled {
            self.titled = titled;
            self.render_again(cx)?;
        }
        Ok(())
    }

    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        if !self.views.contains(view) {
            self.views.push(view.clone());
        }
        let icon = if self.flipped == self.titled {
            "eye-closed"
        } else {
            "coffee"
        };
        Some(Node::icon_button(icon).key("b").tooltip("Flip"))
    }
}

#[test]
fn a_title_button_of_a_plugin_on_its_thread_shows_its_render_anew_of_no_other_frame() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/flipper\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"title-buttons\"]\n",
    );
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(quiet())
        .with_builtin_send(manifest, Box::new(Flipper::default()));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    let icon = |app: &App| -> Option<String> {
        let view = app.windows[&window]
            .chrome
            .iter()
            .find(|view| matches!(view.view, ViewId::TitleButtons(_)))?;
        let tree = &view.surface.tree;
        match tree.kind(tree.root()?)? {
            lokyr_kernel::plugins::view::Kind::Button(button) => Some(button.icon.clone()),
            _ => None,
        }
    };
    fake.send(&mut app, window, WindowEvent::Frame);
    run_until(&mut fake, &mut app, |_, app| {
        icon(app).as_deref() == Some("eye-closed")
    });
    // The command's render, of the plugin's thread, asks for the frame that
    // shows it, as no hover nor tab's change brings one.
    fake.send_app(&mut app, AppEvent::Command("flipper.flip".to_owned()));
    run_until(&mut fake, &mut app, |_, app| {
        icon(app).as_deref() == Some("coffee")
    });
    // A tab's change, as of a program that starts in it, of a turn of no
    // frame.
    let shown = app.windows[&window].window;
    let tab = app.workspace.window(shown).unwrap().active_tab();
    app.workspace.set_tab_title(tab, Some("flip".to_owned()));
    fake.send_app(&mut app, AppEvent::Wake);
    run_until(&mut fake, &mut app, |_, app| {
        icon(app).as_deref() == Some("eye-closed")
    });
}

/// The frame of the tab bar's text, keyed `t`, in its view, of `window`.
fn tab_text(app: &App, window: WindowId) -> Frame {
    let view = app.windows[&window]
        .chrome
        .iter()
        .find(|view| matches!(view.view, ViewId::TabBar(_)))
        .expect("the tab bar");
    let surface = &view.surface;
    let node = surface
        .tree
        .by_key(&lokyr_kernel::plugins::view::Key::from("t"))
        .expect("its text");
    surface.layout.frame(&surface.tree, node).expect("a frame")
}

#[test]
fn a_bar_through_the_titlebar_leaves_the_plugins_buttons_their_room() {
    for place in [
        plugins::host::BarPlace::Title,
        plugins::host::BarPlace::Sidebar { width: 200.0 },
    ] {
        let (fake, app, window, _) = app_placed(&["first", "second"], false, Some(place));
        let size = fake.window(window).unwrap().size;
        let scale = size.scale as f32;
        let shown = buttons(&app, window);
        assert_eq!(shown.len(), 2, "{place:?}: {shown:?}");
        // The bar's room of the window's buttons ends after the plugins'
        // two, so that nothing of it lies under them, as the window's keys
        // of a sidebar would.
        let view = app.windows[&window]
            .chrome
            .iter()
            .find(|view| matches!(view.view, ViewId::TabBar(_)))
            .and_then(|view| view.frame)
            .expect("the bar's frame");
        let text = tab_text(&app, window);
        let last = shown[1].1;
        assert!(
            view.x + text.x >= last.x + last.width,
            "{place:?}: the text at {} under a button ending at {}",
            view.x + text.x,
            last.x + last.width
        );
        let after = (size.buttons * size.scale) as f32 + 2.0 * 28.0 * scale;
        assert!(view.x + text.x >= after - 0.5, "{place:?}");
    }
}

/// A plugin of a titlebar button whose click faults it.
struct Breaker;

impl plugins::Plugin for Breaker {
    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        matches!(view.slot()?, Slot::TitleButtons)
            .then(|| Node::icon_button("circle").key("b").tooltip("Breaker"))
    }

    fn on_ui_event(&mut self, _: &mut plugins::Cx<'_>, _: &UiEvent) -> Result<(), Error> {
        Err(Error::new(
            lokyr_kernel::commands::Code::Fault,
            "the button broke",
        ))
    }
}

/// A new [`Breaker`], as the app makes it anew after its fault.
fn make_breaker() -> Box<dyn plugins::Plugin + Send> {
    Box::new(Breaker)
}

#[test]
fn a_faulted_plugin_s_button_goes_and_comes_back_as_it_starts_again() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(quiet())
        .with_registration(plugins::Registration {
            manifest: "[plugin]\nname = \"lokyr/breaker\"\nkind = \"builtin\"\nlicense = \"MIT\"\n[contributes]\nslots = [\"title-buttons\"]\n",
            make: make_breaker,
        });
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    let frames = |fake: &mut Fake, app: &mut App| {
        for _ in 0..4 {
            fake.send(app, window, WindowEvent::Frame);
        }
    };
    frames(&mut fake, &mut app);
    let shown = buttons(&app, window);
    assert_eq!(shown.len(), 1, "{shown:?}");
    let scale = fake.window(window).unwrap().size.scale as f32;
    // Its click faults it: its button goes, as it would show a state that
    // no longer holds and take no click.
    click_in(&mut fake, &mut app, window, shown[0].1, scale);
    frames(&mut fake, &mut app);
    assert!(buttons(&app, window).is_empty());
    // A second later it starts again, and its button is back.
    fake.fire_timers(&mut app, Instant::now() + Duration::from_millis(1100));
    frames(&mut fake, &mut app);
    assert_eq!(buttons(&app, window).len(), 1);
}

#[test]
fn the_platform_keeps_the_room_of_the_buttons_that_show_for_the_system_s_title() {
    // A tab bar at the bottom: the system's titlebar of its title, which
    // leaves the two buttons' room.
    let (fake, _app, window, _) = app_placed(
        &["awake", "snap"],
        false,
        Some(plugins::host::BarPlace::Bottom),
    );
    let shown = fake.window(window).unwrap();
    assert_eq!(shown.titlebar, Titlebar::Titled);
    assert_eq!(shown.title_buttons_room, 56.0);
    // Of more than three, the room of the three that show, the app's
    // button of the rest among them.
    let (fake, _app, window, _) = app_placed(
        &["a", "b", "c", "d"],
        false,
        Some(plugins::host::BarPlace::Bottom),
    );
    assert_eq!(fake.window(window).unwrap().title_buttons_room, 84.0);
}

#[test]
fn the_plugins_read_the_window_s_title_as_the_system_s_titlebar_shows_it() {
    // A single tab's title across the titlebar, which its plugin draws of
    // the window's title, the grid's size too.
    let (_fake, app, window, _) = app_titled(&["awake"], false, true);
    let shown = app.windows[&window].window;
    let title = app.workspace.window(shown).unwrap().title().to_owned();
    assert_eq!(title, app.title_of(window));
    assert!(title.contains('×'), "{title}");
}
