//! The tests of the input of the slots' views: the pointer over them
//! before the panes, the keyboard's focus across the pane and their nodes,
//! a field's keys, its clipboard and the input method, and the texts and
//! the selections that their plugins set.

use std::cell::RefCell;
use std::rc::Rc;

use lokyr_kernel::plugins::ui::{UiEvent, ViewId};
use lokyr_kernel::plugins::view::{
    Direction, Kind as NodeKind, Layout as NodeLayout, Length, ListItem, Node,
    PointerShape as Shape, Style, Token, UiEventKind,
};
use lokyr_platform_core::{ContentRect, Ime, Titlebar};
use lokyr_text_core::FontConfig;

use super::*;
use crate::fakes;

/// What the plugin of [`Widgets`] heard, and what its commands set.
#[derive(Default)]
struct Heard {
    events: Vec<UiEvent>,
    /// The text and the revision that `widgets.set` sets in the field
    /// `name`.
    text: (String, u64),
    /// Whether the tab bar shows nothing.
    hidden: bool,
}

/// A plugin of a tab bar of a button `go`, of a tooltip and the pointing
/// hand, and of two fields, `name` and `other`, and of a status bar of a
/// list `rows`, a box `area` and a field `note`; `widgets.set` sets the text of `name`,
/// `widgets.select` the list's fourth item, `widgets.hide` hides the bar.
struct Widgets {
    heard: Rc<RefCell<Heard>>,
    views: Vec<ViewId>,
}

const MANIFEST: &str = concat!(
    "[plugin]\nname = \"lokyr/widgets\"\nkind = \"builtin\"\nlicense = \"MIT\"\n",
    "[contributes]\nslots = [\"tab-bar\", \"status-bar\"]\n",
);

fn sized(node: Node, width: f32, height: f32) -> Node {
    node.layout(NodeLayout {
        width: Length::Points(width),
        height: Length::Points(height),
        ..NodeLayout::default()
    })
}

impl plugins::Plugin for Widgets {
    fn activate(&mut self, cx: &mut plugins::Cx<'_>) -> Result<(), Error> {
        for name in ["widgets.set", "widgets.select", "widgets.hide"] {
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
        let bar = self
            .views
            .iter()
            .find(|view| matches!(view, ViewId::TabBar(_)))
            .cloned();
        let status = self
            .views
            .iter()
            .find(|view| matches!(view, ViewId::StatusBar(_)))
            .cloned();
        match name {
            "widgets.set" => {
                let (text, revision) = self.heard.borrow().text.clone();
                cx.set_text(bar.unwrap(), "name", &text, revision)
            }
            "widgets.select" => cx.select(status.unwrap(), "rows", 3),
            _ => {
                self.heard.borrow_mut().hidden = true;
                cx.request_render(bar.unwrap())
            }
        }
    }

    fn on_ui_event(&mut self, _: &mut plugins::Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.heard.borrow_mut().events.push(event.clone());
        Ok(())
    }

    fn render(&mut self, _: &mut plugins::Cx<'_>, view: &ViewId) -> Option<Node> {
        if !self.views.contains(view) {
            self.views.push(view.clone());
        }
        let row = |children| {
            let mut node = Node::container(children);
            node.layout.grow = 1.0;
            node.layout.gap = 8.0;
            node.style = Style {
                background: Some(Token::from("tab.bar.bg")),
                ..Style::default()
            };
            node
        };
        match view {
            ViewId::TabBar(_) if self.heard.borrow().hidden => None,
            ViewId::TabBar(_) => {
                let mut go = sized(Node::button("Go"), 40.0, 20.0)
                    .key("go")
                    .tooltip("Go there");
                go.style.pointer = Some(Shape::PointingHand);
                Some(row(vec![
                    go,
                    sized(Node::field("Name"), 80.0, 20.0)
                        .key("name")
                        .style(Style {
                            pointer: Some(Shape::Text),
                            ..Style::default()
                        }),
                    sized(Node::field("Other"), 80.0, 20.0).key("other"),
                ]))
            }
            ViewId::StatusBar(_) => {
                let mut rows = Node::list(
                    Node::text(""),
                    30,
                    (
                        0,
                        (0..30)
                            .map(|index| ListItem {
                                texts: vec![format!("{index}")],
                            })
                            .collect(),
                    ),
                );
                if let NodeKind::List(list) = &mut rows.kind {
                    list.row_height = Some(10.0);
                }
                let mut row = row(vec![
                    sized(rows, 60.0, 20.0).key("rows"),
                    sized(Node::container(Vec::new()), 60.0, 20.0).key("area"),
                    sized(Node::field("Note"), 60.0, 20.0).key("note"),
                ]);
                row.layout.direction = Direction::Row;
                Some(row)
            }
            _ => None,
        }
    }
}

/// The app without padding, of a titlebar of 28 points, its window
/// focused, the plugin of [`Widgets`] filling its tab bar and its status
/// bar, its pane running `program` until it shows `ready`.
fn widgets(program: Program) -> (Fake, App, WindowId, Rc<RefCell<Heard>>) {
    let heard = Rc::new(RefCell::new(Heard::default()));
    let plugin = Widgets {
        heard: Rc::clone(&heard),
        views: Vec::new(),
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
    fake.send(&mut app, window, WindowEvent::Frame);
    fake.send(&mut app, window, WindowEvent::Frame);
    assert_eq!(fake.window(window).unwrap().titlebar, Titlebar::Tabs);
    (fake, app, window, heard)
}

/// The middle of the node of `key` of a slot's view of `window`, in
/// points.
fn middle(fake: &Fake, app: &App, window: WindowId, key: &str) -> Point {
    let scale = fake.window(window).unwrap().size.scale;
    for view in &app.windows[&window].chrome {
        let (Some(frame), Some(node)) = (view.frame, view.surface.tree.by_key(&key.into())) else {
            continue;
        };
        let at = view.surface.layout.frame(&view.surface.tree, node).unwrap();
        return Point {
            x: f64::from(frame.x + at.x + at.width / 2.0) / scale,
            y: f64::from(frame.y + at.y + at.height / 2.0) / scale,
        };
    }
    panic!("no node `{key}`");
}

/// The kinds of the events of the node of `key` that the plugin heard,
/// but the hover's, and forgets them all.
fn heard_of(heard: &Rc<RefCell<Heard>>, key: &str) -> Vec<UiEventKind> {
    let events = std::mem::take(&mut heard.borrow_mut().events);
    events
        .into_iter()
        .filter(|event| event.key.0 == key)
        .map(|event| event.kind)
        .filter(|kind| !matches!(kind, UiEventKind::HoverEnter | UiEventKind::HoverLeave))
        .collect()
}

/// The surface of the slot's view of `window` that holds the node `key`.
fn surface_of<'a>(
    app: &'a App,
    window: WindowId,
    key: &str,
) -> &'a lokyr_ui_core::surface::Surface {
    app.windows[&window]
        .chrome
        .iter()
        .map(|view| &view.surface)
        .find(|surface| surface.tree.by_key(&key.into()).is_some())
        .expect("a view of the node")
}

