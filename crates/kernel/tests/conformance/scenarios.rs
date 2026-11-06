//! The scenarios of the conformance suite, of every request of the plugin
//! API, its failures among them: those that the first-party plugins make,
//! `links`', the reads of a line, its link, its directory, what opens, the
//! decorations and the pointer's events, and `core-actions`', the text's
//! size, the clipboard and the writes to a pane; and the events, the
//! panels, the capabilities, the declarations, the app's requests, the
//! state, the reads of rows, the views and the workspace's writes that
//! every plugin meets. Each runs in the world of the suite: a window of a
//! pane whose program, in `/`, wrote [`LINE`], the focused one, and a
//! clipboard of [`CLIPBOARD`]; the kernel's host runs them in their order,
//! in one world, the workspace's leaving it as it found it, and
//! `core-actions`' last, as its writes change the pane.
//!
//! A window, a tab or a pane that a scenario's requests open, whose id
//! each host gives its own, is a symbol, [`window`], [`tab`] and [`pane`]
//! of the order in which the trace first meets it: the test plugin asks
//! with the host's id of a symbol, and its trace tells each id that it
//! meets by its symbol.

use std::collections::BTreeMap;

use lokyr_plugin_sdk::command::{Args, Capability, Category, Code, Command, Error, Value};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{
    BarPlace, Binding, Color, DecorationLook, Direction, Edge, Layout, Line, LineText, MOST_STATE,
    PaneId, Scroll, Side, TabId, TabInfo, Underline, WindowId, WindowInfo,
};
use lokyr_plugin_sdk::message::{Data, Event, Mods, Request, Response};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role, ViewId};
use serde::{Deserialize, Serialize};

/// The line that the world's pane's program wrote.
pub const LINE: &str = "see https://lokyr.dev/x now";

/// The clipboard's text as the world starts.
pub const CLIPBOARD: &str = "old";

/// A pane that no host holds.
const NO_PANE: PaneId = PaneId(999_999);

/// A window that no host holds.
const NO_WINDOW: WindowId = WindowId(999_999);

/// A tab that no host holds.
const NO_TAB: TabId = TabId(999_999);

/// The first of the symbols, past every id that a host gives: a window's
/// are from it, a tab's from twice it, a pane's from three times it.
pub const SYMBOL: u64 = 1 << 40;

/// The window that the trace meets as the `k`th of those that the
/// scenario opens.
pub const fn window(k: u64) -> WindowId {
    WindowId(SYMBOL + k)
}

/// The tab that the trace meets as the `k`th of those that the scenario
/// opens.
pub const fn tab(k: u64) -> TabId {
    TabId(2 * SYMBOL + k)
}

/// The pane that the trace meets as the `k`th of those that the scenario
/// opens.
pub const fn pane(k: u64) -> PaneId {
    PaneId(3 * SYMBOL + k)
}

/// The world of the scenarios, as the kernel's host made it: its window,
/// its tab and its pane, and the pane's row of [`LINE`], with the line's
/// text and cells as the pane reads them.
#[derive(Clone, Debug, PartialEq)]
pub struct World {
    pub window: WindowId,
    pub tab: TabId,
    pub pane: PaneId,
    pub row: i64,
    pub line: LineText,
}

/// A scenario: what the test plugin holds, and its steps.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Scenario {
    /// What it is of.
    pub name: String,
    /// The capabilities that the plugin holds, as its manifest declares
    /// them; `None` for every one, as a first-party plugin's.
    pub capabilities: Option<Vec<Capability>>,
    /// The events that the plugin subscribes to as it activates.
    pub subscriptions: Vec<String>,
    pub steps: Vec<Step>,
}

/// A step of a scenario: a call of the plugin's, the workspace's events,
/// and what the plugin hears after them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Step {
    /// The requests that the call makes, each with the answer that it
    /// takes; none for a step of events alone, which makes no call.
    pub asks: Vec<(Request, Response)>,
    /// The workspace's events, which the host posts after the call.
    pub posts: Vec<Event>,
    /// The events that the plugin hears after them, as one batch.
    pub heard: Vec<Event>,
}

/// A step of a call alone.
fn ask(asks: Vec<(Request, Response)>) -> Step {
    Step {
        asks,
        posts: Vec::new(),
        heard: Vec::new(),
    }
}

