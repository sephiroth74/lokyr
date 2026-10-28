//! The host's API: what a plugin asks of the terminal, and the handles of
//! the windows and the panes that it reaches.
//!
//! Only data crosses it, never references into the host's state. What
//! changes the app, as opening a window or running another command, comes
//! once the plugin's call returns, as the host never calls into a plugin
//! that is running.

use crate::command::{Args, Command, Error};

/// A window, by the host's id, which no other window, tab or pane of the
/// process has had.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub u64);

/// A pane, by the host's id, likewise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaneId(pub u64);

/// A binding of keys to a command, as a plugin asks for it. The user's
/// bindings, in the config, come after the plugins', and win over them.
#[derive(Clone, Debug, PartialEq)]
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

/// How a pane's view scrolls through its scrollback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// What the host gives a plugin: the commands, the keys, the clipboard,
/// the panes, the font's size and the windows.
///
/// Each call that can be refused returns its error, as one that a plugin
/// lacks the capability for: a plugin can treat a capability as optional.
pub trait HostApi {
    /// Registers `command`, whose calls come to the plugin's
    /// [`on_command`](crate::plugin::Plugin::on_command).
    fn register_command(&mut self, command: Command) -> Result<(), Error>;

    /// Runs the command `name` with `args`, once this call of the plugin
    /// returns: the call is checked now, and the command's own failure goes
    /// to the log.
    fn run_command(&mut self, name: &str, args: Args) -> Result<(), Error>;

    /// Binds keys to a command, which needs not be registered yet: its keys
    /// tell the log of it, as they are pressed, until it is.
    fn bind(&mut self, binding: Binding) -> Result<(), Error>;

    /// The clipboard's text, if it holds any.
    fn clipboard_text(&mut self) -> Result<Option<String>, Error>;

    /// Puts `text` on the clipboard.
    fn set_clipboard_text(&mut self, text: &str) -> Result<(), Error>;

    /// The window that has the focus.
    fn focused_window(&self) -> Option<WindowId>;

    /// The pane that has the focus: the active one of the focused window.
    fn focused_pane(&self) -> Option<PaneId>;

    /// The text of `pane`'s selection, if it has one.
    fn selection_text(&mut self, pane: PaneId) -> Result<Option<String>, Error>;

    /// Selects everything of `pane`: its scrollback and its screen.
    fn select_all(&mut self, pane: PaneId) -> Result<(), Error>;

    /// Clears `pane`'s scrollback.
    fn clear_scrollback(&mut self, pane: PaneId) -> Result<(), Error>;

    /// Clears `pane`'s scrollback, and its screen but the prompt that the
    /// cursor is in, which goes to the top; the program is told nothing.
    fn clear(&mut self, pane: PaneId) -> Result<(), Error>;

    /// Pastes `text` into `pane`, as the user pastes: bracketed when its
    /// program asks, without its control characters but tab and the line
    /// breaks. Into a program that does not ask, a text with a line break,
    /// which reaches it as Return, waits for the user to confirm it.
    fn paste(&mut self, pane: PaneId, text: &str) -> Result<(), Error>;

    /// Sends `text` to `pane`'s program as it is, its control characters
    /// too, as typed keys would, which can run commands; the view comes
    /// back to the screen.
    fn send_text(&mut self, pane: PaneId, text: &str) -> Result<(), Error>;

    /// Scrolls `pane`'s view.
    fn scroll(&mut self, pane: PaneId, scroll: Scroll) -> Result<(), Error>;

    /// The size of the text, in points, of every window.
    fn font_size(&self) -> f32;

    /// Gives the text `size`, in points, from 4 to 200; with `None`, the
    /// config's.
    fn set_font_size(&mut self, size: Option<f32>) -> Result<(), Error>;

    /// Opens a window.
    fn open_window(&mut self) -> Result<(), Error>;

    /// Closes `window`.
    fn close_window(&mut self, window: WindowId) -> Result<(), Error>;

    /// Quits the app.
    fn quit(&mut self) -> Result<(), Error>;
}