/// The text of the field `name` of the tab bar.
fn name_text(app: &App, window: WindowId) -> String {
    surface_of(app, window, "name")
        .fields
        .state(&"name".into())
        .map(|state| state.text().to_owned())
        .unwrap_or_default()
}

/// A point over the pane, below the titlebar's tab bar.
const OVER_PANE: Point = Point { x: 9.0, y: 60.0 };

/// A left click at `at`.
fn click_at(fake: &mut Fake, app: &mut App, window: WindowId, at: Point) {
    fake.send(app, window, click(true, at, Modifiers::NONE, 1));
    fake.send(app, window, click(false, at, Modifiers::NONE, 1));
}

/// The press of the key at `code`, of `text`.
fn type_key(fake: &mut Fake, app: &mut App, window: WindowId, code: KeyCode, text: &str) {
    fake.send(app, window, key_event(code, text, Modifiers::NONE, true));
    fake.send(app, window, key_event(code, text, Modifiers::NONE, false));
}

#[test]
fn a_slot_s_view_takes_the_pointer_s_clicks_drags_and_scrolls_before_the_program() {
    // A program that takes the mouse's clicks and the wheel, and shows
    // what it reads.
    let (mut fake, mut app, window, heard) = widgets(reader("\\033[?1000h"));
    // A click on the bar's button: its plugin's, not the program's, and
    // the pane keeps the keyboard, as a button takes no focus.
    let go = middle(&fake, &app, window, "go");
    click_at(&mut fake, &mut app, window, go);
    assert_eq!(heard_of(&heard, "go"), [UiEventKind::Click]);
    assert!(app.windows[&window].pane_has_focus());
    // A drag from the status bar's box over the pane goes on to its end
    // there, the view keeping the pointer.
    let area = middle(&fake, &app, window, "area");
    fake.send(&mut app, window, click(true, area, Modifiers::NONE, 1));
    fake.send(&mut app, window, moved(OVER_PANE, Modifiers::NONE));
    fake.send(
        &mut app,
        window,
        click(false, OVER_PANE, Modifiers::NONE, 1),
    );
    let dragged = heard_of(&heard, "area");
    assert_eq!(
        dragged.first(),
        Some(&UiEventKind::DragStart),
        "{dragged:?}"
    );
    assert!(
        matches!(dragged.last(), Some(UiEventKind::DragEnd { .. })),
        "{dragged:?}"
    );
    // A scroll over the box, its plugin's too.
    let wheel = WindowEvent::Scroll {
        delta: ScrollDelta::Lines { x: 0.0, y: -1.0 },
        phase: None,
        momentum: false,
        position: area,
        mods: Modifiers::NONE,
    };
    fake.send(&mut app, window, wheel);
    assert_eq!(
        heard_of(&heard, "area"),
        [UiEventKind::Scroll { x: 0.0, y: -16.0 }]
    );
    // The program learnt of none of them: a key typed after them shows
    // with nothing before it.
    type_key(&mut fake, &mut app, window, KeyCode::KeyX, "x");
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "x"));
    assert!(!shows(&app, window, "^["), "{:?}", screen(&app, window));
}

