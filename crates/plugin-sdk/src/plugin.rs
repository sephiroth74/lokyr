//! The plugins' side: the trait that a plugin implements, and the context
//! through which its callbacks reach the host, whose methods send the
//! host [requests](crate::message::Request) and read its responses.
//!
//! ```
//! use lokyr_plugin_sdk::command::{Args, Category, Code, Command, Error};
//! use lokyr_plugin_sdk::host::Binding;
//! use lokyr_plugin_sdk::plugin::{Cx, Plugin};
//!
//! #[derive(Default)]
//! struct Copying;
//!
//! impl Plugin for Copying {
//!     fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
//!         cx.register_command(Command {
//!             name: "selection.copy".to_owned(),
//!             title: "Copy".to_owned(),
//!             category: Category::Clipboard,
//!             capabilities: Vec::new(),
//!             params: Vec::new(),
//!             when: Some("selection".to_owned()),
//!         })?;
//!         cx.bind(Binding {
//!             keys: "cmd+c".to_owned(),
//!             command: "selection.copy".to_owned(),
//!             args: Args::new(),
//!             when: Some("selection".to_owned()),
//!         })
//!     }
//!
//!     fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, _args: Args) -> Result<(), Error> {
//!         match name {
//!             "selection.copy" => {
//!                 let Some(pane) = cx.focused_pane() else {
//!                     return Ok(());
//!                 };
//!                 if let Some(text) = cx.selection_text(pane)? {
//!                     cx.set_clipboard_text(&text)?;
//!                 }
//!                 Ok(())
//!             }
//!             _ => Err(Error::new(Code::NotFound, format!("no command `{name}`"))),
//!         }
//!     }
//! }
//! ```

use crate::command::{Args, Code, Command, Error};
use crate::host::{Binding, Host, LineText, PaneId, Scroll, WindowId};
use crate::message::{Call, Request, Response};

/// A plugin: its activation, which registers its commands and binds its
/// keys, and the calls of its commands, which the host checked against
/// their parameters, with their defaults.
///
/// The host calls it on the main thread, and never while it runs: what its
/// calls ask of the app comes once they return. A call that panics
/// disables the plugin, and takes its commands and its bindings away.
pub trait Plugin {
    /// Starts the plugin.
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        let _ = cx;
        Ok(())
    }

    /// Runs `name`, one of the commands that it registered, with `args`.
    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        let _ = (cx, args);
        Err(Error::new(
            Code::NotFound,
            format!("the plugin has no command `{name}`"),
        ))
    }

    /// Its catalogs of [Project Fluent](https://projectfluent.org), each by
    /// its language, as `("it", source)`, English's the source of the
    /// others: the host titles its commands in the language shown with
    /// their messages, `command-` and the command's name, with `-` for its
    /// `.`, as `command-pane-copy`; a title that they lack is the one that
    /// the command registered. By default, none.
    fn catalogs(&self) -> &'static [(&'static str, &'static str)] {
        &[]
    }

    /// Whether `name`, one of the commands that it registered, would do
    /// something now with `args`, which the host checked, as a menu asks
    /// before it shows the command's item enabled or dimmed: by default, it
    /// would. What the call asks of the app is dropped, as it only looks.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        let _ = (cx, name, args);
        true
    }
}

/// What a plugin's callbacks reach the host through: each method sends a
/// request, as the plugin, and reads the host's response. A read answers
/// with the host's state as it handles the request; a write is queued, and
/// applied as the plugin's call returns, so that a read in the same call
/// reads the state before it.
pub struct Cx<'a> {
    host: &'a mut dyn Host,
    caller: String,
}

impl<'a> Cx<'a> {
    /// The context of a callback of the plugin `caller`, on `host`.
    pub fn new(host: &'a mut dyn Host, caller: &str) -> Self {
        Self {
            host,
            caller: caller.to_owned(),
        }
    }

