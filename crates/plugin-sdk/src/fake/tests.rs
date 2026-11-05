use std::collections::BTreeMap;

use super::*;
use crate::command::Category;
use crate::host::Edge;
use crate::host::Scroll;
use crate::ui::{Anchor, Role, Slot};
use crate::view::{ProgressShape, ProgressState, Role as NodeRole, Tab};

type Callback = Box<dyn FnMut(&mut Cx<'_>, &str) -> Result<(), Error>>;

/// A plugin whose command's call the test gives, which takes the command's
/// name, and which renders `tree`.
struct Probe {
    on_command: Callback,
    tree: Option<Node>,
    heard: Vec<Vec<String>>,
}

impl Probe {
    fn new(on_command: impl FnMut(&mut Cx<'_>, &str) -> Result<(), Error> + 'static) -> Self {
        Self {
            on_command: Box::new(on_command),
            tree: None,
            heard: Vec::new(),
        }
    }
}

impl Plugin for Probe {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "probe.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.subscribe(&["tab-created", "probe.ping"])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        (self.on_command)(cx, name)
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.heard
            .push(events.iter().map(|event| event.name().to_owned()).collect());
        Ok(())
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        let pane = cx.focused_pane().unwrap();
        cx.select_all(pane).is_ok()
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<Node> {
        self.tree.clone()
    }
}

#[test]
fn a_read_after_a_write_reads_what_was_before_and_the_write_applies_as_the_call_returns() {
    let mut host = FakeHost::new("acme/probe");
    host.focus(Some(PaneId(2))).set_clipboard(Some("old"));
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let saw = std::rc::Rc::clone(&seen);
    let mut probe = Probe::new(move |cx, _| {
        cx.set_clipboard_text("new")?;
        saw.borrow_mut().push(cx.clipboard_text()?);
        cx.set_state(Data::Integer(4))?;
        saw.borrow_mut().push(Some(format!("{:?}", cx.state()?)));
        Ok(())
    });
    host.activate(&mut probe).unwrap();
    assert_eq!(host.commands()[0].name, "probe.go");
    assert_eq!(host.subscriptions(), ["tab-created", "probe.ping"]);
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(
        *seen.borrow(),
        [Some("old".to_owned()), Some("Nil".to_owned())]
    );
    assert_eq!(host.clipboard(), Some("new"));
    assert_eq!(*host.state(), Data::Integer(4));
    assert_eq!(
        host.take_done(),
        [
            Request::SetClipboardText("new".to_owned()),
            Request::SetState(Data::Integer(4)),
        ]
    );
    // A look drops what it asks.
    assert!(host.applies(&mut probe, "probe.go", &Args::new()));
    assert!(host.take_done().is_empty());
}

#[test]
fn a_request_of_a_pane_not_there_or_that_the_plugin_may_not_make_fails() {
    let mut host = FakeHost::new("acme/probe").with_capabilities(vec![Capability::ClipboardRead]);
    host.pane(PaneId(1)).selection = Some("x".to_owned());
    let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let failed = std::rc::Rc::clone(&errors);
    let mut probe = Probe::new(move |cx, _| {
        let push = |error: Option<Error>| failed.borrow_mut().push(error.map(|error| error.code));
        push(cx.clipboard_text().err());
        push(cx.selection_text(PaneId(1)).err());
        push(cx.scroll(PaneId(9), Scroll::Top).err());
        push(cx.open("javascript:alert(1)").err());
        push(cx.emit("other.ping", "x/1", Data::Nil).err());
        push(cx.emit("probe.ping", "probe/1", Data::Nil).err());
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(
        *errors.borrow(),
        [
            None,
            Some(Code::MissingCapability),
            Some(Code::NotFound),
            Some(Code::MissingCapability),
            Some(Code::InvalidName),
            None,
        ]
    );
    // A first-party plugin's open of a scheme that does not open fails.
    let mut host = FakeHost::new("acme/probe");
    let mut probe = Probe::new(|cx, _| {
        assert!(cx.opens("https://lokyr.dev")? && cx.opens("~/notes")?);
        assert!(!cx.opens("javascript:x")?);
        cx.open("javascript:x")
    });
    host.set_missing(&["~/gone"]);
    assert_eq!(
        host.command(&mut probe, "probe.go", Args::new())
            .unwrap_err()
            .code,
        Code::Failed
    );
    let mut probe = Probe::new(|cx, _| {
        assert!(!cx.opens("~/gone")?);
        cx.open("~/gone")
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(host.take_done(), [Request::Open("~/gone".to_owned())]);
}

#[test]
fn the_events_that_a_plugin_is_owed_come_as_a_batch_of_those_of_its_own_and_its_subscriptions() {
    let mut host = FakeHost::new("acme/probe");
    host.pane(PaneId(1)).lines.insert(
        5,
        LineText {
            text: "five".to_owned(),
            cells: Vec::new(),
        },
    );
    let mut probe = Probe::new(|cx, _| {
        let window = WindowId(1);
        cx.read_lines(PaneId(1), 0, 10)?;
        cx.emit("probe.ping", "probe/1", Data::Nil)?;
        cx.open_panel(
            "menu",
            PanelOptions {
                anchor: Anchor::Pointer(window),
                role: Role::Menu,
                close_outside: true,
                close_escape: true,
            },
        )?;
        cx.close_panel("menu")
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.post(Event::Workspace(crate::event::WorkspaceEvent::TabCreated {
        tab: TabId(2),
        window: WindowId(1),
        index: 1,
    }));
    host.post(Event::Workspace(crate::event::WorkspaceEvent::TabClosed {
        tab: TabId(2),
        window: WindowId(1),
    }));
    host.set_config(Data::Map(BTreeMap::from([(
        "a".to_owned(),
        Data::Integer(1),
    )])));
    host.deliver(&mut probe).unwrap();
    assert_eq!(
        probe.heard,
        [[
            "lines",
            "panel-closed",
            "probe.ping",
            "tab-created",
            "config-changed"
        ]]
    );
    assert!(host.panels().is_empty());
    // Nothing owed, nothing delivered.
    host.deliver(&mut probe).unwrap();
    assert_eq!(probe.heard.len(), 1);
}

#[test]
fn the_workspace_s_reads_give_the_panes_windows_and_tabs() {
    let mut host = FakeHost::new("acme/probe");
    for (pane, tab) in [(1, 1), (2, 1), (3, 2)] {
        host.pane(PaneId(pane)).tab = TabId(tab);
    }
    host.focus(Some(PaneId(2)));
    let mut probe = Probe::new(|cx, _| {
        assert_eq!(cx.focused_window(), Some(WindowId(1)));
        let windows = cx.windows()?;
        assert_eq!(
            windows,
            [WindowInfo {
                id: WindowId(1),
                tabs: vec![TabId(1), TabId(2)],
                active: TabId(1),
            }]
        );
        let tabs = cx.tabs(WindowId(1))?;
        assert_eq!(tabs[0].panes, [PaneId(1), PaneId(2)]);
        assert_eq!((tabs[0].focused, tabs[0].active), (PaneId(2), true));
        assert_eq!(tabs[1].focused, PaneId(3));
        assert_eq!(
            cx.layout(TabId(1))?,
            Layout::Split {
                direction: Direction::Horizontal,
                ratio: None,
                first: Box::new(Layout::Pane(PaneId(1))),
                second: Box::new(Layout::Pane(PaneId(2))),
            }
        );
        assert_eq!(cx.layout(TabId(9)).unwrap_err().code, Code::NotFound);
        let (tab, pane) = cx.new_tab(WindowId(1), None)?;
        assert_ne!(tab, TabId(1));
        assert_ne!(pane, PaneId(1));
        cx.close_pane(PaneId(3))
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let done = host.take_done();
    assert!(matches!(done[0], Request::NewTab { .. }));
    assert_eq!(done[1], Request::ClosePane(PaneId(3)));
    // The new tab is the window's, the closed pane's tab gone with it.
    let mut probe = Probe::new(|cx, _| {
        let tabs: Vec<TabId> = cx.tabs(WindowId(1))?.iter().map(|tab| tab.id).collect();
        assert_eq!(tabs, [TabId(1), TabId(FIRST_ID)]);
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

/// The names of the events that `probe` heard, of every batch.
fn heard(probe: &Probe) -> Vec<String> {
    probe.heard.iter().flatten().cloned().collect()
}

#[test]
fn a_call_s_new_tab_and_split_enter_the_workspace_its_later_writes_find_them_and_their_events_are_owed()
 {
    let mut host = FakeHost::new("acme/probe");
    host.focus(Some(PaneId(1)));
    let mut probe = Probe::new(|cx, _| {
        cx.subscribe(&crate::event::WorkspaceEvent::NAMES)?;
        let (tab, pane) = cx.new_tab(WindowId(1), None)?;
        let split = cx.split(pane, Direction::Vertical, Side::Second)?;
        cx.send_text(split, "make\n")?;
        cx.focus_pane(split)?;
        cx.activate_tab(tab)?;
        cx.zoom(tab, Some(pane))?;
        // A read of the call reads the workspace before its writes.
        assert_eq!(cx.tabs(WindowId(1))?.len(), 1);
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let (tab, pane, split) = (TabId(FIRST_ID), PaneId(FIRST_ID + 1), PaneId(FIRST_ID + 2));
    host.deliver(&mut probe).unwrap();
    assert_eq!(
        heard(&probe),
        [
            "tab-created",
            "pane-created",
            "pane-created",
            "layout-changed",
            "pane-focused",
            "tab-activated",
            "layout-changed",
        ]
    );
    let mut probe = Probe::new(move |cx, _| {
        assert_eq!(
            cx.windows()?,
            [WindowInfo {
                id: WindowId(1),
                tabs: vec![TabId(1), tab],
                active: tab,
            }]
        );
        // The window shows the new tab, whose keyboard the split has.
        assert_eq!(cx.focused_pane(), Some(split));
        let tabs = cx.tabs(WindowId(1))?;
        assert_eq!(tabs[1].panes, [pane, split]);
        assert_eq!((tabs[1].focused, tabs[1].zoomed), (split, Some(pane)));
        assert_eq!(
            cx.layout(tab)?,
            Layout::Split {
                direction: Direction::Vertical,
                ratio: None,
                first: Box::new(Layout::Pane(pane)),
                second: Box::new(Layout::Pane(split)),
            }
        );
        cx.set_ratio(tab, Vec::new(), Some(0.99))
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let mut probe = Probe::new(move |cx, _| {
        let Layout::Split { ratio, .. } = cx.layout(tab)? else {
            panic!("a split");
        };
        // Within 5 % of its parts' edges, as the app keeps it.
        assert_eq!(ratio, Some(0.95));
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn closes_and_moves_change_the_workspace_as_the_app_s_and_the_focus_follows() {
    let mut host = FakeHost::new("acme/probe");
    for (pane, tab) in [(1, 1), (2, 2), (3, 2)] {
        host.pane(PaneId(pane)).tab = TabId(tab);
    }
    host.focus(Some(PaneId(3)));
    let mut probe = Probe::new(|cx, _| {
        cx.subscribe(&crate::event::WorkspaceEvent::NAMES)?;
        assert_eq!(cx.focused_pane(), Some(PaneId(3)));
        cx.close_pane(PaneId(3))?;
        cx.move_tab(TabId(2), WindowId(1), 0)
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.deliver(&mut probe).unwrap();
    assert_eq!(
        heard(&probe),
        ["pane-closed", "layout-changed", "pane-focused", "tab-moved"]
    );
    // The tab's keyboard went to the pane that took the room.
    let mut probe = Probe::new(|cx, _| {
        cx.subscribe(&crate::event::WorkspaceEvent::NAMES)?;
        assert_eq!(cx.focused_pane(), Some(PaneId(2)));
        assert_eq!(cx.windows()?[0].tabs, [TabId(2), TabId(1)]);
        cx.close_tab(TabId(2))
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.deliver(&mut probe).unwrap();
    assert_eq!(
        heard(&probe),
        ["pane-closed", "tab-closed", "tab-activated"]
    );
    // A tab alone in its window stays; the window closes with its last
    // pane, and the focus with it.
    let mut probe = Probe::new(|cx, _| {
        cx.subscribe(&crate::event::WorkspaceEvent::NAMES)?;
        assert_eq!(cx.focused_pane(), Some(PaneId(1)));
        cx.tab_to_new_window(TabId(1))?;
        cx.close_pane(PaneId(1))
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.deliver(&mut probe).unwrap();
    assert_eq!(
        heard(&probe),
        ["pane-closed", "tab-closed", "window-closed"]
    );
    let mut probe = Probe::new(|cx, _| {
        assert_eq!(cx.focused_pane(), None);
        assert!(cx.windows()?.is_empty());
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn a_pane_moves_out_to_a_tab_and_to_a_window_of_its_own() {
    let mut host = FakeHost::new("acme/probe");
    for pane in [1, 2, 3] {
        host.pane(PaneId(pane));
    }
    host.focus(Some(PaneId(1)));
    let mut probe = Probe::new(|cx, _| {
        cx.subscribe(&crate::event::WorkspaceEvent::NAMES)?;
        cx.pane_to_new_tab(PaneId(2), WindowId(1), 5)?;
        cx.pane_to_new_window(PaneId(3))
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.deliver(&mut probe).unwrap();
    assert_eq!(
        heard(&probe),
        [
            "layout-changed",
            "tab-created",
            "pane-moved",
            "layout-changed",
            "window-opened",
            "tab-created",
            "pane-moved",
        ]
    );
    let mut probe = Probe::new(|cx, _| {
        let windows = cx.windows()?;
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].tabs, [TabId(1), TabId(FIRST_ID)]);
        assert_eq!(cx.tabs(WindowId(1))?[1].panes, [PaneId(2)]);
        assert_eq!(cx.tabs(windows[1].id)?[0].panes, [PaneId(3)]);
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn the_writes_apply_before_the_app_s_requests() {
    let mut host = FakeHost::new("acme/probe").with_commands(vec![named("pane.clear")]);
    host.focus(Some(PaneId(1)));
    let mut probe = Probe::new(|cx, _| {
        cx.run_command("pane.clear", Args::new())?;
        cx.send_text(PaneId(1), "ls\n")
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(
        host.take_done(),
        [
            Request::SendText {
                pane: PaneId(1),
                text: "ls\n".to_owned(),
            },
            Request::RunCommand {
                name: "pane.clear".to_owned(),
                args: Args::new(),
            },
        ]
    );
}

#[test]
fn a_read_of_rows_gives_every_row_held_from_the_first_a_blank_one_empty() {
    let mut host = FakeHost::new("acme/probe");
    let pane = host.pane(PaneId(1));
    pane.held = -2..4;
    pane.rows.insert(0, ("first".to_owned(), true));
    pane.rows.insert(3, ("last".to_owned(), false));
    let mut probe = Probe::new(|cx, _| {
        cx.read_lines(PaneId(1), -10, 3)?;
        cx.read_lines(PaneId(1), 2, 100).map(|_| ())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let owed = std::mem::take(&mut host.owed);
    let reads: Vec<Vec<(i64, String, bool)>> = owed
        .iter()
        .map(|event| match event {
            Event::Lines { lines, .. } => lines
                .iter()
                .map(|line| (line.row, line.text.clone(), line.wrapped))
                .collect(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        reads,
        [
            vec![
                (-2, String::new(), false),
                (-1, String::new(), false),
                (0, "first".to_owned(), true),
            ],
            vec![(2, String::new(), false), (3, "last".to_owned(), false)],
        ]
    );
}

/// A plugin whose look decorates, opens a panel, reads rows and opens a
/// tab, as a menu's question should not.
struct Looker;

impl Plugin for Looker {
    fn activate(&mut self, _: &mut Cx<'_>) -> Result<(), Error> {
        Ok(())
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        let look = DecorationLook {
            underline: None,
            underline_color: None,
            background: None,
            pointing: true,
        };
        cx.decorate(PaneId(1), (0, 0), (2, 0), look).is_ok()
            && cx
                .open_panel(
                    "menu",
                    PanelOptions {
                        anchor: Anchor::Pointer(WindowId(1)),
                        role: Role::Menu,
                        close_outside: true,
                        close_escape: true,
                    },
                )
                .is_ok()
            && cx.read_lines(PaneId(1), 0, 1).is_ok()
            && cx.new_tab(WindowId(1), None).is_ok()
    }
}

#[test]
fn a_look_leaves_the_host_as_it_was() {
    let mut host = FakeHost::new("acme/probe");
    host.focus(Some(PaneId(1)));
    assert!(host.applies(&mut Looker, "looker.go", &Args::new()));
    assert!(host.decorations().is_empty());
    assert!(host.panels().is_empty());
    assert!(host.owed.is_empty());
    assert!(host.take_done().is_empty());
    let mut probe = Probe::new(|cx, _| {
        assert_eq!(cx.windows()?[0].tabs, [TabId(1)]);
        // The ids that the look took are given again.
        assert_eq!(cx.new_tab(WindowId(1), None)?.0, TabId(FIRST_ID));
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn a_render_s_tree_stays_and_its_snapshot_tells_its_content() {
    let mut host = FakeHost::new("acme/probe");
    let mut probe = Probe::new(|_, _| Ok(()));
    let mut field = Node::field("Find").key("query").focusable();
    field.tooltip = Some("What to find".to_owned());
    probe.tree = Some(Node::container(vec![
        Node::text("2 matches").role(NodeRole::Status),
        field,
        Node::toggle("case", true).key("case"),
        Node::button("Next").key("next").label("Next match"),
        Node::icon_button("gear").key("settings"),
        Node::spacer(),
        Node::separator(),
        Node::progress(ProgressShape::Bar, ProgressState::Normal, 0.5),
        Node::progress(ProgressShape::Ring, ProgressState::Indeterminate, 0.0),
        Node::list(
            Node::container(vec![Node::text("row")]),
            100,
            (10, Vec::new()),
        )
        .key("results"),
        Node::tabs(
            vec![
                Tab {
                    label: "One".to_owned(),
                    ..Tab::default()
                },
                Tab {
                    label: "Two".to_owned(),
                    ..Tab::default()
                },
            ],
            1,
            Node::scroll(vec![Node::text("page")]),
        ),
    ]));
    let view = ViewId::StatusBar(WindowId(1));
    let tree = host.render(&mut probe, &view).unwrap();
    assert_eq!(host.tree(&view), Some(&tree));
    assert_eq!(
        snapshot(&tree),
        concat!(
            "container\n",
            "  text \"2 matches\" role=Status\n",
            "  field \"Find\" key=query tooltip=\"What to find\" focusable\n",
            "  toggle \"\" icon=\"case\" on key=case\n",
            "  button \"Next\" key=next label=\"Next match\"\n",
            "  button \"\" icon=\"gear\" key=settings\n",
            "  spacer\n",
            "  separator\n",
            "  progress Bar Normal 0.5\n",
            "  progress Ring Indeterminate\n",
            "  list of 100, 0 given from 10 key=results\n",
            "    row:\n",
            "      container\n",
            "        text \"row\"\n",
            "  tabs \"One\" | \"Two\" chosen 1\n",
            "    scroll\n",
            "      text \"page\"\n",
        )
    );
}

/// A command of the name `name`, of no parameters.
fn named(name: &str) -> Command {
    Command {
        name: name.to_owned(),
        title: name.to_owned(),
        category: Category::Panes,
        capabilities: Vec::new(),
        params: Vec::new(),
        when: None,
    }
}

#[test]
fn the_requests_that_the_app_s_host_refuses_the_fake_refuses_alike() {
    let mut host = FakeHost::new("acme/probe")
        .with_slots(vec![Slot::StatusBar])
        .with_commands(vec![named("app.other")]);
    host.focus(Some(PaneId(1)));
    let codes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = std::rc::Rc::clone(&codes);
    let mut probe = Probe::new(move |cx, _| {
        let push = |result: Result<(), Error>| {
            seen.borrow_mut().push(result.err().map(|error| error.code))
        };
        let none = WindowId(9);
        push(cx.new_tab(none, None).map(|_| ()));
        push(cx.close_tab(TabId(9)));
        push(cx.activate_tab(TabId(9)));
        push(cx.move_tab(TabId(1), none, 0));
        push(cx.zoom(TabId(1), Some(PaneId(9))));
        push(cx.close_window(none));
        push(cx.resize_pane(PaneId(1), Edge::Right, 2.0));
        push(cx.set_ratio(TabId(1), Vec::new(), Some(3.0)));
        push(cx.set_font_size(Some(1000.0)));
        push(cx.set_state(Data::Number(f64::NAN)));
        push(cx.emit("probe.ping", "", Data::Nil));
        push(cx.emit("probe.a b", "x/1", Data::Nil));
        push(cx.run_command("app.none", Args::new()));
        push(cx.run_command("app.other", Args::new()));
        let menu = |anchor| PanelOptions {
            anchor,
            role: Role::Menu,
            close_outside: true,
            close_escape: true,
        };
        push(cx.open_panel("", menu(Anchor::Pointer(WindowId(1)))));
        push(cx.open_panel("menu", menu(Anchor::Pointer(none))));
        push(cx.request_render(ViewId::TabBar(WindowId(1))));
        push(cx.request_render(ViewId::StatusBar(none)));
        push(cx.request_render(ViewId::StatusBar(WindowId(1))));
        push(cx.set_text(ViewId::StatusBar(WindowId(1)), "query", "x", 0));
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    use Code::{Failed, InvalidName, NotFound};
    assert_eq!(
        *codes.borrow(),
        [
            Some(NotFound),
            Some(NotFound),
            Some(NotFound),
            Some(NotFound),
            Some(NotFound),
            Some(NotFound),
            Some(Failed),
            Some(Failed),
            Some(Failed),
            Some(Failed),
            Some(InvalidName),
            Some(InvalidName),
            Some(NotFound),
            None,
            Some(InvalidName),
            Some(NotFound),
            Some(NotFound),
            Some(NotFound),
            None,
            Some(NotFound),
        ]
    );
}

#[test]
fn the_declarations_are_checked_as_the_app_s_host_checks_them() {
    let codes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = std::rc::Rc::clone(&codes);
    let mut probe = Probe::new(move |cx, _| {
        let push = |result: Result<(), Error>| {
            seen.borrow_mut().push(result.err().map(|error| error.code))
        };
        let bind = |keys: &str, command: &str, when: Option<&str>| Binding {
            keys: keys.to_owned(),
            command: command.to_owned(),
            args: Args::new(),
            when: when.map(str::to_owned),
        };
        push(cx.bind(bind("cmd+shfit+t", "probe.go", None)));
        push(cx.bind(bind("cmd+t", "probe go", None)));
        push(cx.bind(bind("cmd+t", "probe.go", Some("pane.focused &&"))));
        push(cx.bind(bind("cmd+shift+t", "probe.go", Some("pane.focused"))));
        push(cx.subscribe(&["tab-opened"]));
        push(cx.register_command(named("probe.go")));
        push(cx.register_command(named("Probe.other")));
        Ok(())
    });
    let mut host = FakeHost::new("acme/probe");
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    use Code::{AlreadyRegistered, InvalidBinding, InvalidName};
    assert_eq!(
        *codes.borrow(),
        [
            Some(InvalidBinding),
            Some(InvalidBinding),
            Some(InvalidBinding),
            None,
            Some(InvalidName),
            Some(AlreadyRegistered),
            Some(InvalidName),
        ]
    );
    // A plugin of a manifest that declares its capabilities registers
    // commands of its namespace alone, which no other has, and runs those
    // that its manifest declares, of the capabilities that it holds.
    let mut host = FakeHost::new("acme/probe")
        .with_capabilities(Vec::new())
        .with_uses(vec!["tabs.*".to_owned()])
        .with_commands(vec![named("tabs.new"), named("app.quit")]);
    let mut probe = Probe::new(|cx, _| {
        assert_eq!(
            cx.register_command(named("other.go")).unwrap_err().code,
            Code::InvalidName
        );
        assert!(cx.run_command("tabs.new", Args::new()).is_ok());
        assert!(cx.run_command("probe.go", Args::new()).is_ok());
        assert_eq!(
            cx.run_command("app.quit", Args::new()).unwrap_err().code,
            Code::Undeclared
        );
        Ok(())
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let mut host = FakeHost::new("acme/tabs")
        .with_capabilities(Vec::new())
        .with_commands(vec![named("tabs.new")]);
    let mut probe = Probe::new(|_, _| Ok(()));
    assert_eq!(
        host.activate(&mut probe).unwrap_err().code,
        Code::InvalidName
    );
}

#[test]
fn a_panel_s_opening_asks_its_render_its_close_drops_its_tree_and_a_decoration_outlives_its_pane() {
    let mut host = FakeHost::new("acme/probe");
    host.focus(Some(PaneId(1)));
    let mut probe = Probe::new(|cx, name| {
        let pane = PaneId(1);
        let options = PanelOptions {
            anchor: Anchor::Cursor(pane),
            role: Role::Menu,
            close_outside: true,
            close_escape: true,
        };
        match name {
            "probe.go" => cx.open_panel("menu", options),
            _ => cx.close_panel("menu"),
        }
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let view = ViewId::Panel(PanelId::from("menu"));
    assert_eq!(host.take_renders(), std::slice::from_ref(&view));
    probe.tree = Some(Node::text("menu"));
    host.render(&mut probe, &view);
    assert!(host.tree(&view).is_some());
    host.command(&mut probe, "probe.close", Args::new())
        .unwrap();
    assert!(host.tree(&view).is_none());
    // A decoration of a pane that closed since is the plugin's still.
    let look = DecorationLook {
        underline: None,
        underline_color: None,
        background: None,
        pointing: false,
    };
    let mut probe = Probe::new(move |cx, name| match name {
        "probe.go" => cx.decorate(PaneId(1), (0, 0), (1, 0), look).map(|_| ()),
        "probe.close" => cx.close_pane(PaneId(1)),
        _ => cx.undecorate(PaneId(1), FIRST_ID),
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.command(&mut probe, "probe.close", Args::new())
        .unwrap();
    host.command(&mut probe, "probe.undecorate", Args::new())
        .unwrap();
    assert!(host.decorations().is_empty());
}

#[test]
fn the_config_s_change_reaches_an_active_plugin_alone() {
    let mut host = FakeHost::new("acme/probe");
    let mut probe = Probe::new(|_, _| Ok(()));
    host.set_config(Data::Integer(1));
    host.activate(&mut probe).unwrap();
    host.deliver(&mut probe).unwrap();
    assert!(probe.heard.is_empty());
    host.set_config(Data::Integer(2));
    host.deliver(&mut probe).unwrap();
    assert_eq!(probe.heard, [["config-changed"]]);
}
