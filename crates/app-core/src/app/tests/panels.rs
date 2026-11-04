//! The tests of the plugins' panels in the windows: their anchors, their
//! roles' focus and pointer, what closes them, and the menus that the
//! platform shows as its own.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{Anchor, Corner, PanelId, PanelOptions, Role, UiEvent, ViewId};
use lokyr_kernel::plugins::view::{Layout as NodeLayout, Length, Node, UiEventKind};
use lokyr_platform_core::Ime;
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// What the plugin of [`Paneler`] heard: its nodes' events, and its panels'
/// closings, by their ids.
#[derive(Default)]
struct Heard {
    events: Vec<UiEvent>,
    closed: Vec<String>,
}

/// A plugin of a tab bar of a button `open`, whose commands open its
/// panels: `paneler.dialog`, a dialog below the button, of a field `find`,
/// which Escape closes; `paneler.menu`, a menu at the pointer, of a field
/// and a button, which a click outside or Escape closes; `paneler.native`,
/// a menu of two buttons alone, which the platform shows as its own;
/// `paneler.tip`, a tooltip in the window's bottom right corner;
/// `paneler.cell`, `paneler.cursor` and `paneler.corner`, dialogs below the
/// pane's cell of its first row's third column, below its cursor, and in
/// its top right corner; `paneler.close` closes the dialog.
struct Paneler {
    heard: Rc<RefCell<Heard>>,
}

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/paneler\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"tab-bar\"]\n",
);

const COMMANDS: [&str; 8] = [
    "paneler.dialog",
    "paneler.menu",
    "paneler.native",
    "paneler.tip",
    "paneler.cell",
    "paneler.cursor",
    "paneler.corner",
    "paneler.close",
];

fn sized(node: Node, width: f32, height: f32) -> Node {
    node.layout(NodeLayout {
        width: Length::Points(width),
        height: Length::Points(height),
        ..NodeLayout::default()
    })
}

fn options(
    anchor: Anchor,
    role: Role,
    (close_outside, close_escape): (bool, bool),
) -> PanelOptions {
    PanelOptions {
        anchor,
        role,
        close_outside,
        close_escape,
    }
}

impl plugins::Plugin for Paneler {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in COMMANDS {
            cx.register_command(Command {
                name: name.to_owned(),
                title: name.to_owned(),
                category: Category::View,
                capabilities: Vec::new(),
                params: Vec::new(),
                when: None,
            })?;
        }
        Ok(())
    }

    fn on_command(&mut self, cx: &mut plugins::Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        let window = cx.focused_window().unwrap();
        let pane = cx.focused_pane().unwrap();
        let bar = ViewId::TabBar(window);
        let (id, options) = match name {
            "paneler.dialog" => (
                "dialog",
                options(
                    Anchor::Node {
                        view: bar,
                        key: "open".into(),
                    },
                    Role::Dialog,
                    (false, true),
                ),
            ),
            "paneler.menu" => (
                "menu",
                options(Anchor::Pointer(window), Role::Menu, (true, true)),
            ),
            "paneler.native" => (
                "native",
                options(Anchor::Pointer(window), Role::Menu, (true, true)),
            ),
            "paneler.tip" => (
                "tip",
                options(
                    Anchor::WindowCorner {
                        window,
                        corner: Corner::BottomRight,
                    },
                    Role::Tooltip,
                    (false, false),
                ),
            ),
            "paneler.cell" => (
                "dialog",
                options(
                    Anchor::Cell {
                        pane,
                        column: 2,
                        row: 0,
                    },
                    Role::Dialog,
                    (false, true),
                ),
            ),
            "paneler.cursor" => (
                "dialog",
                options(Anchor::Cursor(pane), Role::Dialog, (false, true)),
            ),
            "paneler.corner" => (
                "dialog",
                options(
                    Anchor::PaneCorner {
                        pane,
                        corner: Corner::TopRight,
                    },
                    Role::Dialog,
                    (false, true),
                ),
            ),
            _ => return cx.close_panel("dialog"),
        };
        cx.open_panel(id, options)
    }

    fn on_event(
        &mut self,
        _: &mut plugins::Cx<'_>,
        events: &[plugins::Event],
    ) -> Result<(), Error> {
        for event in events {
            if let plugins::Event::PanelClosed(id) = event {
                self.heard.borrow_mut().closed.push(id.0.clone());
            }
        }
        Ok(())
    }

    fn on_ui_event(&mut self, _: &mut plugins::Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.heard.borrow_mut().events.push(event.clone());
        Ok(())
    }

    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        let ViewId::Panel(PanelId(id)) = view else {
            let mut bar =
                Node::container(vec![sized(Node::button("Open"), 60.0, 20.0).key("open")]);
            bar.layout.grow = 1.0;
            return Some(bar);
        };
        Some(match id.as_str() {
            "native" => Node::container(vec![
                Node::button("First").key("first"),
                Node::button("Second").key("second"),
            ]),
            "menu" => Node::container(vec![
                sized(Node::field("Filter"), 100.0, 20.0).key("filter"),
                Node::button("Item").key("item"),
            ]),
            "tip" => sized(Node::text("a tip"), 80.0, 20.0),
            _ => Node::container(vec![sized(Node::field("Find"), 120.0, 20.0).key("find")]),
        })
    }
}

