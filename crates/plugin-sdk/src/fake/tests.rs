use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::*;
use crate::command::Category;
use crate::host::Edge;
use crate::host::QuickPlace;
use crate::host::Scroll;
use crate::host::SearchQuery;
use crate::ui::{Anchor, Role, Slot};
use crate::view::{ListItem, ProgressShape, ProgressState, Role as NodeRole, Tab};

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
    let mut host = FakeHost::new("acme/probe")
        .with_capabilities(vec![Capability::ClipboardRead, Capability::PaneView]);
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
                takes_keys: false,
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
    host.number_window(WindowId(1), Some(1), Some("⌥⌘1"));
    let mut probe = Probe::new(|cx, _| {
        assert_eq!(cx.focused_window(), Some(WindowId(1)));
        let windows = cx.windows()?;
        assert_eq!(
            windows,
            [WindowInfo {
                id: WindowId(1),
                tabs: vec![TabId(1), TabId(2)],
                active: TabId(1),
                number: Some(1),
                keys: Some("⌥⌘1".to_owned()),
                bell: false,
                quick: false,
                hidden: false,
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
        cx.close_pane(PaneId(3), false)
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let done = host.take_done();
    assert!(matches!(done[0], Request::NewTab { .. }));
    assert_eq!(
        done[1],
        Request::ClosePane {
            pane: PaneId(3),
            ask: false
        }
    );
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
                number: None,
                keys: None,
                bell: false,
                quick: false,
                hidden: false,
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
        cx.close_pane(PaneId(3), false)?;
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
        cx.close_tab(TabId(2), false)
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
        cx.close_pane(PaneId(1), false)
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

#[test]
fn a_read_of_commands_gives_those_that_meet_its_rows_and_the_view_its_screen() {
    let mut host = FakeHost::new("acme/probe");
    let pane = host.pane(PaneId(1));
    pane.held = -10..24;
    let command = |id: u64, prompt: Option<i64>| host::ShellCommand {
        id,
        prompt: prompt.map(|row| (0, row)),
        ..host::ShellCommand::default()
    };
    // A command whose rows all left, its end above those held, meets none;
    // the next one's prompt left the scrollback: it starts at its first row
    // held, its prompt read as none.
    let gone = host::ShellCommand {
        end: Some((0, -25)),
        ..command(4, Some(-30))
    };
    let left = host::ShellCommand {
        output: Some((0, -5)),
        ..command(1, Some(-20))
    };
    pane.commands = vec![gone, left, command(2, Some(3)), command(3, Some(8))];
    let read = Rc::new(RefCell::new(Vec::new()));
    let mut probe = Probe::new({
        let read = Rc::clone(&read);
        move |cx, _| {
            for (top, bottom) in [(-10, -9), (2, 3), (3, 9), (4, 100), (9, 9)] {
                let ids: Vec<u64> = cx
                    .shell_commands(PaneId(1), top, bottom)?
                    .iter()
                    .map(|command| command.id)
                    .collect();
                read.borrow_mut().push(ids);
            }
            let viewport = cx.viewport(PaneId(1))?;
            assert_eq!(
                (
                    viewport.top,
                    viewport.screen,
                    viewport.oldest,
                    viewport.rows
                ),
                (0, 0, -10, 24)
            );
            Ok(())
        }
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(
        *read.borrow(),
        [vec![1], vec![1], vec![2, 3], vec![2, 3], vec![]]
    );
    // A command of no prompt marked starts at its first mark.
    host.pane(PaneId(1)).commands = vec![host::ShellCommand {
        id: 5,
        output: Some((0, 6)),
        ..host::ShellCommand::default()
    }];
    let found = Rc::new(RefCell::new(Vec::new()));
    let mut probe = Probe::new({
        let found = Rc::clone(&found);
        move |cx, _| {
            for (top, bottom) in [(0, 6), (0, 7)] {
                found
                    .borrow_mut()
                    .push(cx.shell_commands(PaneId(1), top, bottom)?);
            }
            Ok(())
        }
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let found = found.borrow();
    assert!(found[0].is_empty());
    assert_eq!(found[1][0].id, 5);
    assert_eq!(found[1][0].prompt, None);
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
                        takes_keys: false,
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
            (
                10,
                vec![
                    ListItem {
                        texts: vec!["ten".to_owned()],
                    },
                    ListItem {
                        texts: vec!["eleven".to_owned(), "11".to_owned()],
                    },
                ],
            ),
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
            "  list of 100, 2 given from 10 key=results\n",
            "    item 10 \"ten\"\n",
            "    item 11 \"eleven\" | \"11\"\n",
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
            seen.borrow_mut().push(result.err().map(|error| error.code));
        };
        let none = WindowId(9);
        push(cx.new_tab(none, None).map(|_| ()));
        push(cx.close_tab(TabId(9), false));
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
            takes_keys: false,
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
    use Code::{Failed, InvalidName, NotFound, WrongKind};
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
            Some(WrongKind),
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
            seen.borrow_mut().push(result.err().map(|error| error.code));
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
            takes_keys: false,
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
        "probe.close" => cx.close_pane(PaneId(1), false),
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

#[test]
fn a_value_nested_too_deep_is_refused_as_the_app_s_host_refuses_it() {
    let mut host = FakeHost::new("acme/probe");
    let nested = |depth: usize| {
        let mut value = Data::Nil;
        for _ in 0..depth {
            value = Data::List(vec![value]);
        }
        value
    };
    let mut probe = Probe::new(move |cx, _| {
        cx.set_state(nested(Data::DEEPEST))?;
        assert_eq!(
            cx.set_state(nested(1_000_000)).unwrap_err().code,
            Code::Failed
        );
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn a_quick_window_opens_of_a_pane_with_the_focus_and_shows_and_hides() {
    let mut host = FakeHost::new("lokyr/probe");
    let opened = std::rc::Rc::new(std::cell::Cell::new(None));
    let kept = std::rc::Rc::clone(&opened);
    let mut probe = Probe::new(move |cx, _| {
        match kept.get() {
            None => kept.set(Some(cx.open_quick_window(QuickPlace::default(), None)?)),
            Some(window) => {
                let hidden = cx
                    .windows()?
                    .into_iter()
                    .find(|info| info.id == window)
                    .is_some_and(|info| info.hidden);
                let place = hidden.then(QuickPlace::default);
                cx.show_quick_window(window, place)?;
            }
        }
        Ok(())
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let window = opened.get().unwrap();
    let info = |host: &mut FakeHost| {
        let mut found = None;
        host.call(true, |cx| {
            found = cx.windows()?.into_iter().find(|info| info.id == window);
            Ok(())
        })
        .unwrap();
        found.unwrap()
    };
    let shown = info(&mut host);
    assert!(shown.quick && !shown.hidden);
    assert_eq!(
        host.keyboard().map(|pane| host.pane(pane).window),
        Some(window)
    );
    // Hidden, the focus goes; shown again, it comes back.
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert!(info(&mut host).hidden);
    assert_eq!(host.keyboard(), None);
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert!(!info(&mut host).hidden);
    // A window that the plugin did not open as a quick one is none.
    let error = host
        .call(true, |cx| cx.show_quick_window(WindowId(99), None))
        .unwrap_err();
    assert_eq!(error.code, Code::NotFound);
}

#[test]
fn a_host_of_a_manifest_holds_what_it_declares_but_the_network_s_and_the_processes() {
    let manifest = concat!(
        "# A plugin's manifest.\n[plugin]\nname = \"acme/probe\"\n",
        "[capabilities]\npane-view = true\nclipboard-read = false\n",
        "net = [\"lokyr.dev\"]\nprocess-spawn = true\n",
        "env-read = [\n  \"AWS_*\",\n  \"HOME\",\n]\n",
        "[uses]\ncommands = [\"tabs.*\"]\n[contributes]\nslots = [\"tab-bar\"]\n",
    );
    let mut host = FakeHost::new("acme/probe").with_manifest(manifest);
    host.pane(PaneId(1));
    let codes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = std::rc::Rc::clone(&codes);
    let mut probe = Probe::new(move |cx, _| {
        let code = |done: Result<(), Error>| done.err().map(|error| error.code);
        let mut seen = seen.borrow_mut();
        seen.push(code(cx.scroll(PaneId(1), Scroll::Top)));
        seen.push(code(cx.clipboard_text().map(drop)));
        seen.push(code(cx.select_all(PaneId(1))));
        seen.push(code(cx.quit()));
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(
        *codes.borrow(),
        [
            None,
            Some(Code::MissingCapability),
            None,
            Some(Code::MissingCapability)
        ]
    );
    assert_eq!(
        super::declared(manifest),
        (
            vec![
                Capability::PaneView,
                Capability::Net("lokyr.dev".to_owned()),
                Capability::ProcessSpawn,
                Capability::EnvRead("AWS_*".to_owned()),
                Capability::EnvRead("HOME".to_owned()),
            ],
            Some(vec!["tabs.*".to_owned()])
        )
    );
}

#[test]
fn a_declared_plugin_s_keys_run_what_it_may_run_as_the_app_s_host_checks_them() {
    let send = Command {
        name: "pane.send-text".to_owned(),
        title: "Send Text".to_owned(),
        category: Category::Input,
        capabilities: vec![Capability::PtyWrite],
        params: vec![crate::command::Param {
            name: "text".to_owned(),
            kind: crate::command::Kind::String,
            default: None,
        }],
        when: None,
    };
    let text = Args::from([(
        "text".to_owned(),
        crate::command::Value::String("rm -rf ~\n".to_owned()),
    )]);
    let bind = |host: FakeHost, keys: &str, uses: &[&str]| {
        let mut host = host
            .with_commands(vec![send.clone()])
            .with_uses(uses.iter().map(|name| (*name).to_owned()).collect());
        let keys = keys.to_owned();
        let args = text.clone();
        let mut probe = Probe::new(move |cx, _| {
            cx.bind(Binding {
                keys: keys.clone(),
                command: "pane.send-text".to_owned(),
                args: args.clone(),
                when: None,
            })
        });
        host.command(&mut probe, "probe.go", Args::new())
            .err()
            .map(|error| error.code)
    };
    let declared = || FakeHost::new("acme/probe").with_capabilities(vec![Capability::GlobalHotkey]);
    // A global key that types into the shell, of a command that it does not
    // declare, or of one whose capability it lacks.
    assert_eq!(
        bind(declared(), "global:cmd+c", &[]),
        Some(Code::Undeclared)
    );
    assert_eq!(
        bind(declared(), "global:cmd+c", &["pane.send-text"]),
        Some(Code::MissingCapability)
    );
    assert_eq!(bind(declared(), "ctrl+t", &[]), Some(Code::Undeclared));
    // Of `pty-write` and the command declared, it binds it; and a
    // first-party plugin binds any.
    let typing = FakeHost::new("acme/probe")
        .with_capabilities(vec![Capability::GlobalHotkey, Capability::PtyWrite]);
    assert_eq!(bind(typing, "global:cmd+c", &["pane.send-text"]), None);
    let mut first_party = FakeHost::new("lokyr/probe").with_commands(vec![send.clone()]);
    let mut probe = Probe::new(|cx, _| {
        cx.bind(Binding {
            keys: "ctrl+t".to_owned(),
            command: "pane.send-text".to_owned(),
            args: Args::new(),
            when: None,
        })
    });
    assert!(
        first_party
            .command(&mut probe, "probe.go", Args::new())
            .is_ok()
    );
}

#[test]
fn a_call_s_writes_are_bounded_as_the_app_s_host_bounds_them() {
    let mut host = FakeHost::new("acme/probe");
    host.pane(PaneId(1));
    let codes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let seen = std::rc::Rc::clone(&codes);
    let mut probe = Probe::new(move |cx, _| {
        let mut seen = seen.borrow_mut();
        for _ in 0..=host::MOST_WRITES {
            seen.push(
                cx.scroll(PaneId(1), Scroll::Top)
                    .err()
                    .map(|error| error.code),
            );
        }
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let codes = codes.borrow();
    assert!(codes[..host::MOST_WRITES].iter().all(Option::is_none));
    assert_eq!(codes[host::MOST_WRITES], Some(Code::Failed));
}

#[test]
fn a_call_that_fails_drops_what_it_asked_and_a_look_declares_nothing() {
    let mut host = FakeHost::new("acme/probe");
    host.pane(PaneId(1));
    let mut failing = Probe::new(|cx, _| {
        cx.scroll(PaneId(1), Scroll::Top)?;
        cx.open("https://lokyr.dev")?;
        Err(Error::new(Code::Failed, "after its writes"))
    });
    let failed = host.command(&mut failing, "probe.go", Args::new());
    assert!(failed.is_err_and(|error| error.code == Code::Failed));
    assert!(host.take_done().is_empty());
    // A look's declarations fail, and none stays.
    let mut binder = Binder::default();
    host.activate(&mut binder).unwrap();
    assert!(host.applies(&mut binder, "binder.go", &Args::new()));
    assert_eq!(binder.tried, [Some(Code::Failed)]);
    assert!(host.bindings().is_empty());
}

/// A plugin whose `applies` tries to bind a key, as `tried` keeps.
#[derive(Default)]
struct Binder {
    tried: Vec<Option<Code>>,
}

impl Plugin for Binder {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "binder.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        let tried = cx.bind(Binding {
            keys: "ctrl+l".to_owned(),
            command: "binder.go".to_owned(),
            args: Args::new(),
            when: None,
        });
        self.tried.push(tried.err().map(|error| error.code));
        true
    }
}

#[test]
fn a_window_s_point_of_no_number_is_refused_as_the_app_s_host_refuses_it() {
    let mut host = FakeHost::new("acme/probe");
    host.focus(Some(PaneId(1)));
    let mut probe = Probe::new(|cx, _| {
        let window = cx.focused_window().unwrap();
        let tab = cx.tabs(window)?[0].id;
        cx.tab_to_new_window_at(
            tab,
            host::ScreenPoint {
                x: f64::NAN,
                y: 0.0,
            },
        )
    });
    let refused = host
        .command(&mut probe, "probe.go", Args::new())
        .unwrap_err();
    assert_eq!(refused.code, Code::WrongKind);
    assert!(refused.message.contains("finite"), "{refused}");
}

/// A plugin whose `closer.go` opens its panel `p`, whose other commands
/// close it and fail, and whose look closes it.
struct Closer;

impl Plugin for Closer {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        for name in ["closer.go", "closer.fails"] {
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

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        if name == "closer.go" {
            return cx.open_panel(
                "p",
                PanelOptions {
                    anchor: Anchor::Pointer(WindowId(1)),
                    role: Role::Dialog,
                    close_outside: false,
                    close_escape: true,
                    takes_keys: false,
                },
            );
        }
        cx.close_panel("p")?;
        Err(Error::new(Code::Failed, "on purpose"))
    }

    fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
        cx.close_panel("p").is_ok()
    }

    fn render(&mut self, _: &mut Cx<'_>, _: &ViewId) -> Option<Node> {
        Some(Node::text("p"))
    }
}

#[test]
fn a_failed_call_or_a_look_that_closes_a_panel_leaves_it_open_of_its_tree() {
    let mut host = FakeHost::new("acme/closer");
    // A pane of the window 1.
    host.pane(PaneId(1));
    let mut closer = Closer;
    host.activate(&mut closer).unwrap();
    host.command(&mut closer, "closer.go", Args::new()).unwrap();
    let panel = ViewId::Panel(PanelId::from("p"));
    host.render(&mut closer, &panel);
    assert!(host.tree(&panel).is_some());
    let failed = host.command(&mut closer, "closer.fails", Args::new());
    assert_eq!(failed.unwrap_err().code, Code::Failed);
    assert!(host.applies(&mut closer, "closer.go", &Args::new()));
    assert!(host.panels().contains_key(&PanelId::from("p")));
    assert!(host.tree(&panel).is_some());
    assert!(host.owed.is_empty(), "{:?}", host.owed);
}

#[test]
fn the_fake_host_s_timers_fire_as_its_time_moves_on_once_and_repeating() {
    let mut host = FakeHost::new("acme/probe");
    let ids = Rc::new(RefCell::new(Vec::new()));
    let (kept, asks) = (Rc::clone(&ids), Rc::new(RefCell::new(0)));
    let mut probe = Probe::new(move |cx, _| {
        let mut asked = asks.borrow_mut();
        *asked += 1;
        match *asked {
            1 => {
                let once = cx.start_timer(std::time::Duration::from_millis(100), false)?;
                let again = cx.start_timer(std::time::Duration::from_millis(40), true)?;
                let soon = cx.start_timer(std::time::Duration::ZERO, false)?;
                kept.borrow_mut().extend([once, again, soon]);
                Ok(())
            }
            // A call that cancels one, starts another and fails: the timers
            // stay as they were.
            2 => {
                cx.cancel_timer(kept.borrow()[1])?;
                cx.start_timer(std::time::Duration::from_millis(10), true)?;
                Err(Error::new(Code::Failed, "after its timers"))
            }
            _ => cx.cancel_timer(kept.borrow()[1]),
        }
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let [once, again, soon] = ids.borrow()[..] else {
        panic!("three timers");
    };
    assert_eq!(host.timers(), [once, again, soon]);
    let wait = |host: &mut FakeHost, probe: &mut Probe, by: u64| {
        host.advance(std::time::Duration::from_millis(by));
        host.deliver(probe).unwrap();
        std::mem::take(&mut probe.heard)
    };
    // The one of no delay at the least, the one that repeats at each delay,
    // their events to the plugin alone, which subscribed to none.
    let timer = || vec!["timer".to_owned()];
    assert_eq!(wait(&mut host, &mut probe, host::LEAST_DELAY), [timer()]);
    assert_eq!(host.timers(), [once, again]);
    assert!(wait(&mut host, &mut probe, 29).is_empty());
    assert_eq!(wait(&mut host, &mut probe, 1), [timer()]);
    // At 140 ms the one of 100 ms, and the one that repeats once.
    assert_eq!(
        wait(&mut host, &mut probe, 100),
        [vec!["timer".to_owned(); 2]]
    );
    assert_eq!(host.timers(), [again]);
    let failed = host.command(&mut probe, "probe.go", Args::new());
    assert_eq!(failed.unwrap_err().code, Code::Failed);
    assert_eq!(host.timers(), [again]);
    // At 180 ms it fires; cancelled, the event that it owes goes too.
    host.advance(std::time::Duration::from_millis(40));
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.deliver(&mut probe).unwrap();
    assert!(probe.heard.is_empty());
    assert!(host.timers().is_empty());
}

#[test]
fn the_fake_host_bounds_the_timers_and_starts_none_in_a_look() {
    struct Looking;
    impl Plugin for Looking {
        fn activate(&mut self, _: &mut Cx<'_>) -> Result<(), Error> {
            Ok(())
        }

        fn on_command(&mut self, cx: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
            let day = std::time::Duration::from_millis(host::MOST_DELAY);
            let past = cx.start_timer(day + std::time::Duration::from_millis(1), false);
            assert_eq!(past, Err(Error::new(Code::Failed, DELAY_RANGE)));
            for _ in 0..host::MOST_TIMERS {
                cx.start_timer(day, true)?;
            }
            let more = cx.start_timer(day, true);
            assert_eq!(more, Err(Error::new(Code::Unavailable, TIMERS_BOUND)));
            Ok(())
        }

        fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
            cx.start_timer(std::time::Duration::from_millis(10), false)
                .is_ok()
        }
    }
    let mut host = FakeHost::new("acme/looking");
    let mut plugin = Looking;
    host.activate(&mut plugin).unwrap();
    assert!(!host.applies(&mut plugin, "looking.go", &Args::new()));
    assert!(host.timers().is_empty());
    host.command(&mut plugin, "looking.go", Args::new())
        .unwrap();
    assert_eq!(host.timers().len(), host::MOST_TIMERS);
}

#[test]
fn a_look_starts_no_search_and_a_failed_call_s_search_ends_the_one_before_running_on() {
    let mut host = FakeHost::new("acme/probe");
    host.pane(PaneId(1))
        .rows
        .insert(0, ("find me".to_owned(), false));
    host.focus(Some(PaneId(1)));
    let query = || SearchQuery {
        text: "find".to_owned(),
        ..SearchQuery::default()
    };
    let started = Rc::new(RefCell::new(Vec::new()));
    let seen = Rc::clone(&started);
    let mut probe = Probe::new(move |cx, _| {
        let id = cx.search(PaneId(1), query())?;
        seen.borrow_mut().push(id);
        Ok(())
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let first = started.borrow()[0];
    assert_eq!(host.searches(), [first]);
    // A call that fails: its search ends, of no event, and the one that it
    // ended runs on.
    let mut failing = Probe::new(move |cx, _| {
        cx.search(PaneId(1), query())?;
        Err(Error::new(Code::Failed, "on purpose"))
    });
    host.command(&mut failing, "probe.go", Args::new())
        .unwrap_err();
    assert_eq!(host.searches(), [first]);
    // A look starts none, and ends none.
    struct Looking(Option<Code>, Option<Code>);
    impl Plugin for Looking {
        fn applies(&mut self, cx: &mut Cx<'_>, _: &str, _: &Args) -> bool {
            self.0 = cx
                .search(PaneId(1), SearchQuery::default())
                .err()
                .map(|error| error.code);
            self.1 = cx.end_search(1).err().map(|error| error.code);
            true
        }
    }
    let mut looking = Looking(None, None);
    assert!(host.applies(&mut looking, "probe.go", &Args::new()));
    assert_eq!(
        (looking.0, looking.1),
        (Some(Code::Failed), Some(Code::Failed))
    );
    assert_eq!(host.searches(), [first]);
}

#[test]
fn a_declaration_made_already_changes_nothing_and_a_plugin_s_are_bounded() {
    let mut host = FakeHost::new("acme/probe");
    let mut probe = Probe::new(|cx, _| {
        for _ in 0..2 {
            cx.bind(Binding {
                keys: "cmd+k".to_owned(),
                command: "probe.go".to_owned(),
                args: Args::new(),
                when: None,
            })?;
        }
        Ok(())
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(host.bindings().len(), 1);
    // Past the most, refused.
    let mut many = Probe::new(|cx, _| {
        for at in 0..host::MOST_DECLARED {
            cx.bind(Binding {
                keys: format!("cmd+{}", ["a", "b", "c", "d"][at % 4]),
                command: format!("probe.go{at}"),
                args: Args::new(),
                when: None,
            })?;
        }
        Ok(())
    });
    let error = host
        .command(&mut many, "probe.go", Args::new())
        .unwrap_err();
    assert!(
        error
            .message
            .contains("4096 commands, keys and menu items at most"),
        "{error:?}"
    );
}

/// A plugin that hears the tabs' events, by their names, and runs the
/// requests that `asks` makes as its command runs.
struct TabListener {
    heard: Rc<RefCell<Vec<String>>>,
    asks: Callback,
}

impl Plugin for TabListener {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.register_command(Command {
            name: "listener.go".to_owned(),
            title: "Go".to_owned(),
            category: Category::View,
            capabilities: Vec::new(),
            params: Vec::new(),
            when: None,
        })?;
        cx.subscribe(&["tab-changed", "tab-activated"])
    }

    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _: Args) -> Result<(), Error> {
        (self.asks)(cx, name)
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.heard
            .borrow_mut()
            .extend(events.iter().map(|event| event.name().to_owned()));
        Ok(())
    }
}

/// The listener of `asks`, and what it hears.
fn listener(
    asks: impl FnMut(&mut Cx<'_>, &str) -> Result<(), Error> + 'static,
) -> (TabListener, Rc<RefCell<Vec<String>>>) {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let plugin = TabListener {
        heard: Rc::clone(&heard),
        asks: Box::new(asks),
    };
    (plugin, heard)
}

#[test]
fn a_tab_shown_loses_its_activity_and_its_bell_and_takes_its_pane_s_look() {
    let mut host = FakeHost::new("acme/listener");
    host.pane(PaneId(1)).title = Some("one".to_owned());
    let second = host.pane(PaneId(2));
    second.tab = TabId(2);
    second.title = Some("two".to_owned());
    host.focus(Some(PaneId(1)));
    let (mut plugin, heard) = listener(|cx, _| cx.activate_tab(TabId(2)));
    host.activate(&mut plugin).unwrap();
    host.set_tab_marks(
        TabId(2),
        TabMarks {
            activity: true,
            bell: true,
            ..TabMarks::default()
        },
    );
    host.deliver(&mut plugin).unwrap();
    heard.borrow_mut().clear();
    host.command(&mut plugin, "listener.go", Args::new())
        .unwrap();
    host.deliver(&mut plugin).unwrap();
    assert_eq!(*heard.borrow(), ["tab-activated", "tab-changed"]);
    let mut probe = Probe::new(|cx, _| {
        let tabs = cx.tabs(WindowId(1))?;
        assert_eq!(tabs[1].title, "two");
        assert!(!tabs[1].activity && !tabs[1].bell);
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn a_plugin_that_does_not_read_the_processes_reads_no_program_s_marks_nor_place() {
    let mut host = FakeHost::new("acme/probe").with_capabilities(vec![Capability::PaneView]);
    let pane = host.pane(PaneId(1));
    pane.directory = Some("/Users/me/work".to_owned());
    host.focus(Some(PaneId(1)));
    host.keyboard();
    host.set_tab_marks(
        TabId(1),
        TabMarks {
            running: true,
            alternate: true,
            printing: true,
            ..TabMarks::default()
        },
    );
    let mut probe = Probe::new(|cx, _| {
        let tab = &cx.tabs(WindowId(1))?[0];
        assert!(!tab.running && !tab.alternate && !tab.printing);
        assert_eq!(cx.pane_title(PaneId(1))?, None);
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    // One that reads them reads its marks, and its folder for a title.
    let mut host = FakeHost::new("acme/probe");
    host.pane(PaneId(1)).directory = Some("/Users/me/work".to_owned());
    host.focus(Some(PaneId(1)));
    host.keyboard();
    host.set_tab_marks(
        TabId(1),
        TabMarks {
            running: true,
            ..TabMarks::default()
        },
    );
    let mut probe = Probe::new(|cx, _| {
        assert!(cx.tabs(WindowId(1))?[0].running);
        assert_eq!(cx.pane_title(PaneId(1))?, Some("work".to_owned()));
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn a_swap_takes_a_pane_that_the_call_opened_and_holds_no_pane_twice() {
    let mut host = FakeHost::new("acme/probe");
    host.pane(PaneId(1));
    host.focus(Some(PaneId(1)));
    let mut probe = Probe::new(|cx, _| {
        let opened = cx.split(PaneId(1), Direction::Vertical, Side::Second)?;
        cx.swap_panes(PaneId(1), opened)
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let layout = host.tab_layout(TabId(1)).unwrap();
    let held = leaves(&layout);
    assert_eq!(held.len(), 2);
    assert_eq!(held[1], PaneId(1));
    // A pane moved out of its tab earlier in the call is swapped of none.
    let mut probe = Probe::new(|cx, _| {
        let other = cx.layout(TabId(1))?;
        let panes = leaves(&other);
        cx.pane_to_new_window(panes[0])?;
        cx.swap_panes(panes[0], panes[1])
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let held = leaves(&host.tab_layout(TabId(1)).unwrap());
    assert_eq!(held, [PaneId(1)]);
    // Every pane in one tab alone.
    let mut probe = Probe::new(|cx, _| {
        let mut held = Vec::new();
        for window in cx.windows()? {
            for tab in window.tabs {
                held.extend(leaves(&cx.layout(tab)?));
            }
        }
        let mut unique = held.clone();
        unique.sort_by_key(|pane| pane.0);
        unique.dedup();
        assert_eq!(unique.len(), held.len(), "{held:?}");
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
}

#[test]
fn a_tab_into_a_pane_tells_its_tab_s_change_of_the_moved_pane_s_title() {
    let mut host = FakeHost::new("acme/listener");
    host.pane(PaneId(1)).title = Some("one".to_owned());
    let second = host.pane(PaneId(2));
    second.tab = TabId(2);
    second.title = Some("two".to_owned());
    host.focus(Some(PaneId(1)));
    let (mut plugin, heard) = listener(|cx, _| cx.tab_into_pane(TabId(2), PaneId(1), Edge::Right));
    host.activate(&mut plugin).unwrap();
    host.command(&mut plugin, "listener.go", Args::new())
        .unwrap();
    host.deliver(&mut plugin).unwrap();
    assert!(
        heard.borrow().contains(&"tab-changed".to_owned()),
        "{:?}",
        heard.borrow()
    );
}

/// A plugin that hears the panes' changes and closes, by their panes.
struct PaneListener(Rc<RefCell<Vec<String>>>);

impl Plugin for PaneListener {
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        cx.subscribe(&["pane-changed", "pane-closed"])
    }

    fn on_command(&mut self, _: &mut Cx<'_>, _: &str, _: Args) -> Result<(), Error> {
        Ok(())
    }

    fn on_event(&mut self, _: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.0
            .borrow_mut()
            .extend(events.iter().map(|event| match event {
                Event::Workspace(WorkspaceEvent::PaneChanged { pane, .. }) => {
                    format!("pane-changed {}", pane.0)
                }
                event => event.name().to_owned(),
            }));
        Ok(())
    }
}

#[test]
fn a_pane_s_change_that_a_plugin_cannot_read_tells_it_nothing_as_the_app_s_host() {
    // Of no `process-read`, as a third-party plugin that does not declare
    // it: a change of a pane's folder or program, whose title it does not
    // read, tells nothing; of its program's title, it does.
    let mut host = FakeHost::new("acme/listener").with_manifest(
        "[plugin]\nname = \"acme/listener\"\n[capabilities]\nnotifications = true\n",
    );
    host.pane(PaneId(1)).title = Some("vim".to_owned());
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut plugin = PaneListener(Rc::clone(&heard));
    host.activate(&mut plugin).unwrap();
    let changed = || {
        Event::Workspace(WorkspaceEvent::PaneChanged {
            pane: PaneId(1),
            tab: TabId(1),
        })
    };
    host.post(changed());
    host.deliver(&mut plugin).unwrap();
    assert_eq!(*heard.borrow(), ["pane-changed 1"]);
    host.pane(PaneId(1)).directory = Some("/tmp".to_owned());
    host.post(changed());
    host.deliver(&mut plugin).unwrap();
    assert_eq!(*heard.borrow(), ["pane-changed 1"]);
    host.pane(PaneId(1)).title = Some("htop".to_owned());
    host.post(changed());
    host.deliver(&mut plugin).unwrap();
    assert_eq!(*heard.borrow(), ["pane-changed 1", "pane-changed 1"]);
    // Of `process-read`, every change.
    let mut reading = FakeHost::new("acme/listener");
    reading.pane(PaneId(1)).title = Some("vim".to_owned());
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut plugin = PaneListener(Rc::clone(&heard));
    reading.activate(&mut plugin).unwrap();
    for _ in 0..2 {
        reading.post(changed());
        reading.deliver(&mut plugin).unwrap();
    }
    assert_eq!(*heard.borrow(), ["pane-changed 1", "pane-changed 1"]);
}

#[test]
#[should_panic(expected = "a timer's event is the host's")]
fn a_timer_s_event_is_the_host_s_and_no_post_gives_it() {
    let mut host = FakeHost::new("acme/probe");
    host.post(Event::Timer { id: 1 });
}

#[test]
fn a_pane_s_cells_read_of_its_view_s_rows_and_its_selection_s_as_the_test_gives_them() {
    use crate::image::CellRow;
    let mut host = FakeHost::new("acme/probe");
    host.focus(Some(PaneId(1)));
    let bold = StyleRun {
        first: 0,
        last: 1,
        style: crate::host::TextStyle {
            bold: true,
            ..crate::host::TextStyle::default()
        },
    };
    let pane = host.pane(PaneId(1));
    pane.grid = (80, 2);
    pane.held = 0..2;
    pane.rows.insert(0, ("ls -l   ".to_owned(), false));
    pane.styles.insert(0, vec![bold]);
    let selected = vec![CellRow {
        text: "-l".to_owned(),
        runs: Vec::new(),
    }];
    let read = Rc::new(RefCell::new(Vec::new()));
    let kept = Rc::clone(&read);
    let mut probe = Probe::new(move |cx, _| {
        kept.borrow_mut()
            .push((cx.screen_cells(PaneId(1))?, cx.selection_cells(PaneId(1))?));
        Ok(())
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    host.pane(PaneId(1)).selected_cells = Some(selected.clone());
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let read = read.borrow();
    let row = |text: &str, runs: Vec<StyleRun>| CellRow {
        text: text.to_owned(),
        runs,
    };
    assert_eq!(read[0].0, [row("ls -l", vec![bold]), row("", Vec::new())]);
    assert_eq!(read[0].1, None);
    assert_eq!(read[1].1, Some(selected));
    // A deselection takes the cells away, as the app's.
    let mut deselect = Probe::new(|cx, _| cx.deselect(PaneId(1)));
    host.command(&mut deselect, "probe.go", Args::new())
        .unwrap();
    assert_eq!(host.pane(PaneId(1)).selected_cells, None);
    // Without `terminal-read`, refused.
    let mut host = FakeHost::new("acme/probe").with_capabilities(Vec::new());
    host.pane(PaneId(1));
    let mut probe = Probe::new(|cx, _| cx.screen_cells(PaneId(1)).map(drop));
    let refused = host.command(&mut probe, "probe.go", Args::new());
    assert_eq!(refused.unwrap_err().code, Code::MissingCapability);
}

#[test]
fn a_png_on_the_clipboard_takes_its_text_s_place_and_what_is_none_is_refused() {
    let mut host = FakeHost::new("acme/probe");
    host.set_clipboard(Some("old"));
    let png = crate::image::Image::blank(1.0).png.0;
    let given = png.clone();
    let mut probe = Probe::new(move |cx, name| match name {
        "probe.go" => cx.set_clipboard_image(given.clone()),
        _ => Ok(()),
    });
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(host.clipboard_image(), Some(png.as_slice()));
    assert_eq!(host.clipboard(), None);
    let mut text = Probe::new(|cx, _| cx.set_clipboard_text("new"));
    host.command(&mut text, "probe.go", Args::new()).unwrap();
    assert_eq!(
        (host.clipboard(), host.clipboard_image()),
        (Some("new"), None)
    );
    let mut gif = Probe::new(|cx, _| cx.set_clipboard_image(b"GIF89a".to_vec()));
    let refused = host.command(&mut gif, "probe.go", Args::new()).unwrap_err();
    assert!(refused.message.contains("PNG"), "{refused:?}");
}

#[test]
fn a_save_shows_its_panel_one_at_a_time_of_a_window_and_its_end_is_heard() {
    let mut host = FakeHost::new("acme/probe").with_capabilities(Vec::new());
    host.focus(Some(PaneId(1)));
    let window = host.pane(PaneId(1)).window;
    let ids = Rc::new(RefCell::new(Vec::new()));
    let kept = Rc::clone(&ids);
    let mut probe = Probe::new(move |cx, _| {
        let id = cx.save_file(None, "Lokyr.png", b"bytes".to_vec());
        let again = cx.save_file(Some(window), "again.png", b"x".to_vec());
        kept.borrow_mut()
            .push((id, again.map_err(|error| error.message)));
        Ok(())
    });
    host.activate(&mut probe).unwrap();
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let (id, again) = ids.borrow_mut().remove(0);
    let id = id.unwrap();
    assert_eq!(again, Err(crate::message::SAVING.to_owned()));
    assert_eq!(host.saves(), [(id, window, "Lokyr.png".to_owned())]);
    // While its panel shows, the next call's is refused too.
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    assert_eq!(
        ids.borrow_mut().remove(0).0.unwrap_err().message,
        crate::message::SAVING
    );
    // The user chooses a place: the file holds the bytes, and the plugin
    // hears of it; then a new save shows, which the user cancels.
    host.answer_save(id, Some("/Users/me/Pictures/Lokyr.png"));
    assert_eq!(
        host.saved_file("/Users/me/Pictures/Lokyr.png"),
        Some(&b"bytes"[..])
    );
    host.deliver(&mut probe).unwrap();
    assert_eq!(probe.heard.last().unwrap(), &["saved".to_owned()]);
    host.command(&mut probe, "probe.go", Args::new()).unwrap();
    let (next, _) = ids.borrow_mut().remove(0);
    host.answer_save(next.unwrap(), None);
    assert!(host.saves().is_empty());
    // A name of a `/`, refused; a call that fails shows no panel.
    let mut wrong = Probe::new(|cx, _| cx.save_file(None, "a/b.png", Vec::new()).map(drop));
    assert!(host.command(&mut wrong, "probe.go", Args::new()).is_err());
    let mut failing = Probe::new(|cx, _| {
        cx.save_file(None, "x.png", Vec::new())?;
        Err(Error::new(Code::Failed, "on purpose"))
    });
    assert!(host.command(&mut failing, "probe.go", Args::new()).is_err());
    assert!(host.saves().is_empty());
}

#[test]
fn a_kept_image_s_id_is_kept_four_at_most_and_let_go_as_the_plugin_asks() {
    let mut host = FakeHost::new("acme/probe").with_capabilities(Vec::new());
    let ids = Rc::new(RefCell::new(Vec::new()));
    let kept = Rc::clone(&ids);
    let mut probe = Probe::new(move |cx, _| {
        let image = cx.render_image(crate::image::ImageSpec {
            rows: vec![crate::image::CellRow::default()],
            keep: true,
            ..crate::image::ImageSpec::default()
        })?;
        kept.borrow_mut().push(image.id.unwrap());
        Ok(())
    });
    for _ in 0..5 {
        host.command(&mut probe, "probe.go", Args::new()).unwrap();
    }
    let ids = ids.borrow().clone();
    assert_eq!(host.kept_images(), &ids[1..]);
    let last = ids[4];
    let mut dropper = Probe::new(move |cx, _| cx.drop_image(last));
    host.command(&mut dropper, "probe.go", Args::new()).unwrap();
    assert_eq!(host.kept_images(), &ids[1..4]);
    // Not kept, of no id.
    let mut plain = Probe::new(|cx, _| {
        let image = cx.render_image(crate::image::ImageSpec {
            rows: vec![crate::image::CellRow::default()],
            ..crate::image::ImageSpec::default()
        })?;
        assert_eq!(image.id, None);
        Ok(())
    });
    host.command(&mut plain, "probe.go", Args::new()).unwrap();
    // A node of one, as a snapshot writes it.
    assert_eq!(snapshot(&Node::image(7)), "image 7\n");
}
