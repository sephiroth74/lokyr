//! The tests of the slots' views of the plugins in the windows.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{Slot, ViewId};
use lokyr_kernel::plugins::view::{Length, Node, Style, Token};
use lokyr_platform_core::Titlebar;
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// The heights of the views of [`Bars`], in points: the pane's header's,
/// the strip's and the status bar's; the tab bar takes the titlebar's.
const HEADER: f32 = 20.0;
const STRIP: f32 = 4.0;
const STATUS: f32 = 22.0;

/// A plugin of every slot: each view a box of its slot's height, of its
/// view's name, or none for the slots of `empty`; it panics at its renders
/// once `panics` holds, `bars.again` asks for a render of each view, and
/// `bars.panel` emits `bars.open`, at whose delivery it opens a panel and
/// closes it, whose closings it counts in `closed`.
struct Bars {
    empty: Rc<RefCell<Vec<Slot>>>,
    panics: Rc<Cell<bool>>,
    views: Vec<ViewId>,
    closed: Rc<Cell<usize>>,
}

impl plugins::Plugin for Bars {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["bars.again", "bars.panel"] {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        cx.subscribe(&["bars.open"])
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        if name == "bars.panel" {
            return cx.emit("bars.open", "bars.open", plugins::Data::Nil);
        }
        for view in &self.views {
            cx.request_render(view.clone())?;
        }
        Ok(())
    }

