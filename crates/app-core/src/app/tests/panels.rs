//! The tests of the plugins' panels in the windows: their anchors, their
//! roles' focus and pointer, what closes them, and the menus that the
//! platform shows as its own.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{
    Anchor, Corner, PanelId, PanelOptions, Role, Slot, UiEvent, ViewId,
};
use lokyr_kernel::plugins::view::{
    Layout as NodeLayout, Length, Node, ProgressShape, ProgressState, UiEventKind,
};
use lokyr_platform_core::Ime;
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;
use crate::window::NativeMenu;

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
/// its top right corner; `paneler.close` closes the dialog;
/// `paneler.commands`, a menu of two buttons alone keyed `window.new` and
/// `third`, which `paneler.close-commands` closes; `paneler.both` opens
/// `native` and `commands` in one call.
struct Paneler {
    heard: Rc<RefCell<Heard>>,
}

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/paneler\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"tab-bar\"]\n",
);

const COMMANDS: [&str; 11] = [
    "paneler.dialog",
    "paneler.menu",
    "paneler.native",
    "paneler.tip",
    "paneler.cell",
    "paneler.cursor",
    "paneler.corner",
    "paneler.close",
    "paneler.commands",
    "paneler.close-commands",
    "paneler.both",
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
            "paneler.commands" => (
                "commands",
                options(Anchor::Pointer(window), Role::Menu, (true, true)),
            ),
            "paneler.close-commands" => return cx.close_panel("commands"),
            "paneler.both" => {
                for id in ["native", "commands"] {
                    cx.open_panel(
                        id,
                        options(Anchor::Pointer(window), Role::Menu, (true, true)),
                    )?;
                }
                return Ok(());
            }
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
            let mut bar = Node::container(vec![
                sized(Node::button("Open"), 60.0, 20.0).key("open"),
                sized(
                    Node::progress(ProgressShape::Bar, ProgressState::Normal, 0.5),
                    100.0,
                    4.0,
                )
                .key("progress"),
            ]);
            bar.layout.grow = 1.0;
            return Some(bar);
        };
        Some(match id.as_str() {
            "native" => Node::container(vec![
                Node::button("First").key("first"),
                Node::button("Second").key("second"),
            ]),
            "commands" => Node::container(vec![
                Node::button("New Window").key("window.new"),
                Node::button("Third").key("third"),
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
fn a_fill_that_a_panel_covers_is_the_frame_s_to_draw_under_the_panel() {
    let (mut fake, mut app, window, _) = paneler(reader(""));
    fake.set_draws_fills(true);
    frames(&mut fake, &mut app, window);
    let fills = |fake: &Fake| fake.window(window).unwrap().fills.len();
    let covered = |app: &App| {
        app.windows[&window]
            .chrome
            .iter()
            .find(|view| view.view.slot() == Some(Slot::TabBar))
            .map(|view| view.surface.covered.len())
    };
    // The bar's progress, which the window server fills.
    assert_eq!((fills(&fake), covered(&app)), (1, Some(0)));
    // A menu at the pointer over it: the frame fills it, under the menu.
    let scale = fake.window(window).unwrap().size.scale;
    let over = middle(node_frame(&app, window, "progress"), scale);
    fake.set_pointer(window, over);
    command(&mut fake, &mut app, window, "paneler.menu");
    assert!(panel_frame(&app, window, "menu").is_some());
    assert_eq!((fills(&fake), covered(&app)), (0, Some(1)));
    // Closed, the window server fills it again.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", Modifiers::NONE, true),
    );
    frames(&mut fake, &mut app, window);
    assert!(panel_frame(&app, window, "menu").is_none());
    assert_eq!((fills(&fake), covered(&app)), (1, Some(0)));
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
    assert!(app.windows[&window].native.is_empty());
}

/// The commands of the items of the platform's last menu.
fn last_menu(fake: &Fake) -> Vec<String> {
    let (_, _, menu) = fake.context_menus().last().cloned().expect("a menu");
    menu.items
        .iter()
        .filter_map(|item| match item {
            lokyr_platform_core::MenuItem::Command { command, .. } => Some(command.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_native_menu_whose_panel_its_plugin_closes_closes_and_its_choice_runs_no_command() {
    let (mut fake, mut app, window, heard) = paneler(reader(""));
    command(&mut fake, &mut app, window, "paneler.commands");
    assert_eq!(last_menu(&fake), ["window.new", "third"]);
    command(&mut fake, &mut app, window, "paneler.close-commands");
    assert_eq!(fake.closed_context_menus(), [window]);
    assert_eq!(heard.borrow().closed, ["commands"]);
    // A choice that came before the platform's menu closed runs nothing,
    // though its item's key is an app's command.
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand("window.new".to_owned()),
    );
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    frames(&mut fake, &mut app, window);
    assert_eq!(fake.windows().count(), 1);
    assert!(heard.borrow().events.is_empty());
    assert!(app.windows[&window].native.is_empty());
    // The window's own context menu runs its commands still.
    right_click(&mut fake, &mut app, window, OVER_PANE, Modifiers::NONE);
    assert_eq!(fake.context_menus().len(), 2);
    assert_eq!(app.windows[&window].native, [NativeMenu::Window]);
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand("window.new".to_owned()),
    );
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    assert_eq!(fake.windows().count(), 2);
}

#[test]
fn native_menus_shown_one_after_another_close_in_their_order_each_told() {
    let (mut fake, mut app, window, heard) = paneler(reader(""));
    command(&mut fake, &mut app, window, "paneler.native");
    command(&mut fake, &mut app, window, "paneler.commands");
    assert_eq!(fake.context_menus().len(), 2);
    // The first's closing, as the second's showing closed it, closes the
    // first's panel alone.
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    frames(&mut fake, &mut app, window);
    assert_eq!(heard.borrow().closed, ["native"]);
    assert_eq!(app.windows[&window].native.len(), 1);
    // The second's choice is its own.
    fake.send(
        &mut app,
        window,
        WindowEvent::ContextCommand("third".to_owned()),
    );
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    frames(&mut fake, &mut app, window);
    assert!(
        heard
            .borrow()
            .events
            .iter()
            .any(|event| event.key.0 == "third" && event.kind == UiEventKind::Click)
    );
    assert_eq!(heard.borrow().closed, ["native", "commands"]);
    // Two of one call show in their order, and the first opens again.
    command(&mut fake, &mut app, window, "paneler.both");
    assert_eq!(fake.context_menus().len(), 4);
    assert_eq!(last_menu(&fake), ["window.new", "third"]);
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    fake.send(&mut app, window, WindowEvent::ContextMenuClosed);
    frames(&mut fake, &mut app, window);
    assert_eq!(
        heard.borrow().closed,
        ["native", "commands", "native", "commands"]
    );
    command(&mut fake, &mut app, window, "paneler.native");
    assert_eq!(fake.context_menus().len(), 5);
}

#[test]
fn a_dialog_that_takes_the_focus_as_it_opens_ends_the_pane_s_composition() {
    let (mut fake, mut app, window, _) = paneler(reader(""));
    let composing = Ime::Preedit {
        text: "にほ".to_owned(),
        selection: 6..6,
    };
    fake.send(&mut app, window, WindowEvent::Ime(composing));
    assert!(app.windows[&window].preedit.is_some());
    let ended = fake.window(window).unwrap().compositions_ended;
    command(&mut fake, &mut app, window, "paneler.dialog");
    let state = &app.windows[&window];
    assert!(!state.pane_has_focus());
    // The pane's composition ended as the dialog's field took the keys,
    // which the input method forgot.
    assert!(state.preedit.is_none());
    assert_eq!(fake.window(window).unwrap().compositions_ended, ended + 1);
}

/// Whether the titlebar's own areas of `window` hold `at`, in points.
fn titlebar_holds(fake: &Fake, window: WindowId, at: Point) -> bool {
    fake.window(window)
        .unwrap()
        .titlebar_areas
        .iter()
        .any(|area| area.contains(at))
}

#[test]
fn a_panel_over_the_titlebar_takes_its_presses_there_and_a_menu_the_whole_bar() {
    let (mut fake, mut app, window, heard) = paneler(reader(""));
    let scale = fake.window(window).unwrap().size.scale;
    // Far right on the titlebar, the window's own until a menu shows.
    let bar_end = Point { x: 380.0, y: 10.0 };
    assert!(!titlebar_holds(&fake, window, bar_end));
    // A menu at the pointer over the titlebar takes its presses, and the
    // bar's too, whose press closes it, of no further use.
    fake.send(
        &mut app,
        window,
        moved(Point { x: 120.0, y: 6.0 }, Modifiers::NONE),
    );
    command(&mut fake, &mut app, window, "paneler.menu");
    let frame = panel_frame(&app, window, "menu").expect("the menu");
    let on_menu = Point {
        x: f64::from(frame.x) / scale + 4.0,
        y: f64::from(frame.y) / scale + 2.0,
    };
    assert!(on_menu.y < 28.0);
    assert!(titlebar_holds(&fake, window, on_menu));
    assert!(titlebar_holds(&fake, window, bar_end));
    fake.send(&mut app, window, click(true, bar_end, Modifiers::NONE, 1));
    fake.send(&mut app, window, click(false, bar_end, Modifiers::NONE, 1));
    frames(&mut fake, &mut app, window);
    assert_eq!(heard.borrow().closed, ["menu"]);
    assert!(!titlebar_holds(&fake, window, bar_end));
    // A dialog below the bar's button takes the presses of its part over
    // the titlebar alone.
    command(&mut fake, &mut app, window, "paneler.dialog");
    let frame = panel_frame(&app, window, "dialog").expect("the dialog");
    let top = f64::from(frame.y) / scale;
    assert!(top < 28.0, "{frame:?}");
    let on_dialog = Point {
        x: f64::from(frame.x) / scale + 4.0,
        y: (top + 28.0) / 2.0,
    };
    assert!(titlebar_holds(&fake, window, on_dialog));
    assert!(!titlebar_holds(&fake, window, bar_end));
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

#[test]
fn a_panel_is_in_the_window_s_tree_over_the_slots_and_its_field_has_the_focus() {
    let (mut fake, mut app, window, _) = paneler(reader(""));
    assert!(fake.ask_accessibility(&mut app, window));
    command(&mut fake, &mut app, window, "paneler.dialog");
    fake.fire_timers(&mut app, Instant::now() + Duration::from_secs(1));
    let tree = fake
        .window(window)
        .unwrap()
        .accessibility
        .last()
        .unwrap()
        .clone();
    let id_of = |named: &dyn Fn(&accesskit::Node) -> bool| {
        tree.nodes
            .iter()
            .find(|(_, node)| named(node))
            .map(|(id, _)| *id)
    };
    let (open, find) = (
        id_of(&|node| node.label() == Some("Open")).expect("the bar's button"),
        id_of(&|node| node.placeholder() == Some("Find")).expect("the dialog's field"),
    );
    // The dialog after the bar, of its field's focus.
    let (_, root) = tree
        .nodes
        .iter()
        .find(|(id, _)| *id == lokyr_ui_core::access::WINDOW)
        .unwrap();
    let surfaces: Vec<u8> = root
        .children()
        .iter()
        .filter_map(|&id| lokyr_ui_core::access::view::surface_of(id))
        .collect();
    let (bar, dialog) = (
        lokyr_ui_core::access::view::surface_of(open).unwrap(),
        lokyr_ui_core::access::view::surface_of(find).unwrap(),
    );
    let place = |number| surfaces.iter().position(|&at| at == number);
    assert!(place(bar) < place(dialog), "{surfaces:?}");
    assert_eq!(tree.focus, find);
}

/// The text of the find bar's count, as `window` shows it.
fn find_count(app: &App, window: WindowId) -> Option<String> {
    let view = app.windows[&window]
        .chrome
        .iter()
        .find(|view| view.view == ViewId::Panel(PanelId::from("find")))?;
    let tree = &view.surface.tree;
    let node = tree.by_key(&"count".into())?;
    match tree.kind(node)? {
        lokyr_kernel::plugins::view::Kind::Text(text) => Some(text.clone()),
        _ => None,
    }
}

#[test]
fn the_find_bar_searches_its_pane_steps_and_leaves_its_match_selected_as_it_closes() {
    let none = Modifiers::NONE;
    let drawing = Drawing::new(FontConfig::default()).unwrap();
    let mut fake = Fake::default();
    fake.set_bar(28.0);
    let program = shell("printf 'a needle\\nnothing\\nneedle 日本 two\\n'; echo ready; read line");
    let mut app = App::new(Some(drawing), unpadded(), None).with_program(program);
    fake.start(&mut app);
    let window = fake.windows().next().unwrap();
    app.windows.get_mut(&window).unwrap().surface = Some(fakes::Surface::default());
    fake.send(&mut app, window, WindowEvent::Focused(true));
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "ready"));
    frames(&mut fake, &mut app, window);
    command(&mut fake, &mut app, window, "search.open");
    assert!(panel_frame(&app, window, "find").is_some());
    assert!(focused_in(&app, window, "find"));
    // Typed, as an input method commits it: the last match, the nearest
    // to the bottom, selected, of two.
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("needle".to_owned())),
    );
    run_until(&mut fake, &mut app, |_, app| {
        find_count(app, window).as_deref() == Some("2 of 2")
    });
    assert_eq!(selected_text(&app, window).as_deref(), Some("needle"));
    let row = |app: &App| {
        app.pane(window)
            .selection()
            .map(|selected| selected.start.1)
    };
    let bottom = row(&app);
    // Return, up, the other.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Enter, "\r", none, true),
    );
    run_until(&mut fake, &mut app, |_, app| {
        find_count(app, window).as_deref() == Some("1 of 2")
    });
    assert_ne!(row(&app), bottom);
    // Japanese, as its input method commits it, in place of the text.
    fake.send(&mut app, window, key_event(KeyCode::KeyA, "a", CMD, true));
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("日本".to_owned())),
    );
    run_until(&mut fake, &mut app, |_, app| {
        find_count(app, window).as_deref() == Some("1 of 1")
    });
    assert_eq!(selected_text(&app, window).as_deref(), Some("日本"));
    // Escape closes it, its match staying selected.
    fake.send(
        &mut app,
        window,
        key_event(KeyCode::Escape, "\u{1b}", none, true),
    );
    frames(&mut fake, &mut app, window);
    assert!(panel_frame(&app, window, "find").is_none());
    assert_eq!(selected_text(&app, window).as_deref(), Some("日本"));
}
