//! The automation's requests, which scripts and shortcuts make of the app
//! from outside it, as the host hands them to the plugin of the capability
//! `automation` in [`Event::Automation`](crate::message::Event::Automation),
//! and its answers, which it gives back by
//! [`Request::AnswerAutomation`](crate::message::Request::AnswerAutomation).
//! The windows, the tabs and the panes go by the host's ids, which stay the
//! same between a script's requests.

use serde::{Deserialize, Serialize};

use crate::command::{Args, Code, Error};
use crate::host::{PaneId, TabId, WindowId};

/// The longest text of a request, a command line, a folder or text sent,
/// and of a text answered, in bytes.
pub const LONGEST_TEXT: usize = 1 << 20;

/// How long the host waits for the plugin's answer before it fails the
/// request, in milliseconds.
pub const ANSWER_WITHIN_MS: u64 = 10_000;

/// What a script or a shortcut asks of the app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomationRequest {
    /// The windows, each of its tabs, and each tab of its panes.
    Workspace,
    /// Opens a window, its pane in `folder` if given, which runs `command`
    /// at its shell's start if given.
    NewWindow {
        /// The folder of its pane.
        #[serde(default)]
        folder: Option<String>,
        /// The command line that its shell runs as it starts.
        #[serde(default)]
        command: Option<String>,
    },
    /// Opens a tab in `window`, or in the window in front, as a new tab's
    /// pane starts, in `folder` and running `command` as a window's.
    NewTab {
        /// The window.
        #[serde(default)]
        window: Option<WindowId>,
        /// The folder of its pane.
        #[serde(default)]
        folder: Option<String>,
        /// The command line that its shell runs as it starts.
        #[serde(default)]
        command: Option<String>,
    },
    /// Splits `pane`, or the focused one, with a new pane at `edge`, in
    /// `folder` and running `command` as a window's.
    Split {
        /// The pane.
        #[serde(default)]
        pane: Option<PaneId>,
        /// Where the new pane goes.
        edge: AutomationEdge,
        /// The folder of its pane.
        #[serde(default)]
        folder: Option<String>,
        /// The command line that its shell runs as it starts.
        #[serde(default)]
        command: Option<String>,
    },
    /// Sends `text` to `pane`, or to the focused one, as typed.
    SendText {
        /// The pane.
        #[serde(default)]
        pane: Option<PaneId>,
        /// The text.
        text: String,
    },
    /// Brings a window, a tab or a pane forward.
    Focus(AutomationTarget),
    /// Closes a window, a tab or a pane.
    Close(AutomationTarget),
    /// Runs a command of the registry by its name and arguments, as the
    /// user's own, as the palette runs it.
    Run {
        /// The command's name, as `splits.split`.
        command: String,
        /// Its arguments, by their names.
        #[serde(default)]
        args: Args,
    },
    /// The text of `pane`, or of the focused one: its screen, or its
    /// selection.
    PaneText {
        /// The pane.
        #[serde(default)]
        pane: Option<PaneId>,
        /// Its selection, rather than its screen.
        #[serde(default)]
        selection: bool,
    },
    /// Switches to the session of `name`, the one used last of several,
    /// making it first if there is none of that name.
    SwitchSession {
        /// The session's name, not empty.
        name: String,
    },
    /// The commands of the registry, each of its name and title.
    Commands,
}

impl AutomationRequest {
    /// Whether its texts are within [`LONGEST_TEXT`], or the error.
    pub fn check(&self) -> Result<(), Error> {
        let texts: Vec<&str> = match self {
            Self::NewWindow { folder, command }
            | Self::NewTab {
                folder, command, ..
            }
            | Self::Split {
                folder, command, ..
            } => [folder, command]
                .into_iter()
                .filter_map(|text| text.as_deref())
                .collect(),
            Self::SendText { text, .. } => vec![text.as_str()],
            Self::Run { command, .. } => vec![command.as_str()],
            Self::SwitchSession { name } => vec![name.as_str()],
            Self::Workspace
            | Self::Focus(_)
            | Self::Close(_)
            | Self::PaneText { .. }
            | Self::Commands => Vec::new(),
        };
        if texts.iter().any(|text| text.len() > LONGEST_TEXT) {
            return Err(Error::new(
                Code::WrongKind,
                format!("an automation's text is longer than {LONGEST_TEXT} bytes"),
            ));
        }
        Ok(())
    }
}

/// Where a split's new pane goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomationEdge {
    /// On the right.
    Right,
    /// On the left.
    Left,
    /// Below.
    Down,
    /// Above.
    Up,
}

/// A window, a tab or a pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomationTarget {
    /// A window.
    Window(WindowId),
    /// A tab.
    Tab(TabId),
    /// A pane.
    Pane(PaneId),
}

/// The plugin's answer to a request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AutomationAnswer {
    /// Done, of nothing to tell.
    Done,
    /// What a window, a tab or a split opened.
    Opened {
        /// The window.
        window: WindowId,
        /// The tab.
        tab: TabId,
        /// The pane.
        pane: PaneId,
    },
    /// The windows, in their order.
    Workspace(Vec<AutomationWindow>),
    /// A text.
    Text(String),
    /// The commands, in the registry's order.
    Commands(Vec<AutomationCommand>),
    /// Refused or failed, of the registry's code and its message.
    Error {
        /// The code's number.
        code: u16,
        /// What went wrong.
        message: String,
    },
}

impl AutomationAnswer {
    /// The answer of `error`.
    pub fn of_error(error: &Error) -> Self {
        Self::Error {
            code: error.code.number(),
            message: error.message.clone(),
        }
    }
}

/// A window, as the automation tells it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationWindow {
    /// Its id.
    pub id: WindowId,
    /// Its title, as its titlebar shows it.
    pub title: String,
    /// Whether it is the window in front.
    pub focused: bool,
    /// Its tabs, in their order.
    pub tabs: Vec<AutomationTab>,
}

/// A tab, as the automation tells it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationTab {
    /// Its id.
    pub id: TabId,
    /// Its title.
    pub title: String,
    /// Whether its window shows it.
    pub active: bool,
    /// Its panes, in their layout's order.
    pub panes: Vec<AutomationPane>,
}

/// A command of the registry, as the automation tells it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationCommand {
    /// Its name, as `tabs.new`.
    pub name: String,
    /// Its title, as the menus show it.
    pub title: String,
}

/// A pane, as the automation tells it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationPane {
    /// Its id.
    pub id: PaneId,
    /// Its title.
    pub title: String,
    /// The folder that its program works in, as it told it.
    pub folder: Option<String>,
    /// Whether it has its tab's keyboard.
    pub focused: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_s_texts_are_bounded() {
        let long = "x".repeat(LONGEST_TEXT + 1);
        assert!(AutomationRequest::Workspace.check().is_ok());
        assert!(
            AutomationRequest::SendText {
                pane: None,
                text: long.clone()
            }
            .check()
            .is_err()
        );
        assert!(
            AutomationRequest::NewTab {
                window: None,
                folder: Some(long),
                command: None
            }
            .check()
            .is_err()
        );
        assert!(
            AutomationRequest::Run {
                command: "window.new".to_owned(),
                args: Args::new()
            }
            .check()
            .is_ok()
        );
    }

    #[test]
    fn an_error_s_answer_tells_its_code_s_number() {
        let error = Error::new(Code::NotFound, "there is no pane 7");
        assert_eq!(
            AutomationAnswer::of_error(&error),
            AutomationAnswer::Error {
                code: Code::NotFound.number(),
                message: "there is no pane 7".to_owned()
            }
        );
    }
}