    fn on_event(
        &mut self,
        cx: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for event in events {
            match event {
                plugins::Event::PanelClosed(_) => self.closed.set(self.closed.get() + 1),
                plugins::Event::Custom { .. } => {
                    let window = cx.focused_window().expect("a window");
                    cx.open_panel(
                        "menu",
                        lokyr_kernel::plugins::ui::PanelOptions {
                            anchor: lokyr_kernel::plugins::ui::Anchor::Pointer(window),
                            role: lokyr_kernel::plugins::ui::Role::Menu,
                            close_outside: true,
                            close_escape: true,
                        },
                    )?;
                    cx.close_panel("menu")?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        assert!(!self.panics.get(), "a render's panic");
        if !self.views.contains(view) {
            self.views.push(view.clone());
        }
        let slot = view.slot()?;
        if self.empty.borrow().contains(&slot) {
            return None;
        }
        let height = match slot {
            Slot::PaneHeader => HEADER,
            Slot::WindowStrip => STRIP,
            Slot::StatusBar => STATUS,
            Slot::TabBar => 10.0,
        };
        let mut node = Node::container(vec![Node::text(slot.name())]);
        node.layout.height = Length::Points(height);
        node.layout.grow = 1.0;
        node.style = Style {
            background: Some(Token::from("tab.bar.bg")),
            ..Style::default()
        };
        Some(node)
    }
}

/// The rectangles of the shapes of `items`.
fn shapes(items: &[Item]) -> Vec<Rect> {
    items
        .iter()
        .flat_map(|item| match item {
            Item::Shapes(shapes) => shapes.iter().map(|shape| shape.rect).collect(),
            Item::Rect { rect, .. } => vec![*rect],
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn a_plugin_s_views_fill_their_slots_which_the_panes_give_their_room() {
    let (empty, panics) = (Rc::new(RefCell::new(Vec::new())), Rc::new(Cell::new(false)));
    let bars = Bars {
        empty: Rc::clone(&empty),
        panics: Rc::clone(&panics),
        views: Vec::new(),
        closed: Rc::new(Cell::new(0)),
    };
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/bars\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"tab-bar\", \"window-strip\", \"status-bar\", \"pane-header\"]\n",
    );
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(idle())
        .with_builtin(manifest, Box::new(bars));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    let frame = |fake: &mut Fake, app: &mut App| {
        fake.send(app, window, WindowEvent::Frame);
        fake.send(app, window, WindowEvent::Frame);
    };
    frame(&mut fake, &mut app);
    let size = fake.window(window).unwrap().size;
    let (scale, (width, height)) = (size.scale, (size.pixels.width, size.pixels.height));
    let pixels = |points: f32| (f64::from(points) * scale).round() as i32;
    let titlebar = size.bar_pixels() as i32;
    let state = &app.windows[&window];
    let framed = |slot: Slot| {
        state
            .chrome
            .iter()
            .find(|view| view.view.slot() == Some(slot))
            .and_then(|view| view.frame)
            .map(super::super::chrome::rect_of)
    };
    // The tab bar in the titlebar, which takes a tab bar's style, past the
    // window's buttons; the strip over the panes' top; the status bar at
    // the window's bottom; the pane's header above its grid.
    assert_eq!(fake.window(window).unwrap().titlebar, Titlebar::Tabs);
    let tab_bar = framed(Slot::TabBar).expect("the tab bar");
    assert_eq!((tab_bar.y, tab_bar.height), (0, titlebar as u32));
    assert_eq!(tab_bar.x + tab_bar.width as i32, width as i32);
    let strip = framed(Slot::WindowStrip).expect("the strip");
    assert_eq!((strip.y, strip.height), (titlebar, pixels(STRIP) as u32));
    let status = framed(Slot::StatusBar).expect("the status bar");
    assert_eq!(status.y + status.height as i32, height as i32);
    assert_eq!(status.height, pixels(STATUS) as u32);
    let header = framed(Slot::PaneHeader).expect("the pane's header");
    assert_eq!((header.y, header.height), (titlebar, pixels(HEADER) as u32));
    // The pane's grid below its header, above the status bar.
    let pane = state.views[0].rect;
    assert_eq!(pane.y, titlebar + pixels(HEADER));
    assert_eq!(pane.y + pane.height as i32, status.y);
    // Each view drawn in its region, over the panes.
    let drawn = shapes(&state.shown.as_ref().expect("a frame").items);
    for region in [tab_bar, strip, status, header] {
        assert!(drawn.contains(&region), "{region:?} in {drawn:?}");
    }
    // A slot without a view takes no room, which the panes take back.
    empty.borrow_mut().push(Slot::StatusBar);
    app.run(&mut fake, "bars.again", Args::new());
    frame(&mut fake, &mut app);
    let state = &app.windows[&window];
    assert!(
        state
            .chrome
            .iter()
            .filter(|view| view.view.slot() == Some(Slot::StatusBar))
            .all(|view| view.frame.is_none())
    );
    let pane = state.views[0].rect;
    assert_eq!(pane.y + pane.height as i32, height as i32);
    assert!(app.diagnostics.shown().is_none());
    // A plugin that stops keeps its views as they were, and a notice tells
    // it.
    panics.set(true);
    empty.borrow_mut().clear();
    app.run(&mut fake, "bars.again", Args::new());
    frame(&mut fake, &mut app);
    let state = &app.windows[&window];
    assert!(
        state
            .chrome
            .iter()
            .any(|view| view.view.slot() == Some(Slot::TabBar) && view.frame.is_some())
    );
    assert_eq!(fake.window(window).unwrap().titlebar, Titlebar::Tabs);
    let shown = app.diagnostics.shown().expect("a notice");
    assert!(
        shown.text.contains("lokyr/bars") && shown.text.contains("panicked"),
        "{}",
        shown.text
    );
}

#[test]
fn a_plugin_s_render_alone_draws_and_a_panel_s_closing_comes_in_its_turn() {
    let closed = Rc::new(Cell::new(0));
    let bars = Bars {
        empty: Rc::new(RefCell::new(Vec::new())),
        panics: Rc::new(Cell::new(false)),
        views: Vec::new(),
        closed: Rc::clone(&closed),
    };
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/bars\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"status-bar\"]\n",
    );
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(idle())
        .with_builtin(manifest, Box::new(bars));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    for _ in 0..3 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    assert!(!fake.window(window).unwrap().frame_requested);
    // A command whose plugin renders its view again, and asks nothing else:
    // its window draws it.
    fake.send_app(&mut app, AppEvent::Command("bars.again".to_owned()));
    assert!(fake.window(window).unwrap().frame_requested);
    // A panel that a delivery's call opens and closes: its plugin hears so
    // in a turn of its own, which no event of the platform's need bring.
    fake.send_app(&mut app, AppEvent::Command("bars.panel".to_owned()));
    assert_eq!(closed.get(), 0);
    assert!(app.deliver_timer.is_some());
    fake.fire_timers(&mut app, Instant::now());
    assert_eq!(closed.get(), 1);
    assert!(app.deliver_timer.is_none());
}
