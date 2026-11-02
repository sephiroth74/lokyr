//! The plugin API as messages: the requests of a plugin to the host, each
//! with the plugin that asks, the host's responses, and the events that the
//! host tells. They are plain data, which serde carries, so that a plugin
//! on the main thread, one on a thread of its own, a Luau VM, and the IPC's
//! and MCP's clients reach the host through the same messages.
//!
//! The host answers a read with its state as it handles the request, and
//! queues a write, which it applies, in the order of the plugin's writes,
//! as the plugin's call returns: a plugin that reads what it just wrote, in
//! the same call, reads the state before its write. What changes the app,
//! as a window to open or a command to run, comes once the call returns
//! too, after the writes. A plugin's declarations, its commands, its keys
//! and the events that it subscribes to, take effect at once, as the host
//! checks them.
//!
//! The host gathers the events, the workspace's changes and those that
//! plugins emit, and delivers them in batches, in their order, each plugin
//! the events that it subscribed to, once a turn of the main thread: an
//! event that a call emits comes after the call returns, never within it,
//! and one that a delivery's call emits, in the next batch.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::command::{Args, Capability, Command, Error};
use crate::event::WorkspaceEvent;
use crate::host::{Binding, LineText, PaneId, Scroll, WindowId};

/// A request, with the plugin that asks it, by its name, as it crosses to
/// the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    /// The plugin that asks.
    pub caller: String,
    /// What it asks.
    pub request: Request,
}

/// What a plugin asks of the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Request {
    /// Registers a command, whose calls come to the plugin.
    RegisterCommand(Command),
    /// Binds keys to a command, which needs not be registered yet.
    Bind(Binding),
    /// Subscribes to the events of these names: the workspace's, as
    /// `tab-created`, or plugins' own, as `search.progress`.
    Subscribe(Vec<String>),
    /// Emits an event of the plugin's own, whose name is of its namespace,
    /// as `search.progress`, to the plugins that subscribed to it, once the
    /// call returns.
    Emit {
        /// Its name, `namespace.event`.
        name: String,
        /// The id of its value's schema, as `search.progress/1`.
        schema: String,
        /// Its value.
        value: Data,
    },
    /// Runs a command with arguments, once the call returns; the call is
    /// checked as it is asked.
    RunCommand {
        /// The command's name.
        name: String,
        /// Its arguments.
        args: Args,
    },
    /// The clipboard's text.
    ClipboardText,
    /// Puts text on the clipboard.
    SetClipboardText(String),
    /// The window that has the focus.
    FocusedWindow,
    /// The pane that has the focus: the active one of the focused window.
    FocusedPane,
    /// The text of a pane's selection.
    SelectionText(PaneId),
    /// The text of the logical line of a pane's row, by its number, as far
    /// as `reach` rows before it and after it, with each character's cell.
    LineText {
        /// The pane.
        pane: PaneId,
        /// The row's number.
        row: i64,
        /// The rows before it and after it that the read takes.
        reach: usize,
    },
    /// The target of the OSC 8 link at a cell of a pane, by its column and
    /// its row's number.
    LinkAt {
        /// The pane.
        pane: PaneId,
        /// The cell's column.
        column: u16,
        /// Its row's number.
        row: i64,
    },
    /// The working directory of the program in a pane's foreground.
    WorkingDirectory(PaneId),
    /// Opens a URL, or a path, absolute or from `~`, with the system's
    /// handler, as the user's config says.
    Open(String),
    /// Whether [`Request::Open`] would open the target.
    Opens(String),
    /// Selects everything of a pane: its scrollback and its screen.
    SelectAll(PaneId),
    /// Clears a pane's scrollback.
    ClearScrollback(PaneId),
    /// Clears a pane's scrollback, and its screen but the prompt.
    Clear(PaneId),
    /// Pastes text into a pane, as the user pastes.
    Paste {
        /// The pane.
        pane: PaneId,
        /// The text.
        text: String,
    },
    /// Sends text to a pane's program as it is, as typed keys would.
    SendText {
        /// The pane.
        pane: PaneId,
        /// The text.
        text: String,
    },
    /// Scrolls a pane's view.
    Scroll {
        /// The pane.
        pane: PaneId,
        /// How.
        scroll: Scroll,
    },
    /// The size of the text, in points, of every window.
    FontSize,
    /// Gives the text a size, in points, from 4 to 200; with `None`, the
    /// config's.
    SetFontSize(Option<f32>),
    /// Opens a window.
    OpenWindow,
    /// Closes a window.
    CloseWindow(WindowId),
    /// Quits the app.
    Quit,
}