/// The app without padding, of a titlebar of 28 points, its window
/// focused, the plugin of [`Paneler`] filling its tab bar, its pane running
/// `program` until it shows `ready`.
fn paneler(program: Program) -> (Fake, App, WindowId, Rc<RefCell<Heard>>) {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let plugin = Paneler {
        heard: Rc::clone(&heard),
    };
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let mut app = App::new(Some(drawing), unpadded(), None)
        .with_program(program)
        .with_builtin(MANIFEST, Box::new(plugin));
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    frames(&mut fake, &mut app, window);
    (fake, app, window, heard)
}

/// The frames that `window` asks for, until it asks for none.
fn frames(fake: &mut Fake, app: &mut App, window: WindowId) {
    for _ in 0..4 {
        fake.send(app, window, WindowEvent::Frame);
    }
}

/// Runs `command`, as its menu would, and draws.
fn command(fake: &mut Fake, app: &mut App, window: WindowId, command: &str) {
    fake.send_app(app, AppEvent::Command(command.to_owned()));
    frames(fake, app, window);
}

/// The frame, in pixels, of the panel `id` of `window`, if it shows.
fn panel_frame(app: &App, window: WindowId, id: &str) -> Option<lokyr_ui_core::layout::Frame> {
    app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view == ViewId::Panel(PanelId::from(id)))
        .and_then(|view| view.frame)
}

/// The frame, in pixels, of the node of `key` of a slot's view or panel of
/// `window`, in the window.
fn node_frame(app: &App, window: WindowId, key: &str) -> lokyr_ui_core::layout::Frame {
    for view in &app.windows[&window].chrome {
        let (Some(frame), Some(node)) = (view.frame, view.surface.tree.by_key(&key.into())) else {
            continue;
        };
        let at = view.surface.layout.frame(&view.surface.tree, node).unwrap();
        return lokyr_ui_core::layout::Frame {
            x: at.x + frame.x,
            y: at.y + frame.y,
            ..at
        };
    }
    panic!("no node `{key}`");
}

/// The middle of `frame`, of pixels, in points of a display of `scale`.
fn middle(frame: lokyr_ui_core::layout::Frame, scale: f64) -> Point {
    Point {
        x: f64::from(frame.x + frame.width / 2.0) / scale,
        y: f64::from(frame.y + frame.height / 2.0) / scale,
    }
}

/// Whether a node of `window`'s panel `id` has the keyboard's focus.
fn focused_in(app: &App, window: WindowId, id: &str) -> bool {
    let state = &app.windows[&window];
    match state.focus.target() {
        lokyr_ui_core::focus::Target::Node { panel, .. } => state
            .chrome
            .iter()
            .any(|view| view.id == panel && view.view == ViewId::Panel(PanelId::from(id))),
        lokyr_ui_core::focus::Target::Pane => false,
    }
}

/// A point over the pane, below the titlebar's tab bar.
const OVER_PANE: Point = Point { x: 9.0, y: 60.0 };