/// A failure of `code` and `message`.
fn failed(code: Code, message: &str) -> Response {
    Response::Failed(Error::new(code, message))
}

/// The scenarios, of `world`, in the order that the kernel's host runs
/// them.
pub fn all(world: &World) -> Vec<Scenario> {
    vec![
        links(world),
        events(world),
        capabilities(world),
        declarations(),
        app_requests(world),
        state_and_rows(world),
        views(world),
        workspace(world),
        tabs(world),
        core_actions(world),
    ]
}

/// What a tab bar's plugin asks of a tab: a title and a color of its own,
/// which a read of the tabs gives, and back; a text that the user is asked
/// for, whose answer comes to the plugin alone; its bar's place, which a
/// later call reads; and what of them is refused.
fn tabs(world: &World) -> Scenario {
    let (w, t, p) = (world.window, world.tab, world.pane);
    let info = |title: &str, color| TabInfo {
        id: t,
        window: w,
        title: title.to_owned(),
        color,
        active: true,
        focused: p,
        panes: vec![p],
        zoomed: None,
    };
    let red = Color::Rgb(200, 30, 40);
    let answer = Event::Answered {
        id: 0,
        text: Some("work".to_owned()),
    };
    Scenario {
        name: "a tab's own title and color, and a question".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            Step {
                asks: vec![
                    (
                        Request::SetTabTitle {
                            tab: t,
                            title: Some("named".to_owned()),
                        },
                        Response::Done,
                    ),
                    (
                        Request::SetTabColor {
                            tab: t,
                            color: Some(red),
                        },
                        Response::Done,
                    ),
                    (
                        Request::SetTabColor {
                            tab: t,
                            color: Some(Color::Palette(1)),
                        },
                        failed(
                            Code::Failed,
                            "a tab's color is an RGB one, not one of the palette's",
                        ),
                    ),
                    (
                        Request::SetTabTitle {
                            tab: NO_TAB,
                            title: None,
                        },
                        failed(Code::NotFound, "there is no tab 999999"),
                    ),
                    (
                        Request::AskText {
                            window: None,
                            message: "The tab's title".to_owned(),
                            detail: String::new(),
                            text: "named".to_owned(),
                        },
                        Response::Asking(0),
                    ),
                    (
                        Request::AskText {
                            window: Some(NO_WINDOW),
                            message: String::new(),
                            detail: String::new(),
                            text: String::new(),
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                ],
                posts: vec![answer.clone()],
                heard: vec![answer],
            },
            ask(vec![
                (
                    Request::Tabs(w),
                    Response::Tabs(vec![info("named", Some(red))]),
                ),
                (
                    Request::SetTabTitle {
                        tab: t,
                        title: None,
                    },
                    Response::Done,
                ),
                (
                    Request::SetTabColor {
                        tab: t,
                        color: None,
                    },
                    Response::Done,
                ),
            ]),
            ask(vec![(
                Request::Tabs(w),
                Response::Tabs(vec![info("", None)]),
            )]),
            // Its tab bar's place, which a later call reads, a column's
            // width within its limits.
            ask(vec![
                (
                    Request::SetTabBarPlace(BarPlace::Left { width: 200.0 }),
                    Response::Done,
                ),
                (
                    Request::SetTabBarPlace(BarPlace::Right { width: 10.0 }),
                    failed(
                        Code::Failed,
                        "a tab bar's column is from 60 to 600 points wide, not 10",
                    ),
                ),
                (
                    Request::TabBarPlace,
                    Response::TabBarPlace(BarPlace::Titlebar),
                ),
            ]),
            ask(vec![
                (
                    Request::TabBarPlace,
                    Response::TabBarPlace(BarPlace::Left { width: 200.0 }),
                ),
                (Request::SetTabBarPlace(BarPlace::Titlebar), Response::Done),
            ]),
            // A drag of its column's edge, told to it alone.
            Step {
                asks: Vec::new(),
                posts: vec![Event::TabBarResized { width: 240.0 }],
                heard: vec![Event::TabBarResized { width: 240.0 }],
            },
            // The keys of a command of its arguments, as the platform writes
            // them, and none of others.
            ask(vec![
                (
                    Request::KeysOf {
                        command: "tabs.goto".to_owned(),
                        args: Args::from([("index".to_owned(), Value::Integer(1))]),
                    },
                    Response::Text(Some("⌘1".to_owned())),
                ),
                (
                    Request::KeysOf {
                        command: "tabs.goto".to_owned(),
                        args: Args::from([("index".to_owned(), Value::Integer(2))]),
                    },
                    Response::Text(None),
                ),
                // A message that its catalogs lack, none on either host.
                (
                    Request::Message {
                        id: "tab-untitled".to_owned(),
                        args: BTreeMap::from([("index".to_owned(), Data::Integer(2))]),
                    },
                    Response::Text(None),
                ),
            ]),
        ],
    }
}

/// What `links` asks: the focused pane, its line, its link and its
/// directory, what opens, the underline of a link, and the pointer.
fn links(world: &World) -> Scenario {
    let (pane, row) = (world.pane, world.row);
    let look = DecorationLook {
        underline: Some(Underline::Single),
        underline_color: None,
        background: None,
        pointing: true,
    };
    let pointer = Event::Pointer {
        pane,
        at: Some((6, row)),
        mods: Mods {
            command: true,
            ..Mods::default()
        },
    };
    Scenario {
        name: "links".to_owned(),
        capabilities: None,
        subscriptions: vec!["pointer".to_owned()],
        steps: vec![
            ask(vec![
                (Request::FocusedWindow, Response::Window(Some(world.window))),
                (Request::FocusedPane, Response::Pane(Some(pane))),
                (
                    Request::LineText {
                        pane,
                        row,
                        reach: 32,
                    },
                    Response::Line(Some(world.line.clone())),
                ),
                (
                    Request::LineText {
                        pane,
                        row: row + 1000,
                        reach: 32,
                    },
                    Response::Line(None),
                ),
                (
                    Request::LinkAt {
                        pane,
                        column: 6,
                        row,
                    },
                    Response::Text(None),
                ),
                (
                    Request::WorkingDirectory(pane),
                    Response::Text(Some("/".to_owned())),
                ),
                (
                    Request::Opens("https://lokyr.dev/x".to_owned()),
                    Response::Boolean(true),
                ),
                (
                    Request::Opens("javascript:x".to_owned()),
                    Response::Boolean(false),
                ),
                (
                    Request::Open("javascript:x".to_owned()),
                    failed(
                        Code::Failed,
                        "`javascript:` does not open: only http, https, mailto, file",
                    ),
                ),
                (
                    Request::Open("https://lokyr.dev/x".to_owned()),
                    Response::Done,
                ),
                (
                    Request::Decorate {
                        pane,
                        from: (4, row),
                        to: (22, row),
                        look,
                    },
                    Response::Decoration(0),
                ),
                (
                    Request::Decorate {
                        pane,
                        from: (22, row),
                        to: (4, row),
                        look,
                    },
                    failed(
                        Code::Failed,
                        "a decoration's cells are not the pane's, or not in reading order",
                    ),
                ),
                (
                    Request::Undecorate { pane, id: 999_999 },
                    failed(
                        Code::NotFound,
                        &format!("the plugin has no decoration 999999 of the pane {}", pane.0),
                    ),
                ),
                (
                    Request::LineText {
                        pane: NO_PANE,
                        row,
                        reach: 0,
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
            ]),
            Step {
                asks: Vec::new(),
                posts: vec![pointer.clone()],
                heard: vec![pointer],
            },
        ],
    }
}

/// What every plugin meets of its events and its panels: an event of its
/// own namespace, which it hears as the call returns, and another's,
/// refused; a panel opened once, and closed once, whose closing it hears;
/// and the workspace's events that it subscribed to.
fn events(world: &World) -> Scenario {
    let ping = Event::Custom {
        name: "conformer.ping".to_owned(),
        schema: "conformer.ping/1".to_owned(),
        value: Data::Integer(1),
    };
    let menu = || Request::OpenPanel {
        id: PanelId::from("menu"),
        options: PanelOptions {
            anchor: Anchor::Pointer(world.window),
            role: Role::Menu,
            close_outside: true,
            close_escape: true,
        },
    };
    let created = Event::Workspace(WorkspaceEvent::TabCreated {
        tab: tab(0),
        window: world.window,
        index: 1,
    });
    Scenario {
        name: "events and panels".to_owned(),
        capabilities: None,
        subscriptions: vec!["conformer.ping".to_owned(), "tab-created".to_owned()],
        steps: vec![
            Step {
                asks: vec![
                    (
                        Request::Emit {
                            name: "conformer.ping".to_owned(),
                            schema: "conformer.ping/1".to_owned(),
                            value: Data::Integer(1),
                        },
                        Response::Done,
                    ),
                    (
                        Request::Emit {
                            name: "other.ping".to_owned(),
                            schema: "other.ping/1".to_owned(),
                            value: Data::Nil,
                        },
                        failed(
                            Code::InvalidName,
                            "`other.ping` is not an event of the plugin's, `conformer.event`",
                        ),
                    ),
                    (
                        Request::Emit {
                            name: "conformer.ping".to_owned(),
                            schema: String::new(),
                            value: Data::Nil,
                        },
                        failed(
                            Code::InvalidName,
                            "the event `conformer.ping` names no schema",
                        ),
                    ),
                    (
                        Request::Emit {
                            name: "conformer.a b".to_owned(),
                            schema: "conformer.a/1".to_owned(),
                            value: Data::Nil,
                        },
                        failed(
                            Code::InvalidName,
                            "`conformer.a b` is not an event of the plugin's, `conformer.event`",
                        ),
                    ),
                    (menu(), Response::Done),
                    (
                        menu(),
                        failed(
                            Code::AlreadyRegistered,
                            "the plugin has a panel `menu` open",
                        ),
                    ),
                    (Request::ClosePanel(PanelId::from("menu")), Response::Done),
                    (
                        Request::ClosePanel(PanelId::from("menu")),
                        failed(Code::NotFound, "the plugin has no panel `menu` open"),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![Event::PanelClosed(PanelId::from("menu")), ping],
            },
            // The workspace's events that it did not subscribe to go.
            Step {
                asks: Vec::new(),
                posts: vec![
                    created.clone(),
                    Event::Workspace(WorkspaceEvent::TabClosed {
                        tab: tab(0),
                        window: world.window,
                    }),
                ],
                heard: vec![created],
            },
        ],
    }
}

/// What a plugin of the clipboard's reading alone meets: its read, and the
/// others refused, of the capabilities that it lacks.
fn capabilities(world: &World) -> Scenario {
    let lacks = |capability: &str| {
        failed(
            Code::MissingCapability,
            &format!("the plugin `acme/conformer` lacks the capability `{capability}`"),
        )
    };
    Scenario {
        name: "capabilities".to_owned(),
        capabilities: Some(vec![Capability::ClipboardRead]),
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (
                Request::ClipboardText,
                Response::Text(Some(CLIPBOARD.to_owned())),
            ),
            (Request::SelectionText(world.pane), lacks("terminal-read")),
            (
                Request::SendText {
                    pane: world.pane,
                    text: "x".to_owned(),
                },
                lacks("pty-write"),
            ),
            (Request::Open("https://lokyr.dev".to_owned()), lacks("open")),
            (
                Request::SetClipboardText("x".to_owned()),
                lacks("clipboard-write"),
            ),
        ])],
    }
}

/// What `core-actions` asks: the text's size, the clipboard, whose write a
/// read of the same call does not see, and the writes to the focused pane.
fn core_actions(world: &World) -> Scenario {
    let pane = world.pane;
    Scenario {
        name: "core-actions".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            ask(vec![
                (Request::FontSize, Response::FontSize(13.0)),
                (Request::SetFontSize(Some(14.0)), Response::Done),
                (Request::FocusedPane, Response::Pane(Some(pane))),
                (Request::SelectionText(pane), Response::Text(None)),
                (
                    Request::ClipboardText,
                    Response::Text(Some(CLIPBOARD.to_owned())),
                ),
                (Request::SetClipboardText("new".to_owned()), Response::Done),
                (
                    Request::ClipboardText,
                    Response::Text(Some(CLIPBOARD.to_owned())),
                ),
                (
                    Request::Paste {
                        pane,
                        text: "true".to_owned(),
                    },
                    Response::Done,
                ),
                (
                    Request::SendText {
                        pane,
                        text: "y".to_owned(),
                    },
                    Response::Done,
                ),
                (
                    Request::Scroll {
                        pane,
                        scroll: Scroll::Up(1),
                    },
                    Response::Done,
                ),
                (Request::SelectAll(pane), Response::Done),
                (Request::ClearScrollback(pane), Response::Done),
                (Request::Clear(pane), Response::Done),
                (
                    Request::Scroll {
                        pane: NO_PANE,
                        scroll: Scroll::Top,
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
                (
                    Request::Config,
                    Response::Data(Data::Map(Default::default())),
                ),
                (Request::State, Response::Data(Data::Nil)),
            ]),
            ask(vec![(
                Request::ClipboardText,
                Response::Text(Some("new".to_owned())),
            )]),
        ],
    }
}

/// The test plugin's one command, as its activation registers it.
pub fn ask_command() -> Command {
    Command {
        name: "conformer.ask".to_owned(),
        title: "Ask".to_owned(),
        category: Category::View,
        capabilities: Vec::new(),
        params: Vec::new(),
        when: None,
    }
}

/// What every plugin meets of its declarations: a command, a binding and
/// subscriptions, each refused of a wrong name, keys or context, and a
/// command registered twice.
fn declarations() -> Scenario {
    let bind = |keys: &str, command: &str, when: Option<&str>| {
        Request::Bind(Binding {
            keys: keys.to_owned(),
            command: command.to_owned(),
            args: Args::new(),
            when: when.map(str::to_owned),
        })
    };
    let invalid = |message: &str| failed(Code::InvalidBinding, message);
    let other = Command {
        name: "conformer.other".to_owned(),
        ..ask_command()
    };
    Scenario {
        name: "declarations".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (Request::RegisterCommand(other), Response::Done),
            (
                Request::RegisterCommand(ask_command()),
                failed(
                    Code::AlreadyRegistered,
                    "`conformer.ask` is registered already",
                ),
            ),
            (
                Request::RegisterCommand(Command {
                    name: "Conformer.x".to_owned(),
                    ..ask_command()
                }),
                failed(
                    Code::InvalidName,
                    "`Conformer.x` is not a command's name, `namespace.action`",
                ),
            ),
            (
                bind("cmd+shift+k", "conformer.ask", Some("pane.focused")),
                Response::Done,
            ),
            (
                bind("cmd+shfit+k", "conformer.ask", None),
                invalid("`shfit` is not a modifier, in `cmd+shfit+k`"),
            ),
            (
                bind("cmd+k", "conformer ask", None),
                invalid("`conformer ask` is not a command's name, `namespace.action`"),
            ),
            (
                bind("cmd+k", "conformer.ask", Some("pane.focused &&")),
                invalid("the expression ends early"),
            ),
            (
                Request::Subscribe(vec!["window-opened".to_owned()]),
                Response::Done,
            ),
            (
                Request::Subscribe(vec!["tab-opened".to_owned()]),
                failed(
                    Code::InvalidName,
                    "`tab-opened` is no event: one of the workspace's, as `tab-created`, or a plugin's, `namespace.event`",
                ),
            ),
        ])],
    }
}