#[test]
fn the_hover_enters_and_leaves_and_a_tooltip_shows_after_its_wait() {
    let (mut fake, mut app, window, heard) = widgets(reader(""));
    // The moves come over the slots' views alone.
    let areas = fake.window(window).unwrap().move_areas.clone();
    assert_eq!(areas.len(), 2, "{areas:?}");
    let go = middle(&fake, &app, window, "go");
    assert!(areas.iter().any(|area| area.contains(go)));
    assert!(!areas.iter().any(|area| area.contains(OVER_PANE)));
    // Over the button: its hover, and its tooltip after its wait.
    fake.send(&mut app, window, moved(go, Modifiers::NONE));
    let entered = std::mem::take(&mut heard.borrow_mut().events);
    assert!(
        entered
            .iter()
            .any(|event| event.key.0 == "go" && event.kind == UiEventKind::HoverEnter),
        "{entered:?}"
    );
    let (_, due) = app.tooltip_timer.expect("the tooltip's timer");
    fake.fire_timers(&mut app, due);
    let tooltip = |app: &App| {
        let surface = surface_of(app, window, "go");
        surface
            .tooltip
            .shown(&surface.tree)
            .map(|(text, _)| text.to_owned())
    };
    assert_eq!(tooltip(&app).as_deref(), Some("Go there"));
    // Out of the bar, over the pane: its leave, and the tooltip hides.
    fake.set_pointer(window, OVER_PANE);
    fake.send(&mut app, window, WindowEvent::MouseExited);
    let left = std::mem::take(&mut heard.borrow_mut().events);
    assert!(
        left.iter()
            .any(|event| event.key.0 == "go" && event.kind == UiEventKind::HoverLeave),
        "{left:?}"
    );
    assert_eq!(tooltip(&app), None);
    assert!(app.tooltip_timer.is_none());
}

#[test]
fn the_pointer_s_shapes_and_the_titlebar_s_areas_are_the_bar_s_nodes() {
    let (fake, app, window, _) = widgets(reader(""));
    let shown = fake.window(window).unwrap();
    // The hand over the button, the I-beam over the fields.
    let at = |key| middle(&fake, &app, window, key);
    let shape = |point: Point| {
        shown
            .pointer_areas
            .iter()
            .find(|area| {
                ContentRect {
                    origin: area.origin,
                    size: area.size,
                }
                .contains(point)
            })
            .map(|area| area.shape)
    };
    assert_eq!(shape(at("go")), Some(PointerShape::PointingHand));
    assert_eq!(shape(at("name")), Some(PointerShape::Text));
    // The bar's nodes take the presses over the titlebar, the rest of it
    // the window's.
    let areas = &shown.titlebar_areas;
    assert_eq!(areas.len(), 3, "{areas:?}");
    for key in ["go", "name", "other"] {
        assert!(areas.iter().any(|area| area.contains(at(key))), "{key}");
    }
}