/// What a request is to the host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// It reads the host's state, which it answers at once.
    Read,
    /// It changes the kernel's state, which the host applies as the call
    /// returns.
    Write,
    /// It asks the app, which acts once the call returns.
    App,
    /// It declares the plugin's commands or keys, which take effect at
    /// once.
    Declaration,
}

impl Request {
    /// The capability that the request needs, which the host checks
    /// against the plugin's, if it needs one.
    pub fn capability(&self) -> Option<Capability> {
        match self {
            Self::ClipboardText => Some(Capability::ClipboardRead),
            Self::SetClipboardText(_) => Some(Capability::ClipboardWrite),
            Self::SelectionText(_) | Self::LineText { .. } | Self::LinkAt { .. } => {
                Some(Capability::TerminalRead)
            }
            Self::WorkingDirectory(_) => Some(Capability::ProcessRead),
            Self::Open(_) | Self::Opens(_) => Some(Capability::Open),
            Self::Paste { .. } | Self::SendText { .. } => Some(Capability::PtyWrite),
            Self::RegisterCommand(_)
            | Self::Bind(_)
            | Self::Subscribe(_)
            | Self::Emit { .. }
            | Self::RunCommand { .. }
            | Self::FocusedWindow
            | Self::FocusedPane
            | Self::SelectAll(_)
            | Self::ClearScrollback(_)
            | Self::Clear(_)
            | Self::Scroll { .. }
            | Self::FontSize
            | Self::SetFontSize(_)
            | Self::OpenWindow
            | Self::CloseWindow(_)
            | Self::Quit => None,
        }
    }

    /// What the request is to the host: a read, a write, a request of the
    /// app's, or a declaration.
    pub fn effect(&self) -> Effect {
        match self {
            Self::ClipboardText
            | Self::FocusedWindow
            | Self::FocusedPane
            | Self::SelectionText(_)
            | Self::LineText { .. }
            | Self::LinkAt { .. }
            | Self::WorkingDirectory(_)
            | Self::Opens(_)
            | Self::FontSize => Effect::Read,
            Self::SetClipboardText(_)
            | Self::SelectAll(_)
            | Self::ClearScrollback(_)
            | Self::Clear(_)
            | Self::Paste { .. }
            | Self::SendText { .. }
            | Self::Scroll { .. }
            | Self::Emit { .. } => Effect::Write,
            Self::RunCommand { .. }
            | Self::Open(_)
            | Self::SetFontSize(_)
            | Self::OpenWindow
            | Self::CloseWindow(_)
            | Self::Quit => Effect::App,
            Self::RegisterCommand(_) | Self::Bind(_) | Self::Subscribe(_) => Effect::Declaration,
        }
    }
}

/// The host's answer to a request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Response {
    /// A write queued, a request of the app's taken, or a declaration
    /// taken.
    Done,
    /// A window, or none.
    Window(Option<WindowId>),
    /// A pane, or none.
    Pane(Option<PaneId>),
    /// A text, or none.
    Text(Option<String>),
    /// A line's text, or none.
    Line(Option<LineText>),
    /// Whether it holds.
    Boolean(bool),
    /// The text's size, in points.
    FontSize(f32),
    /// The request failed, and did nothing.
    Failed(Error),
}

/// What the host tells a plugin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Event {
    /// A change of the workspace's windows, tabs, layouts or panes.
    Workspace(WorkspaceEvent),
    /// An event of a plugin's own.
    Custom {
        /// Its name, `namespace.event`, of the plugin's namespace.
        name: String,
        /// The id of its value's schema.
        schema: String,
        /// Its value.
        value: Data,
    },
}

