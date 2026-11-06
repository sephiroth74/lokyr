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
    /// The place that `bars.place` gives its tab bar.
    place: Rc<Cell<plugins::host::BarPlace>>,
    /// The width that the last drag of its column's edge left it.
    resized: Rc<Cell<Option<f32>>>,
}

impl plugins::Plugin for Bars {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["bars.again", "bars.panel", "bars.place"] {
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
        if name == "bars.place" {
            cx.set_tab_bar_place(self.place.get())?;
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
                plugins::Event::TabBarResized { width } => self.resized.set(Some(*width)),
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
        place: Rc::new(Cell::new(plugins::host::BarPlace::default())),
        resized: Rc::new(Cell::new(None)),
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
        place: Rc::new(Cell::new(plugins::host::BarPlace::default())),
        resized: Rc::new(Cell::new(None)),
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

#[test]
fn the_tab_bar_goes_at_its_plugin_s_place_and_the_titlebar_follows() {
    use plugins::host::BarPlace;
    let place = Rc::new(Cell::new(BarPlace::Bottom));
    let bars = Bars {
        empty: Rc::new(RefCell::new(Vec::new())),
        panics: Rc::new(Cell::new(false)),
        views: Vec::new(),
        closed: Rc::new(Cell::new(0)),
        place: Rc::clone(&place),
        resized: Rc::new(Cell::new(None)),
    };
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/bars\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"tab-bar\", \"status-bar\"]\n",
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
    let placed = |fake: &mut Fake, app: &mut App, at: BarPlace| {
        place.set(at);
        app.run(fake, "bars.place", Args::new());
        app.run(fake, "bars.again", Args::new());
        for _ in 0..3 {
            fake.send(app, window, WindowEvent::Frame);
        }
        let state = &app.windows[&window];
        let framed = |slot: Slot| {
            state
                .chrome
                .iter()
                .find(|view| view.view.slot() == Some(slot))
                .and_then(|view| view.frame)
                .map(super::super::chrome::rect_of)
                .expect("a view's frame")
        };
        (
            fake.window(window).unwrap().titlebar,
            framed(Slot::TabBar),
            framed(Slot::StatusBar),
            state.views[0].rect,
        )
    };
    let size = fake.window(window).unwrap().size;
    let (width, height) = (size.pixels.width as i32, size.pixels.height as i32);
    let pixels = |points: f32| (f64::from(points) * size.scale).round() as i32;
    let titlebar = size.bar_pixels() as i32;
    // At the bottom, below the status bar, under a titlebar of the title.
    let (bar, tab_bar, status, pane) = placed(&mut fake, &mut app, BarPlace::Bottom);
    assert_eq!(bar, Titlebar::Titled);
    assert_eq!(tab_bar.y + tab_bar.height as i32, height);
    assert_eq!(status.y + status.height as i32, tab_bar.y);
    assert_eq!((pane.y, pane.y + pane.height as i32), (titlebar, status.y));
    // Down a column on the left, from below the titlebar, the rest beside.
    let (bar, tab_bar, status, pane) = placed(&mut fake, &mut app, BarPlace::Left { width: 100.0 });
    assert_eq!(bar, Titlebar::Titled);
    assert_eq!(
        (tab_bar.x, tab_bar.y, tab_bar.width as i32),
        (0, titlebar, pixels(100.0))
    );
    assert_eq!(tab_bar.y + tab_bar.height as i32, height);
    assert_eq!((status.x, pane.x), (pixels(100.0), pixels(100.0)));
    assert_eq!(pane.x + pane.width as i32, width);
    // And on the right.
    let (_, tab_bar, status, pane) = placed(&mut fake, &mut app, BarPlace::Right { width: 100.0 });
    assert_eq!(tab_bar.x + tab_bar.width as i32, width);
    assert_eq!(pane.x + pane.width as i32, tab_bar.x);
    assert_eq!(status.x + status.width as i32, tab_bar.x);
    // Below a titlebar of the title, across the window.
    let (bar, tab_bar, _, pane) = placed(&mut fake, &mut app, BarPlace::BelowTitlebar);
    assert_eq!(bar, Titlebar::Titled);
    assert_eq!(
        (tab_bar.y, pane.y),
        (titlebar, titlebar + tab_bar.height as i32)
    );
    // In the titlebar again, of a titlebar of tabs.
    let (bar, tab_bar, ..) = placed(&mut fake, &mut app, BarPlace::Titlebar);
    assert_eq!(bar, Titlebar::Tabs);
    assert_eq!(tab_bar.y, 0);
    // A sidebar, through the titlebar, of a titlebar of tabs, the panes
    // beside it below the titlebar, which is the window's, whose drag moves
    // it, but the bar's own parts.
    let (bar, tab_bar, status, pane) =
        placed(&mut fake, &mut app, BarPlace::Sidebar { width: 100.0 });
    assert_eq!(bar, Titlebar::Tabs);
    assert_eq!(
        (
            tab_bar.x,
            tab_bar.y,
            tab_bar.width as i32,
            tab_bar.height as i32
        ),
        (0, 0, pixels(100.0), height)
    );
    let below = fake.window(window).unwrap().size.bar_pixels() as i32;
    assert_eq!((pane.x, pane.y), (pixels(100.0), below));
    assert_eq!(status.x, pixels(100.0));
    let areas = &fake.window(window).unwrap().titlebar_areas;
    assert!(areas.iter().all(|area| area.origin.x < 100.0), "{areas:?}");
    // A column too wide is refused, the place staying.
    place.set(BarPlace::Left { width: 2000.0 });
    app.run(&mut fake, "bars.place", Args::new());
    assert_eq!(
        app.plugins.tab_bar_place("lokyr/bars"),
        BarPlace::Sidebar { width: 100.0 }
    );
}

#[test]
fn a_drag_of_a_column_s_edge_resizes_it_and_its_plugin_learns_of_it() {
    use lokyr_platform_core::PointerShape;
    use plugins::host::BarPlace;
    let resized = Rc::new(Cell::new(None));
    let place = Rc::new(Cell::new(BarPlace::Left { width: 100.0 }));
    let bars = Bars {
        empty: Rc::new(RefCell::new(Vec::new())),
        panics: Rc::new(Cell::new(false)),
        views: Vec::new(),
        closed: Rc::new(Cell::new(0)),
        place: Rc::clone(&place),
        resized: Rc::clone(&resized),
    };
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/bars\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"tab-bar\"]\n",
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
    let frames = |fake: &mut Fake, app: &mut App| {
        for _ in 0..3 {
            fake.send(app, window, WindowEvent::Frame);
        }
    };
    app.run(&mut fake, "bars.place", Args::new());
    app.run(&mut fake, "bars.again", Args::new());
    frames(&mut fake, &mut app);
    let width = |app: &App| app.plugins.tab_bar_place("lokyr/bars").width();
    // The resize's pointer over the column's edge.
    let edge = fake
        .window(window)
        .unwrap()
        .pointer_areas
        .iter()
        .find(|area| area.shape == PointerShape::ResizeColumn)
        .copied()
        .expect("the edge's pointer");
    assert!((edge.origin.x - 97.0).abs() < 0.5, "{edge:?}");
    assert!((edge.size.width - 6.0).abs() < 0.5, "{edge:?}");
    // A drag of 50 points to the right, then its release.
    let at = |x| Point { x, y: 100.0 };
    let none = Modifiers::NONE;
    fake.send(&mut app, window, click(true, at(100.0), none, 1));
    fake.send(&mut app, window, moved(at(150.0), none));
    frames(&mut fake, &mut app);
    assert_eq!(width(&app), Some(150.0));
    let size = fake.window(window).unwrap().size;
    let column = app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::TabBar))
        .and_then(|view| view.frame)
        .unwrap();
    assert!((f64::from(column.width) - 150.0 * size.scale).abs() < 1.0);
    assert_eq!(resized.get(), None);
    fake.send(&mut app, window, click(false, at(150.0), none, 1));
    run_until(&mut fake, &mut app, |_, _| resized.get().is_some());
    assert_eq!(resized.get(), Some(150.0));
    // A drag past the widest stops there; on the right, the edge is the
    // column's left.
    fake.send(&mut app, window, click(true, at(150.0), none, 1));
    fake.send(&mut app, window, moved(at(2000.0), none));
    fake.send(&mut app, window, click(false, at(2000.0), none, 1));
    assert_eq!(width(&app), Some(600.0));
    place.set(BarPlace::Right { width: 100.0 });
    app.run(&mut fake, "bars.place", Args::new());
    frames(&mut fake, &mut app);
    let left = size.points.width - 100.0;
    fake.send(&mut app, window, click(true, at(left), none, 1));
    fake.send(&mut app, window, moved(at(left - 40.0), none));
    fake.send(&mut app, window, click(false, at(left - 40.0), none, 1));
    assert_eq!(width(&app), Some(140.0));
    // A press elsewhere drags nothing.
    fake.send(&mut app, window, click(true, at(left - 200.0), none, 1));
    fake.send(&mut app, window, moved(at(left - 260.0), none));
    fake.send(&mut app, window, click(false, at(left - 260.0), none, 1));
    assert_eq!(width(&app), Some(140.0));
}

/// A plugin of a status bar of one box of a hover of its own, which counts
/// its renders.
struct Hovering(Rc<Cell<usize>>);

impl plugins::Plugin for Hovering {
    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        if view.slot() != Some(Slot::StatusBar) {
            return None;
        }
        self.0.set(self.0.get() + 1);
        let mut node = Node::container(vec![Node::text("hover me")])
            .key("box")
            .style(Style {
                background: Some(Token::from("tab.bar.bg")),
                hover: Some(lokyr_kernel::plugins::view::Hover {
                    background: Some(Token::from("tab.hover.bg")),
                    ..Default::default()
                }),
                transition: Some(lokyr_kernel::plugins::view::Transition {
                    duration: 0.1,
                    easing: lokyr_kernel::plugins::view::Easing::Linear,
                }),
                ..Style::default()
            });
        node.layout.height = Length::Points(STATUS);
        node.layout.grow = 1.0;
        Some(node)
    }
}