#[test]
fn a_field_takes_the_focus_and_the_keys_tab_moves_it_and_escape_gives_them_back() {
    let (mut fake, mut app, window, heard) = widgets(reader(""));
    // A click on the field: its focus, and the keys.
    let name = middle(&fake, &app, window, "name");
    click_at(&mut fake, &mut app, window, name);
    assert!(!app.windows[&window].pane_has_focus());
    assert!(heard_of(&heard, "name").contains(&UiEventKind::Focus));
    type_key(&mut fake, &mut app, window, KeyCode::KeyA, "a");
    type_key(&mut fake, &mut app, window, KeyCode::KeyB, "b");
    assert_eq!(name_text(&app, window), "ab");
    let changed = heard_of(&heard, "name");
    assert_eq!(
        changed.last(),
        Some(&UiEventKind::InputChanged {
            text: "ab".to_owned(),
            revision: 2
        })
    );
    // Tab moves it to the other field, of the same view.
    type_key(&mut fake, &mut app, window, KeyCode::Tab, "\t");
    let events = std::mem::take(&mut heard.borrow_mut().events);
    let kinds: Vec<(&str, &UiEventKind)> = events
        .iter()
        .map(|event| (event.key.0.as_str(), &event.kind))
        .collect();
    assert_eq!(
        kinds,
        [("name", &UiEventKind::Blur), ("other", &UiEventKind::Focus)]
    );
    // Escape gives it back to the pane, which takes the keys again.
    type_key(&mut fake, &mut app, window, KeyCode::Escape, "\u{1b}");
    assert!(app.windows[&window].pane_has_focus());
    assert_eq!(heard_of(&heard, "other"), [UiEventKind::Blur]);
    type_key(&mut fake, &mut app, window, KeyCode::KeyZ, "z");
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "z"));
    // None of the field's keys, nor Tab nor Escape, reached the program.
    assert!(!shows(&app, window, "ab"), "{:?}", screen(&app, window));
    assert!(!shows(&app, window, "^I"), "{:?}", screen(&app, window));
    assert!(!shows(&app, window, "^["), "{:?}", screen(&app, window));
    // A click on the pane takes it back from a field too, and a view that
    // takes its field away gives it back.
    click_at(&mut fake, &mut app, window, name);
    click_at(&mut fake, &mut app, window, OVER_PANE);
    assert!(app.windows[&window].pane_has_focus());
    click_at(&mut fake, &mut app, window, name);
    app.run(&mut fake, "widgets.hide", Args::new());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].pane_has_focus()
    });
}

#[test]
fn copy_paste_and_select_all_are_the_field_s_while_it_has_the_focus() {
    let (mut fake, mut app, window, _) = widgets(reader(""));
    fake.set_clipboard_text("pasted");
    let name = middle(&fake, &app, window, "name");
    click_at(&mut fake, &mut app, window, name);
    // The Edit menu's commands, as their keys run them.
    app.run(&mut fake, "pane.paste", Args::new());
    assert_eq!(name_text(&app, window), "pasted");
    fake.set_clipboard_text("other");
    app.run(&mut fake, "pane.select-all", Args::new());
    app.run(&mut fake, "pane.copy", Args::new());
    assert_eq!(fake.clipboard_text().as_deref(), Some("pasted"));
}

#[test]
fn the_input_method_composes_in_the_field_and_a_move_of_the_focus_ends_it() {
    let (mut fake, mut app, window, heard) = widgets(reader(""));
    let name = middle(&fake, &app, window, "name");
    click_at(&mut fake, &mut app, window, name);
    heard_of(&heard, "name");
    let composing = WindowEvent::Ime(Ime::Preedit {
        text: "にほ".to_owned(),
        selection: 6..6,
    });
    fake.send(&mut app, window, composing);
    let surface = surface_of(&app, window, "name");
    assert!(surface.fields.composing(&surface.tree).is_some());
    // Escape keeps it in the field as it stands, and the input method
    // forgets it, whose next commit would go to the pane.
    let ended = fake.window(window).unwrap().compositions_ended;
    type_key(&mut fake, &mut app, window, KeyCode::Escape, "\u{1b}");
    assert_eq!(fake.window(window).unwrap().compositions_ended, ended + 1);
    assert_eq!(name_text(&app, window), "にほ");
    assert!(
        heard_of(&heard, "name").contains(&UiEventKind::InputChanged {
            text: "にほ".to_owned(),
            revision: 1
        })
    );
    // A commit with the focus back in the field types there.
    click_at(&mut fake, &mut app, window, name);
    fake.send(
        &mut app,
        window,
        WindowEvent::Ime(Ime::Commit("ん".to_owned())),
    );
    assert_eq!(name_text(&app, window), "にほん");
}