/// What every plugin asks of the app: a command run, a window opened or
/// closed, the app quit and the text's size, and what of them is refused.
fn app_requests(world: &World) -> Scenario {
    Scenario {
        name: "the app's requests".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![ask(vec![
            (
                Request::RunCommand {
                    name: "conformer.ask".to_owned(),
                    args: Args::new(),
                },
                Response::Done,
            ),
            (
                Request::RunCommand {
                    name: "app.none".to_owned(),
                    args: Args::new(),
                },
                failed(Code::NotFound, "no command `app.none`"),
            ),
            (
                Request::RunCommand {
                    name: "conformer.ask".to_owned(),
                    args: Args::from([("n".to_owned(), Value::Integer(1))]),
                },
                failed(
                    Code::UnknownArgument,
                    "`conformer.ask` takes no argument `n`",
                ),
            ),
            (Request::OpenWindow, Response::Done),
            (Request::CloseWindow(world.window), Response::Done),
            (
                Request::CloseWindow(NO_WINDOW),
                failed(Code::NotFound, "there is no window 999999"),
            ),
            (Request::Quit, Response::Done),
            (
                Request::SetFontSize(Some(1000.0)),
                failed(
                    Code::Failed,
                    "the size, in points, is from 4 to 200, not 1000",
                ),
            ),
            (Request::SetFontSize(None), Response::Done),
        ])],
    }
}

