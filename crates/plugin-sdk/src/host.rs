//! The host's side: what carries a plugin's calls to the host, the
//! handles of the windows, the tabs and the panes that it reaches, and the
//! types that the requests carry.
//!
//! Only data crosses, never references into the host's state. What
//! changes, the kernel's state or the app, comes once the plugin's call
//! returns, as the host never calls into a plugin that is running.

use serde::{Deserialize, Serialize};

use crate::command::Args;
use crate::message::{Call, Response};

/// A window, by the host's id, which no other window, tab or pane of the
/// process has had.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WindowId(pub u64);

/// A tab, by the host's id, likewise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TabId(pub u64);

/// A pane, by the host's id, likewise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PaneId(pub u64);

/// A binding of keys to a command, as a plugin asks for it. The user's
/// bindings, in the config, come after the plugins', and win over them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    /// The keys, as the config writes them: `cmd+c`, or a sequence, as
    /// `ctrl+a>c`.
    pub keys: String,
    /// The command's name.
    pub command: String,
    /// Its arguments.
    pub args: Args,
    /// The context in which it holds, as `pane.focused && !selection`;
    /// every context without one.
    pub when: Option<String>,
}

/// The text of a logical line of a pane's view, its rows that soft-wrap
/// joined, with the cell of each of its characters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineText {
    /// The text: a cell without a character is a space.
    pub text: String,
    /// The cell of each of the text's characters, in order: its column,
    /// and its row's number.
    pub cells: Vec<(u16, i64)>,
}

/// The most rows before a row and after it that a read of its logical line
/// takes, so that a line of many rows costs no more.
pub const MOST_REACH: usize = 256;

/// How a pane's view scrolls through its scrollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Scroll {
    /// Up into the scrollback, by rows.
    Up(u32),
    /// Down toward the screen, by rows.
    Down(u32),
    /// Up by the screen's rows.
    PageUp,
    /// Down by the screen's rows.
    PageDown,
    /// To the scrollback's first row.
    Top,
    /// To the screen, which the view then follows.
    Bottom,
}

/// What carries a plugin's calls to the host, and the host's answers
/// back: the host itself, for a plugin on the main thread, or a channel's
/// end, for one on a thread of its own or in a VM, whose calls cross as
/// data.
pub trait Host {
    /// Answers `call`: a read with the host's state as it handles the call,
    /// a write once it queued it, which it applies as the plugin's call
    /// returns, a request that the caller may not make, or of a window, a
    /// tab or a pane that is not there, with its error.
    fn handle(&mut self, call: Call) -> Response;
}