#[test]
fn a_dialog_below_a_node_takes_the_keys_and_escape_closes_it_back_to_the_pane() {
    let (mut fake, mut app, window, heard) = paneler(reader(""));
    command(&mut fake, &mut app, window, "paneler.dialog");
    // Below the bar's button, of its tree's size, its field focused.
    let open = node_frame(&app, window, "open");
    let frame = panel_frame(&app, window, "dialog").expect("the dialog");
    assert_eq!((frame.x, frame.y), (open.x, open.y + open.height));
    assert!(focused_in(&app, window, "dialog"));
    // Its field takes the keys, the program none.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyA, "a", Modifiers::NONE, true),
    );
    let typed = heard.borrow().events.iter().any(|event| {
        event.key.0 == "find"
            && matches!(&event.kind, UiEventKind::InputChanged { text, .. } if text == "a")
    });
    assert!(typed);
    // Escape closes it, as its plugin hears, and gives the pane the keys.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", Modifiers::NONE, true),
    );
    frames(&mut fake, &mut app, window);
    assert!(panel_frame(&app, window, "dialog").is_none());
    assert!(app.windows[&window].pane_has_focus());
    assert_eq!(heard.borrow().closed, ["dialog"]);
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyZ, "z", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "z"));
    assert!(!shows(&app, window, "az"), "{:?}", screen(&app, window));
}

#[test]
fn a_menu_at_the_pointer_closes_as_a_click_outside_takes_the_press() {
    // A program of the mouse's clicks.
    let (mut fake, mut app, window, heard) = paneler(reader("\\033[?1000h"));
    let scale = fake.window(window).unwrap().size.scale;
    fake.set_pointer(window, Point { x: 40.0, y: 80.0 });
    command(&mut fake, &mut app, window, "paneler.menu");
    let frame = panel_frame(&app, window, "menu").expect("the menu");
    let pixels = |points: f64| (points * scale) as f32;
    assert_eq!((frame.x, frame.y), (pixels(40.0), pixels(80.0)));
    assert!(focused_in(&app, window, "menu"));
    // A click on its button, its own.
    let item = middle(node_frame(&app, window, "item"), scale);
    fake.send(&mut app, window, click(true, item, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, item, Modifiers::NONE, 1));
    assert!(
        heard
            .borrow()
            .events
            .iter()
            .any(|event| event.key.0 == "item" && event.kind == UiEventKind::Click)
    );
    // A click outside, over the pane, closes it, and goes no further.
    fake.send(&mut app, window, click(true, OVER_PANE, Modifiers::NONE, 1));
    fake.send(
        &mut app,
        window,
        click(false, OVER_PANE, Modifiers::NONE, 1),
    );
    frames(&mut fake, &mut app, window);
    assert!(panel_frame(&app, window, "menu").is_none());
    assert_eq!(heard.borrow().closed, ["menu"]);
    assert!(app.windows[&window].pane_has_focus());
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::KeyX, "x", Modifiers::NONE, true),
    );
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "x"));
    assert!(!shows(&app, window, "^["), "{:?}", screen(&app, window));
}