#[test]
fn a_plugin_s_text_goes_over_no_keys_typed_since_and_its_selection_shows() {
    let (mut fake, mut app, window, heard) = widgets(reader(""));
    let name = middle(&fake, &app, window, "name");
    click_at(&mut fake, &mut app, window, name);
    type_key(&mut fake, &mut app, window, KeyCode::KeyA, "a");
    // The plugin's answer to the first revision.
    heard.borrow_mut().text = ("hello".to_owned(), 1);
    app.run(&mut fake, "widgets.set", Args::new());
    assert_eq!(name_text(&app, window), "hello");
    // A key typed since: an answer to the first revision is too old.
    type_key(&mut fake, &mut app, window, KeyCode::KeyB, "b");
    assert_eq!(name_text(&app, window), "hellob");
    app.run(&mut fake, "widgets.set", Args::new());
    assert_eq!(name_text(&app, window), "hellob");
    // The text that the field holds changes nothing, its caret moved.
    type_key(&mut fake, &mut app, window, KeyCode::ArrowLeft, "");
    let caret = |app: &App| {
        surface_of(app, window, "name")
            .fields
            .state(&"name".into())
            .unwrap()
            .caret()
    };
    assert_eq!(caret(&app), 5);
    heard.borrow_mut().text = ("hellob".to_owned(), 2);
    app.run(&mut fake, "widgets.set", Args::new());
    assert_eq!(caret(&app), 5);
    // The list's selection.
    app.run(&mut fake, "widgets.select", Args::new());
    let rows = surface_of(&app, window, "rows")
        .lists
        .state(&"rows".into())
        .and_then(lokyr_ui_core::list::ListState::selected);
    assert_eq!(rows, Some(3));
}

#[test]
fn escape_from_fields_of_two_views_that_took_the_focus_from_each_other_reaches_the_pane() {
    let (mut fake, mut app, window, _) = widgets(reader(""));
    let (name, note) = (
        middle(&fake, &app, window, "name"),
        middle(&fake, &app, window, "note"),
    );
    click_at(&mut fake, &mut app, window, name);
    click_at(&mut fake, &mut app, window, note);
    click_at(&mut fake, &mut app, window, name);
    type_key(&mut fake, &mut app, window, KeyCode::Escape, "\u{1b}");
    assert!(app.windows[&window].pane_has_focus());
}

#[test]
fn tab_s_release_after_a_key_typed_before_it_went_up_keeps_the_key() {
    let (mut fake, mut app, window, _) = widgets(reader(""));
    let name = middle(&fake, &app, window, "name");
    click_at(&mut fake, &mut app, window, name);
    // Tab down, `a` down before Tab goes up, then `b`.
    let none = Modifiers::NONE;
    for (code, text, pressed) in [
        (KeyCode::Tab, "\t", true),
        (KeyCode::KeyA, "a", true),
        (KeyCode::Tab, "\t", false),
        (KeyCode::KeyA, "a", false),
    ] {
        fake.send(&mut app, window, key_event(code, text, none, pressed));
    }
    type_key(&mut fake, &mut app, window, KeyCode::KeyB, "b");
    let other = surface_of(&app, window, "other")
        .fields
        .state(&"other".into())
        .map(|state| state.text().to_owned());
    assert_eq!(other.as_deref(), Some("ab"));
}

#[test]
fn the_pane_s_program_learns_of_the_focus_that_a_render_gives_back() {
    // A program of the focus's reports.
    let (mut fake, mut app, window, _) = widgets(reader("\\033[?1004h"));
    let name = middle(&fake, &app, window, "name");
    click_at(&mut fake, &mut app, window, name);
    // The bar hides, its field with it: the focus back to the pane, which
    // the program hears before the key.
    app.run(&mut fake, "widgets.hide", Args::new());
    run_until(&mut fake, &mut app, |_, app| {
        app.windows[&window].pane_has_focus()
    });
    type_key(&mut fake, &mut app, window, KeyCode::KeyZ, "z");
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "z"));
    assert!(
        shows(&app, window, "^[[O^[[Iz"),
        "{:?}",
        screen(&app, window)
    );
}

#[test]
fn a_paste_with_a_node_of_no_field_focused_goes_nowhere() {
    let (mut fake, mut app, window, _) = widgets(reader(""));
    fake.set_clipboard_text("pasted");
    // The list takes the focus: the Edit menu's paste is its, and does
    // nothing.
    let rows = middle(&fake, &app, window, "rows");
    click_at(&mut fake, &mut app, window, rows);
    assert!(!app.windows[&window].pane_has_focus());
    app.run(&mut fake, "pane.paste", Args::new());
    type_key(&mut fake, &mut app, window, KeyCode::Escape, "\u{1b}");
    type_key(&mut fake, &mut app, window, KeyCode::KeyZ, "z");
    run_until(&mut fake, &mut app, |_, app| shows(app, window, "z"));
    assert!(!shows(&app, window, "pasted"), "{:?}", screen(&app, window));
}