/// What every plugin meets of its state and of the reads of rows: a state
/// kept, which a read of the same call does not see, and one refused; a
/// read of rows, whose rows come as an event; the styles of a row that the
/// pane does not hold, and of a pane not there.
fn state_and_rows(world: &World) -> Scenario {
    let kept = Data::Map(BTreeMap::from([("a".to_owned(), Data::Integer(1))]));
    Scenario {
        name: "state and rows".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            Step {
                asks: vec![
                    (Request::SetState(kept.clone()), Response::Done),
                    (Request::State, Response::Data(Data::Nil)),
                    (
                        Request::SetState(Data::Text("x".repeat(MOST_STATE))),
                        failed(
                            Code::Failed,
                            &format!(
                                "a plugin's state is {MOST_STATE} bytes at most as JSON, not {}",
                                MOST_STATE + 2
                            ),
                        ),
                    ),
                    (
                        Request::ReadLines {
                            pane: world.pane,
                            first: world.row,
                            count: 1,
                        },
                        Response::Reading(0),
                    ),
                    (
                        Request::Styles {
                            pane: world.pane,
                            row: 1000,
                        },
                        Response::Styles(None),
                    ),
                    (
                        Request::Styles {
                            pane: NO_PANE,
                            row: 0,
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                ],
                posts: Vec::new(),
                heard: vec![Event::Lines {
                    id: 0,
                    pane: world.pane,
                    lines: vec![Line {
                        row: world.row,
                        text: LINE.to_owned(),
                        wrapped: false,
                    }],
                    complete: true,
                }],
            },
            ask(vec![(Request::State, Response::Data(kept))]),
        ],
    }
}

/// The test plugin's panel, of the id `menu`.
pub const MENU: &str = "menu";

/// What every plugin meets of its views: a panel opened, whose render it
/// gives, a field's text set and a list's selection moved in its tree,
/// renders asked for, and what of them is refused, of a node of another
/// kind, a node not there, a view not its own, a panel of no id or of an
/// anchor not there.
fn views(world: &World) -> Scenario {
    let menu = ViewId::Panel(PanelId::from(MENU));
    let options = |anchor| PanelOptions {
        anchor,
        role: Role::Menu,
        close_outside: true,
        close_escape: true,
    };
    let set_text = |key: &str| Request::SetText {
        view: menu.clone(),
        key: key.into(),
        text: "x".to_owned(),
        revision: 0,
    };
    let select = |key: &str| Request::Select {
        view: menu.clone(),
        key: key.into(),
        item: 1,
    };
    let status_bar = ViewId::StatusBar(world.window);
    Scenario {
        name: "views".to_owned(),
        capabilities: None,
        subscriptions: Vec::new(),
        steps: vec![
            ask(vec![
                (
                    Request::OpenPanel {
                        id: PanelId::from(MENU),
                        options: options(Anchor::Pointer(world.window)),
                    },
                    Response::Done,
                ),
                (
                    Request::OpenPanel {
                        id: PanelId::from(""),
                        options: options(Anchor::Pointer(world.window)),
                    },
                    failed(Code::InvalidName, "a panel's id is not empty"),
                ),
                (
                    Request::OpenPanel {
                        id: PanelId::from("other"),
                        options: options(Anchor::Pointer(NO_WINDOW)),
                    },
                    failed(Code::NotFound, "there is no window 999999"),
                ),
                (
                    Request::OpenPanel {
                        id: PanelId::from("other"),
                        options: options(Anchor::Cursor(NO_PANE)),
                    },
                    failed(Code::NotFound, "there is no pane 999999"),
                ),
            ]),
            // Its render came as it opened.
            ask(vec![
                (set_text("query"), Response::Done),
                (select("results"), Response::Done),
                (
                    set_text("results"),
                    failed(Code::NotFound, "`results` is not a field"),
                ),
                (
                    select("query"),
                    failed(Code::NotFound, "`query` is not a list"),
                ),
                (
                    set_text("none"),
                    failed(
                        Code::NotFound,
                        &format!("the view {menu:?} has no node `none`"),
                    ),
                ),
                (Request::RequestRender(menu.clone()), Response::Done),
                (
                    Request::RequestRender(status_bar.clone()),
                    failed(
                        Code::NotFound,
                        &format!("the plugin has no view {status_bar:?}"),
                    ),
                ),
            ]),
            Step {
                asks: vec![(Request::ClosePanel(PanelId::from(MENU)), Response::Done)],
                posts: Vec::new(),
                heard: vec![Event::PanelClosed(PanelId::from(MENU))],
            },
        ],
    }
}

/// What every plugin meets of the workspace's writes, of the tabs, the
/// splits and the windows that its calls open, which a later write of the
/// same call finds, and of their events; what of them is refused; and the
/// workspace left as the scenario found it.
fn workspace(world: &World) -> Scenario {
    let (w, t, p) = (world.window, world.tab, world.pane);
    let ws = |event| Event::Workspace(event);
    let info = |id, active, focused, panes: Vec<PaneId>, zoomed| TabInfo {
        id,
        window: w,
        title: String::new(),
        color: None,
        active,
        focused,
        panes,
        zoomed,
    };
    let split = |ratio| Layout::Split {
        direction: Direction::Vertical,
        ratio,
        first: Box::new(Layout::Pane(pane(0))),
        second: Box::new(Layout::Pane(pane(1))),
    };
    let names = WorkspaceEvent::NAMES.iter().map(|&name| name.to_owned());
    Scenario {
        name: "the workspace".to_owned(),
        capabilities: None,
        subscriptions: names.collect(),
        steps: vec![
            Step {
                asks: vec![
                    (
                        Request::NewTab {
                            window: w,
                            index: None,
                        },
                        Response::Tab {
                            tab: tab(0),
                            pane: pane(0),
                        },
                    ),
                    (
                        Request::Split {
                            pane: pane(0),
                            direction: Direction::Vertical,
                            side: Side::Second,
                        },
                        Response::Pane(Some(pane(1))),
                    ),
                    (
                        Request::SendText {
                            pane: pane(1),
                            text: "x".to_owned(),
                        },
                        Response::Done,
                    ),
                    (Request::ActivateTab(tab(0)), Response::Done),
                    (Request::FocusPane(pane(1)), Response::Done),
                    (
                        Request::Zoom {
                            tab: tab(0),
                            pane: Some(pane(0)),
                        },
                        Response::Done,
                    ),
                    (
                        Request::NewTab {
                            window: NO_WINDOW,
                            index: None,
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                    (
                        Request::CloseTab {
                            tab: NO_TAB,
                            ask: false,
                        },
                        failed(Code::NotFound, "there is no tab 999999"),
                    ),
                    (
                        Request::ClosePane {
                            pane: NO_PANE,
                            ask: true,
                        },
                        failed(Code::NotFound, "there is no pane 999999"),
                    ),
                    (
                        Request::MoveTab {
                            tab: tab(0),
                            window: NO_WINDOW,
                            index: 0,
                        },
                        failed(Code::NotFound, "there is no window 999999"),
                    ),
                    (
                        Request::ResizePane {
                            pane: p,
                            edge: Edge::Right,
                            by: 2.0,
                        },
                        failed(Code::Failed, "a resize of 2 is not a share from -1 to 1"),
                    ),
                    (
                        Request::SetRatio {
                            tab: t,
                            path: Vec::new(),
                            ratio: Some(3.0),
                        },
                        failed(Code::Failed, "a split's ratio is a share from 0 to 1"),
                    ),
                    // Of no change: a pane of no edge, a tab of one pane.
                    (
                        Request::ResizePane {
                            pane: p,
                            edge: Edge::Right,
                            by: 0.1,
                        },
                        Response::Done,
                    ),
                    (Request::Equalize(t), Response::Done),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::TabCreated {
                        tab: tab(0),
                        window: w,
                        index: 1,
                    }),
                    ws(WorkspaceEvent::PaneCreated {
                        pane: pane(0),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::PaneCreated {
                        pane: pane(1),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::TabActivated {
                        tab: tab(0),
                        window: w,
                    }),
                    ws(WorkspaceEvent::PaneFocused {
                        pane: pane(1),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                ],
            },
            Step {
                asks: vec![
                    (
                        Request::Windows,
                        Response::Windows(vec![WindowInfo {
                            id: w,
                            tabs: vec![t, tab(0)],
                            active: tab(0),
                            number: None,
                            keys: None,
                        }]),
                    ),
                    (
                        Request::Tabs(w),
                        Response::Tabs(vec![
                            info(t, false, p, vec![p], None),
                            info(tab(0), true, pane(1), vec![pane(0), pane(1)], Some(pane(0))),
                        ]),
                    ),
                    (Request::Layout(tab(0)), Response::Layout(split(None))),
                    (Request::FocusedPane, Response::Pane(Some(pane(1)))),
                    (
                        Request::SetRatio {
                            tab: tab(0),
                            path: Vec::new(),
                            ratio: Some(0.3),
                        },
                        Response::Done,
                    ),
                    (
                        Request::MoveTab {
                            tab: tab(0),
                            window: w,
                            index: 0,
                        },
                        Response::Done,
                    ),
                    (Request::TabToNewWindow(tab(0)), Response::Done),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::TabMoved {
                        tab: tab(0),
                        from: w,
                        window: w,
                        index: 0,
                    }),
                    ws(WorkspaceEvent::WindowOpened { window: window(0) }),
                    ws(WorkspaceEvent::TabMoved {
                        tab: tab(0),
                        from: w,
                        window: window(0),
                        index: 0,
                    }),
                    ws(WorkspaceEvent::TabActivated { tab: t, window: w }),
                ],
            },
            Step {
                asks: vec![
                    (
                        Request::Windows,
                        Response::Windows(vec![
                            WindowInfo {
                                id: w,
                                tabs: vec![t],
                                active: t,
                                number: None,
                                keys: None,
                            },
                            WindowInfo {
                                id: window(0),
                                tabs: vec![tab(0)],
                                active: tab(0),
                                number: None,
                                keys: None,
                            },
                        ]),
                    ),
                    (Request::FocusedPane, Response::Pane(Some(p))),
                    (Request::Layout(tab(0)), Response::Layout(split(Some(0.3)))),
                    (Request::PaneToNewWindow(pane(1)), Response::Done),
                    (
                        Request::PaneToNewTab {
                            pane: pane(0),
                            window: w,
                            index: 5,
                        },
                        Response::Done,
                    ),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::LayoutChanged { tab: tab(0) }),
                    ws(WorkspaceEvent::PaneFocused {
                        pane: pane(0),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::WindowOpened { window: window(1) }),
                    ws(WorkspaceEvent::TabCreated {
                        tab: tab(1),
                        window: window(1),
                        index: 0,
                    }),
                    ws(WorkspaceEvent::PaneMoved {
                        pane: pane(1),
                        from: tab(0),
                        tab: tab(1),
                    }),
                    ws(WorkspaceEvent::TabMoved {
                        tab: tab(0),
                        from: window(0),
                        window: w,
                        index: 1,
                    }),
                    ws(WorkspaceEvent::WindowClosed { window: window(0) }),
                ],
            },
            Step {
                asks: vec![
                    (
                        Request::CloseTab {
                            tab: tab(1),
                            ask: false,
                        },
                        Response::Done,
                    ),
                    // Of a shell at its prompt, as the user confirms it.
                    (
                        Request::CloseTab {
                            tab: tab(0),
                            ask: true,
                        },
                        Response::Done,
                    ),
                ],
                posts: Vec::new(),
                heard: vec![
                    ws(WorkspaceEvent::PaneClosed {
                        pane: pane(1),
                        tab: tab(1),
                    }),
                    ws(WorkspaceEvent::TabClosed {
                        tab: tab(1),
                        window: window(1),
                    }),
                    ws(WorkspaceEvent::WindowClosed { window: window(1) }),
                    ws(WorkspaceEvent::PaneClosed {
                        pane: pane(0),
                        tab: tab(0),
                    }),
                    ws(WorkspaceEvent::TabClosed {
                        tab: tab(0),
                        window: w,
                    }),
                ],
            },
            ask(vec![(
                Request::Windows,
                Response::Windows(vec![WindowInfo {
                    id: w,
                    tabs: vec![t],
                    active: t,
                    number: None,
                    keys: None,
                }]),
            )]),
        ],
    }
}

/// `scenario` as JSON, as a front end of another language takes it.
pub fn json(scenario: &Scenario) -> String {
    serde_json::to_string_pretty(scenario).expect("a scenario writes as JSON")
}

/// The scenario of `text`, its JSON.
pub fn from_json(text: &str) -> Scenario {
    serde_json::from_str(text).expect("a scenario reads from JSON")
}