#[test]
fn a_hover_fades_with_no_render_of_its_plugin() {
    let renders = Rc::new(Cell::new(0));
    let manifest = concat!(
        "[plugin]\nname = \"lokyr/hovering\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
        "[contributes]\nslots = [\"status-bar\"]\n",
    );
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(idle())
        .with_builtin(manifest, Box::new(Hovering(Rc::clone(&renders))));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    for _ in 0..2 {
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    // The chrome's tokens of the window's look, once its pane takes the
    // theme's colors.
    let theme = app.colors(fake.appearance()).background;
    let theme = lokyr_render_core::Color::from_rgb8(theme.r, theme.g, theme.b);
    let settled = |app: &App| {
        app.windows[&window]
            .chrome_tokens
            .as_ref()
            .is_some_and(|((_, background, _), _)| *background == theme)
    };
    let deadline = Instant::now() + std::time::Duration::from_secs(10);
    while !settled(&app) {
        assert!(Instant::now() < deadline, "the window's look settles");
        fake.deliver_wake(&mut app);
        fake.send(&mut app, window, WindowEvent::Frame);
    }
    let rendered = renders.get();
    assert!(rendered > 0);
    let state = &app.windows[&window];
    let bar = state
        .chrome
        .iter()
        .find(|view| view.view.slot() == Some(Slot::StatusBar))
        .and_then(|view| view.frame)
        .map(super::super::chrome::rect_of)
        .unwrap();
    let scale = fake.window(window).unwrap().size.scale;
    let inside = Point {
        x: f64::from(bar.x) / scale + 20.0,
        y: (f64::from(bar.y) + f64::from(bar.height) / 2.0) / scale,
    };
    // The tokens of the window's chrome, of its own appearance.
    let tokens = &state.chrome_tokens.as_ref().expect("the chrome's tokens").1;
    let (inactive, veil) = (
        tokens.get("tab.bar.bg").unwrap(),
        tokens.get("tab.hover.bg").unwrap(),
    );
    // The hover's veil over the box's own background, opaque.
    let hovered = lokyr_render_core::Color {
        red: veil.red * veil.alpha + inactive.red * (1.0 - veil.alpha),
        green: veil.green * veil.alpha + inactive.green * (1.0 - veil.alpha),
        blue: veil.blue * veil.alpha + inactive.blue * (1.0 - veil.alpha),
        alpha: 1.0,
    };
    let fills = |app: &App| -> Vec<lokyr_render_core::Color> {
        app.windows[&window]
            .shown
            .as_ref()
            .map(|shown| {
                shown
                    .items
                    .iter()
                    .flat_map(|item| match item {
                        Item::Shapes(shapes) => shapes
                            .iter()
                            .filter(|shape| shape.rect == bar)
                            .map(|shape| shape.fill)
                            .collect(),
                        _ => Vec::new(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    assert_eq!(fills(&app), [inactive]);
    // Over the box, the frames fade its background in, of no render.
    fake.send(&mut app, window, moved(inside, Modifiers::NONE));
    assert!(fake.window(window).unwrap().frame_requested);
    let near = |fills: &[lokyr_render_core::Color]| {
        let [fill] = fills else {
            return false;
        };
        [
            (fill.red, hovered.red),
            (fill.green, hovered.green),
            (fill.blue, hovered.blue),
            (fill.alpha, hovered.alpha),
        ]
        .iter()
        .all(|(a, b)| (a - b).abs() < 1e-4)
    };
    run_until(&mut fake, &mut app, |_, app| near(&fills(app)));
    assert_eq!(renders.get(), rendered);
}

#[test]
fn the_chrome_takes_the_window_s_own_appearance_and_the_terminal_s_colors() {
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    // A program that makes its background light, in the dark appearance.
    let program = shell("printf '\\033]11;#fdf6e3\\007'; echo ready; exec sleep 30");
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(program);
    fake.start(&mut app);
    fake.send_app(&mut app, AppEvent::AppearanceChanged(Appearance::Dark));
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    fake.send(&mut app, window, WindowEvent::Frame);
    let state = &app.windows[&window];
    let Some(((dark, background, _), tokens)) = state.chrome_tokens.as_ref() else {
        panic!("the chrome's tokens");
    };
    // The light variant's tokens, as the window's own appearance, and
    // the terminal's background the program's.
    assert!(!dark);
    let light = app.theme.variant(false);
    assert_eq!(
        tokens.get("tab.inactive.fg"),
        light.tokens.get("tab.inactive.fg")
    );
    let program = lokyr_render_core::Color::from_rgb8(0xfd, 0xf6, 0xe3);
    assert_eq!(*background, program);
    assert_eq!(tokens.get("terminal.bg"), Some(program));
    assert_eq!(
        fake.window(window).unwrap().appearance,
        Some(Appearance::Light)
    );
}
