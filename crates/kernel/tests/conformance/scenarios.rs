//! The scenarios of the conformance suite, of the requests that the
//! first-party plugins make: `links`', the reads of a line, its link, its
//! directory, what opens, the decorations and the pointer's events;
//! `core-actions`', the text's size, the clipboard and the writes to a
//! pane; and the events, the panels and the capabilities that every plugin
//! meets. Each runs in the world of the suite: a window of a pane whose
//! program, in `/`, wrote [`LINE`], the focused one, and a clipboard of
//! [`CLIPBOARD`]; the kernel's host runs them in their order, in one world,
//! `core-actions`' last, as its writes change the pane.

use lokyr_plugin_sdk::command::{Capability, Code, Error};
use lokyr_plugin_sdk::event::WorkspaceEvent;
use lokyr_plugin_sdk::host::{
    DecorationLook, LineText, PaneId, Scroll, TabId, Underline, WindowId,
};
use lokyr_plugin_sdk::message::{Data, Event, Mods, Request, Response};
use lokyr_plugin_sdk::ui::{Anchor, PanelId, PanelOptions, Role};
use serde::{Deserialize, Serialize};

/// The line that the world's pane's program wrote.
pub const LINE: &str = "see https://lokyr.dev/x now";

/// The clipboard's text as the world starts.
pub const CLIPBOARD: &str = "old";

/// A pane that no host holds.
const NO_PANE: PaneId = PaneId(999_999);

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
        core_actions(world),
    ]
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
        tab: TabId(world.tab.0 + 1000),
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
                        tab: TabId(world.tab.0 + 1000),
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

/// `scenario` as JSON, as a front end of another language takes it.
pub fn json(scenario: &Scenario) -> String {
    serde_json::to_string_pretty(scenario).expect("a scenario writes as JSON")
}

/// The scenario of `text`, its JSON.
pub fn from_json(text: &str) -> Scenario {
    serde_json::from_str(text).expect("a scenario reads from JSON")
}