impl Event {
    /// The event's name, as a plugin subscribes to it.
    pub fn name(&self) -> &str {
        match self {
            Self::Workspace(event) => event.name(),
            Self::Custom { name, .. } => name,
        }
    }
}

/// A plain value, as an event of a plugin's carries: text, numbers, true
/// or false, and lists and maps of them, as a Luau table is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Data {
    /// Nothing.
    Nil,
    /// True or false.
    Boolean(bool),
    /// A whole number.
    Integer(i64),
    /// A number.
    Number(f64),
    /// Text.
    Text(String),
    /// Values in their order.
    List(Vec<Data>),
    /// Values by their keys.
    Map(BTreeMap<String, Data>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{Category, Code, Value};

    #[test]
    fn the_messages_cross_as_data_and_come_back_the_same() {
        let calls = [
            Request::RegisterCommand(Command {
                name: "x.y".to_owned(),
                title: "Y".to_owned(),
                category: Category::View,
                capabilities: vec![Capability::Net("lokyr.dev".to_owned())],
                params: Vec::new(),
                when: None,
            }),
            Request::RunCommand {
                name: "x.y".to_owned(),
                args: Args::from([("n".to_owned(), Value::Integer(3))]),
            },
            Request::LineText {
                pane: PaneId(4),
                row: -2,
                reach: 32,
            },
            Request::Scroll {
                pane: PaneId(4),
                scroll: Scroll::Up(3),
            },
            Request::SetFontSize(None),
            Request::Quit,
        ]
        .map(|request| Call {
            caller: "links".to_owned(),
            request,
        });
        for call in calls {
            let text = serde_json::to_string(&call).unwrap();
            assert_eq!(serde_json::from_str::<Call>(&text).unwrap(), call, "{text}");
        }
        let responses = [
            Response::Done,
            Response::Line(Some(LineText {
                text: "日".to_owned(),
                cells: vec![(0, 7)],
            })),
            Response::Failed(Error::new(Code::NotFound, "there is no pane 4")),
        ];
        for response in responses {
            let text = serde_json::to_string(&response).unwrap();
            assert_eq!(serde_json::from_str::<Response>(&text).unwrap(), response);
        }
        let event = Event::Workspace(WorkspaceEvent::LayoutChanged {
            tab: crate::host::TabId(9),
        });
        let text = serde_json::to_string(&event).unwrap();
        assert_eq!(text, r#"{"workspace":{"layout-changed":{"tab":9}}}"#);
        assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), event);
        assert_eq!(event.name(), "layout-changed");
        let custom = Event::Custom {
            name: "search.progress".to_owned(),
            schema: "search.progress/1".to_owned(),
            value: Data::Map(BTreeMap::from([
                ("matches".to_owned(), Data::Integer(12)),
                ("done".to_owned(), Data::Boolean(false)),
                (
                    "panes".to_owned(),
                    Data::List(vec![
                        Data::Integer(4),
                        Data::Nil,
                        Data::Text("x".to_owned()),
                    ]),
                ),
            ])),
        };
        let text = serde_json::to_string(&custom).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&text).unwrap(), custom);
        assert_eq!(custom.name(), "search.progress");
    }

    #[test]
    fn each_request_needs_its_capability_and_has_its_effect() {
        let pane = PaneId(1);
        assert_eq!(
            Request::ClipboardText.capability(),
            Some(Capability::ClipboardRead)
        );
        assert_eq!(
            Request::SendText {
                pane,
                text: String::new()
            }
            .capability(),
            Some(Capability::PtyWrite)
        );
        assert_eq!(Request::SelectAll(pane).capability(), None);
        assert_eq!(Request::SelectionText(pane).effect(), Effect::Read);
        assert_eq!(
            Request::SetClipboardText(String::new()).effect(),
            Effect::Write
        );
        assert_eq!(Request::Open(String::new()).effect(), Effect::App);
        assert_eq!(
            Request::Bind(Binding {
                keys: "cmd+c".to_owned(),
                command: "x.y".to_owned(),
                args: Args::new(),
                when: None,
            })
            .effect(),
            Effect::Declaration
        );
    }
}