    /// Sends `request`, and returns the host's response.
    pub fn request(&mut self, request: Request) -> Response {
        self.host.handle(Call {
            caller: self.caller.clone(),
            request,
        })
    }

    /// Sends `request`, which the host does, or queues.
    fn done(&mut self, request: Request) -> Result<(), Error> {
        match self.request(request) {
            Response::Done => Ok(()),
            other => Err(failed(other)),
        }
    }

    /// Sends `request`, which a text answers.
    fn text(&mut self, request: Request) -> Result<Option<String>, Error> {
        match self.request(request) {
            Response::Text(text) => Ok(text),
            other => Err(failed(other)),
        }
    }

    /// Registers `command`, whose calls come to the plugin's
    /// [`on_command`](Plugin::on_command).
    pub fn register_command(&mut self, command: Command) -> Result<(), Error> {
        self.done(Request::RegisterCommand(command))
    }

    /// Runs the command `name` with `args`, once this call of the plugin
    /// returns: the call is checked now, and the command's own failure goes
    /// to the log.
    pub fn run_command(&mut self, name: &str, args: Args) -> Result<(), Error> {
        self.done(Request::RunCommand {
            name: name.to_owned(),
            args,
        })
    }

    /// Binds keys to a command, which needs not be registered yet: its keys
    /// tell the log of it, as they are pressed, until it is.
    pub fn bind(&mut self, binding: Binding) -> Result<(), Error> {
        self.done(Request::Bind(binding))
    }

    /// The clipboard's text, if it holds any.
    pub fn clipboard_text(&mut self) -> Result<Option<String>, Error> {
        self.text(Request::ClipboardText)
    }

    /// Puts `text` on the clipboard, as this call returns.
    pub fn set_clipboard_text(&mut self, text: &str) -> Result<(), Error> {
        self.done(Request::SetClipboardText(text.to_owned()))
    }

    /// The window that has the focus.
    pub fn focused_window(&mut self) -> Option<WindowId> {
        match self.request(Request::FocusedWindow) {
            Response::Window(window) => window,
            _ => None,
        }
    }

    /// The pane that has the focus: the active one of the focused window.
    pub fn focused_pane(&mut self) -> Option<PaneId> {
        match self.request(Request::FocusedPane) {
            Response::Pane(pane) => pane,
            _ => None,
        }
    }

    /// The text of `pane`'s selection, if it has one.
    pub fn selection_text(&mut self, pane: PaneId) -> Result<Option<String>, Error> {
        self.text(Request::SelectionText(pane))
    }

    /// The text of the logical line that `pane`'s row numbered `row` is in,
    /// of the screen or of its scrollback, as far as `reach` rows before it
    /// and after it, [`MOST_REACH`](crate::host::MOST_REACH) at most, with
    /// each character's cell; `None` for a row that the pane does not hold.
    ///
    /// A row's number stays its own as output scrolls it into the
    /// scrollback and the view scrolls, until the scrollback's limit takes
    /// it away; a resize, which lays the rows out again, numbers them
    /// anew. A command of a cell, as a click's, takes the number of the
    /// row that the click was on, as the drawn frame showed it.
    pub fn line_text(
        &mut self,
        pane: PaneId,
        row: i64,
        reach: usize,
    ) -> Result<Option<LineText>, Error> {
        match self.request(Request::LineText { pane, row, reach }) {
            Response::Line(line) => Ok(line),
            other => Err(failed(other)),
        }
    }

    /// The target of the OSC 8 link that the cell of `pane` at `column` of
    /// the row numbered `row` is part of, if it is one's, as
    /// [`Cx::line_text`] numbers the rows.
    pub fn link_at(
        &mut self,
        pane: PaneId,
        column: u16,
        row: i64,
    ) -> Result<Option<String>, Error> {
        self.text(Request::LinkAt { pane, column, row })
    }

    /// The working directory of the program in `pane`'s foreground, as a
    /// shell's at its prompt.
    pub fn working_directory(&mut self, pane: PaneId) -> Result<Option<String>, Error> {
        self.text(Request::WorkingDirectory(pane))
    }