#[test]
fn a_menu_of_menu_items_alone_is_the_platform_s_own_whose_choice_and_closing_its_plugin_hears() {
    let (mut fake, mut app, window, heard) = paneler(reader(""));
    command(&mut fake, &mut app, window, "paneler.native");
    // The platform shows it, of its two items, and the window draws none.
    let (shown, _, menu) = fake.context_menus().last().cloned().expect("a menu");
    assert_eq!(shown, window);
    let commands: Vec<&str> = menu
        .items
        .iter()
        .filter_map(|item| match item {
            lokyr_platform_core::MenuItem::Command { command, .. } => Some(command.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(commands, ["first", "second"]);
    assert!(panel_frame(&app, window, "native").is_none());
    // Its choice, then its closing.
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand("second".to_owned()),
    );
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    frames(&mut fake, &mut app, window);
    assert!(
        heard
            .borrow()
            .events
            .iter()
            .any(|event| event.key.0 == "second" && event.kind == UiEventKind::Click)
    );
    assert_eq!(heard.borrow().closed, ["native"]);
    // Closed without a choice, its plugin hears it as well.
    command(&mut fake, &mut app, window, "paneler.native");
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    frames(&mut fake, &mut app, window);
    assert_eq!(heard.borrow().closed, ["native", "native"]);
    assert!(app.windows[&window].native.is_none());
}

#[test]
fn a_tooltip_in_the_window_s_corner_takes_neither_the_keys_nor_the_pointer() {
    let (mut fake, mut app, window, _) = paneler(reader(""));
    command(&mut fake, &mut app, window, "paneler.tip");
    let state = &app.windows[&window];
    let frame = panel_frame(&app, window, "tip").expect("the tooltip");
    let scale = fake.window(window).unwrap().size.scale as f32;
    let room = state.overlay;
    let margin = 12.0 * scale;
    assert_eq!(frame.x + frame.width, room.x + room.width - margin);
    assert_eq!(frame.y + frame.height, room.y + room.height - margin);
    assert!(state.pane_has_focus());
    // No pointer's area of its own.
    let shown = fake.window(window).unwrap();
    let over = middle(frame, f64::from(scale));
    assert!(!shown.pointer_areas.iter().any(|area| {
        lokyr_platform_core::ContentRect {
            origin: area.origin,
            size: area.size,
        }
        .contains(over)
    }));
}

#[test]
fn dialogs_below_a_cell_below_the_cursor_and_in_the_pane_s_corner() {
    let (mut fake, mut app, window, _) = paneler(reader(""));
    let state = &app.windows[&window];
    let view = state.views[0];
    let (width, height) = (
        f32::from(view.grid.cell_width),
        f32::from(view.grid.cell_height),
    );
    let (left, top) = (view.rect.x as f32, view.rect.y as f32);
    // Below the first row's third cell.
    command(&mut fake, &mut app, window, "paneler.cell");
    let frame = panel_frame(&app, window, "dialog").expect("the cell's dialog");
    assert_eq!((frame.x, frame.y), (left + 2.0 * width, top + height));
    command(&mut fake, &mut app, window, "paneler.close");
    // Below the cursor's cell.
    command(&mut fake, &mut app, window, "paneler.cursor");
    let cursor = app.pane(window).snapshot().cursor().position;
    let frame = panel_frame(&app, window, "dialog").expect("the cursor's dialog");
    assert_eq!(
        (frame.x, frame.y),
        (
            left + f32::from(cursor.x) * width,
            top + f32::from(cursor.y + 1) * height
        )
    );
    command(&mut fake, &mut app, window, "paneler.close");
    // In the pane's top right corner.
    command(&mut fake, &mut app, window, "paneler.corner");
    let scale = fake.window(window).unwrap().size.scale as f32;
    let frame = panel_frame(&app, window, "dialog").expect("the corner's dialog");
    let rect = app.windows[&window].views[0].rect;
    assert_eq!(
        (frame.x + frame.width, frame.y),
        (
            rect.x as f32 + rect.width as f32 - 12.0 * scale,
            rect.y as f32 + 12.0 * scale
        )
    );
}

#[test]
fn a_dialog_that_its_plugin_closes_ends_its_field_s_composition_and_gives_the_keys_back() {
    let (mut fake, mut app, window, heard) = paneler(reader(""));
    command(&mut fake, &mut app, window, "paneler.dialog");
    assert!(focused_in(&app, window, "dialog"));
    let composing = WindowEvent::Ime(Ime::Preedit {
        text: "にほ".to_owned(),
        selection: 6..6,
    });
    fake.send(&mut app, window, composing);
    let ended = fake.window(window).unwrap().compositions_ended;
    command(&mut fake, &mut app, window, "paneler.close");
    assert!(panel_frame(&app, window, "dialog").is_none());
    assert_eq!(fake.window(window).unwrap().compositions_ended, ended + 1);
    assert!(app.windows[&window].pane_has_focus());
    // The plugin heard its closing once, as it asked for it.
    assert_eq!(heard.borrow().closed, ["dialog"]);
}