    /// Opens `target`, once this call returns, with the system's handler:
    /// a URL of `http`, `https`, `mailto` or `file`, or a path, absolute or
    /// from `~`, a file in its application and a directory in the file
    /// manager. Another scheme, or a relative path, fails. As the user's
    /// config says, a URL or a file asks the user first, showing it, and a
    /// file can show in the file manager instead; a file that would run
    /// never does.
    pub fn open(&mut self, target: &str) -> Result<(), Error> {
        self.done(Request::Open(target.to_owned()))
    }

    /// Whether [`open`](Self::open) would open `target`: a URL of its
    /// schemes, or a path, absolute or from `~`, that is there.
    pub fn opens(&mut self, target: &str) -> Result<bool, Error> {
        match self.request(Request::Opens(target.to_owned())) {
            Response::Boolean(opens) => Ok(opens),
            other => Err(failed(other)),
        }
    }

    /// Selects everything of `pane`, its scrollback and its screen, as
    /// this call returns.
    pub fn select_all(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::SelectAll(pane))
    }

    /// Clears `pane`'s scrollback, as this call returns.
    pub fn clear_scrollback(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::ClearScrollback(pane))
    }

    /// Clears `pane`'s scrollback, and its screen but the prompt that the
    /// cursor is in, which goes to the top, as this call returns; the
    /// program is told nothing.
    pub fn clear(&mut self, pane: PaneId) -> Result<(), Error> {
        self.done(Request::Clear(pane))
    }

    /// Pastes `text` into `pane`, as the user pastes, as this call returns:
    /// bracketed when its program asks, without its control characters but
    /// tab and the line breaks. Into a program that does not ask, a text of
    /// several lines, whose line breaks reach it as Return, waits for the
    /// user to confirm it, and a single line pastes without the line break
    /// after it; the user's config can ask always, or never.
    pub fn paste(&mut self, pane: PaneId, text: &str) -> Result<(), Error> {
        self.done(Request::Paste {
            pane,
            text: text.to_owned(),
        })
    }

    /// Sends `text` to `pane`'s program as it is, as this call returns, its
    /// control characters too, as typed keys would, which can run
    /// commands; the view comes back to the screen.
    pub fn send_text(&mut self, pane: PaneId, text: &str) -> Result<(), Error> {
        self.done(Request::SendText {
            pane,
            text: text.to_owned(),
        })
    }

    /// Scrolls `pane`'s view, as this call returns.
    pub fn scroll(&mut self, pane: PaneId, scroll: Scroll) -> Result<(), Error> {
        self.done(Request::Scroll { pane, scroll })
    }

    /// The size of the text, in points, of every window.
    pub fn font_size(&mut self) -> Result<f32, Error> {
        match self.request(Request::FontSize) {
            Response::FontSize(size) => Ok(size),
            other => Err(failed(other)),
        }
    }

    /// Gives the text `size`, in points, from 4 to 200, once this call
    /// returns; with `None`, the config's.
    pub fn set_font_size(&mut self, size: Option<f32>) -> Result<(), Error> {
        self.done(Request::SetFontSize(size))
    }

    /// Opens a window, once this call returns.
    pub fn open_window(&mut self) -> Result<(), Error> {
        self.done(Request::OpenWindow)
    }

    /// Closes `window`, once this call returns.
    pub fn close_window(&mut self, window: WindowId) -> Result<(), Error> {
        self.done(Request::CloseWindow(window))
    }

    /// Quits the app, once this call returns.
    pub fn quit(&mut self) -> Result<(), Error> {
        self.done(Request::Quit)
    }
}

/// The error of `response`, which failed, or answered another request.
fn failed(response: Response) -> Error {
    match response {
        Response::Failed(error) => error,
        other => Error::new(
            Code::Fault,
            format!("the host answered another request: {other:?}"),
        ),
    }
}
