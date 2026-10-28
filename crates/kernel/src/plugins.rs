//! The plugins that the app runs, built in, and the host that they reach
//! the app through. Each call of a plugin runs within `catch_unwind`, so
//! that one that panics is disabled, with its commands and its bindings,
//! rather than taking the app down.
//!
//! A plugin's calls change the kernel's state at once, as a pane's
//! selection; what changes the app, as a window to open or another command
//! to run, comes as a request, which the app takes once the plugin's call
//! returns, as a plugin is never called while it runs.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

pub use lokyr_plugin_sdk::host::{self, HostApi};
pub use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_text_core::Size;
use lokyr_vt_core::terminal::Scroll;

use crate::commands::{self, Args, Code, Command, Error, Registry};
use crate::config::ConfirmPaste;
use crate::keymap::{self, Binding, Invocation, When};
use crate::paste::{self, Paste};
use crate::workspace::{Pane, PaneId, WindowId, Workspace};

/// What a plugin asks of the app, which the app does once the plugin's
/// call returns.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Run a command, whose call the registry checked.
    Run(Invocation),
    /// Open a window.
    OpenWindow,
    /// Close a window.
    CloseWindow(WindowId),
    /// Quit.
    Quit,
    /// Give the text a size, in points; with `None`, the config's.
    FontSize(Option<f32>),
    /// Ask the user to confirm a paste of `text` into `pane`, whose
    /// program takes its line breaks as Return.
    ConfirmPaste {
        /// The pane.
        pane: PaneId,
        /// What to paste.
        text: String,
    },
}

/// The clipboard, which the app reaches through its platform.
pub trait Clipboard {
    /// Its text, if it holds any.
    fn text(&self) -> Option<String>;

    /// Puts `text` on it.
    fn set_text(&mut self, text: &str);
}

/// What the host reaches of the app during a plugin's call.
pub struct Context<'a> {
    /// The commands.
    pub registry: &'a mut Registry,
    /// The windows, the tabs and the panes.
    pub workspace: &'a mut Workspace,
    /// The clipboard.
    pub clipboard: &'a mut dyn Clipboard,
    /// The window that has the focus.
    pub focused: Option<WindowId>,
    /// The text's size, in points.
    pub font_size: f32,
    /// When a paste with line breaks asks the user first.
    pub confirm_paste: ConfirmPaste,
}

/// Where a plugin is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Added, and not activated yet.
    Inactive,
    /// Activated: its commands run, and its keys are bound.
    Active,
    /// Stopped, as its activation failed or a call panicked: it has no
    /// commands nor bindings.
    Disabled,
}

/// A plugin, and what the host keeps of it.
struct Entry {
    plugin: Box<dyn Plugin>,
    state: State,
}

/// What the host keeps of a plugin: its name, its status, and the commands
/// and the bindings that it gave.
struct State {
    name: String,
    status: Status,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
}

/// The plugins, in the order of their activation, and what their calls
/// asked of the app.
#[derive(Default)]
pub struct Plugins {
    entries: Vec<Entry>,
    /// The requests of the calls since the last take.
    requests: Vec<Request>,
    /// Whether the plugins' bindings changed since the last look.
    bindings_changed: bool,
}

impl std::fmt::Debug for Plugins {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(
                self.entries
                    .iter()
                    .map(|entry| (&entry.state.name, entry.state.status)),
            )
            .finish()
    }
}

impl Plugins {
    /// No plugins.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `plugin`, by its `name`, to activate.
    pub fn add(&mut self, name: &str, plugin: Box<dyn Plugin>) {
        self.entries.push(Entry {
            plugin,
            state: State {
                name: name.to_owned(),
                status: Status::Inactive,
                commands: Vec::new(),
                bindings: Vec::new(),
            },
        });
    }

    /// Activates the plugins that are not active yet, in their order, and
    /// returns the errors of those whose activation failed or panicked,
    /// which are disabled.
    pub fn activate(&mut self, context: &mut Context<'_>) -> Vec<Error> {
        let mut errors = Vec::new();
        for entry in &mut self.entries {
            if entry.state.status != Status::Inactive {
                continue;
            }
            entry.state.status = Status::Active;
            let activated = call(
                entry,
                context,
                &mut self.requests,
                &mut self.bindings_changed,
                |plugin, cx| plugin.activate(cx),
            );
            if let Err(error) = activated {
                if entry.state.status == Status::Active {
                    disable(
                        &mut entry.state,
                        context.registry,
                        &mut self.bindings_changed,
                    );
                    errors.push(Error::new(
                        error.code,
                        format!(
                            "the plugin `{}` did not start, and is disabled: {}",
                            entry.state.name, error.message
                        ),
                    ));
                } else {
                    errors.push(error);
                }
            }
        }
        errors
    }

    /// Whether an active plugin registered `command`.
    pub fn owns(&self, command: &str) -> bool {
        self.owner(command).is_some()
    }

    fn owner(&self, command: &str) -> Option<usize> {
        self.entries.iter().position(|entry| {
            entry.state.status == Status::Active
                && entry.state.commands.iter().any(|ours| ours.name == command)
        })
    }

    /// Runs `command`, which an active plugin registered, with `args`,
    /// which the registry checked: the plugin's own error, or a fault when
    /// it panicked, which disables it.
    pub fn run(
        &mut self,
        command: &str,
        args: Args,
        context: &mut Context<'_>,
    ) -> Result<(), Error> {
        let Some(at) = self.owner(command) else {
            return Err(Error::new(
                Code::NotFound,
                format!("no plugin runs `{command}`"),
            ));
        };
        call(
            &mut self.entries[at],
            context,
            &mut self.requests,
            &mut self.bindings_changed,
            |plugin, cx| plugin.on_command(cx, command, args),
        )
    }

    /// The bindings of the active plugins, in their order, which the
    /// keymap takes after the app's and before the config's.
    pub fn bindings(&self) -> impl Iterator<Item = &Binding> {
        self.active().flat_map(|state| &state.bindings)
    }

    /// The commands of the active plugins, which a registry made anew, as
    /// for another language, takes again.
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.active().flat_map(|state| &state.commands)
    }

    fn active(&self) -> impl Iterator<Item = &State> {
        self.entries
            .iter()
            .map(|entry| &entry.state)
            .filter(|state| state.status == Status::Active)
    }

    /// The status of the plugin `name`, if it was added.
    pub fn status(&self, name: &str) -> Option<Status> {
        self.entries
            .iter()
            .find(|entry| entry.state.name == name)
            .map(|entry| entry.state.status)
    }

    /// Takes what the plugins' calls asked of the app since the last take.
    pub fn take_requests(&mut self) -> Vec<Request> {
        std::mem::take(&mut self.requests)
    }

    /// Whether the plugins' bindings changed since the last call, as a
    /// plugin bound keys or was disabled.
    pub fn take_bindings_changed(&mut self) -> bool {
        std::mem::take(&mut self.bindings_changed)
    }
}

/// Calls `call` on `entry`'s plugin, within `catch_unwind`: a plugin that
/// panics is disabled, and the requests of the call go.
fn call(
    entry: &mut Entry,
    context: &mut Context<'_>,
    requests: &mut Vec<Request>,
    bindings_changed: &mut bool,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<(), Error>,
) -> Result<(), Error> {
    let before = requests.len();
    let Entry { plugin, state } = entry;
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut host = Host {
            state: &mut *state,
            context: &mut *context,
            requests: &mut *requests,
            bindings_changed: &mut *bindings_changed,
        };
        call(plugin.as_mut(), &mut Cx::new(&mut host))
    }));
    outcome.unwrap_or_else(|payload| {
        requests.truncate(before);
        disable(state, context.registry, bindings_changed);
        Err(Error::new(
            Code::Fault,
            format!(
                "the plugin `{}` panicked{}; it is disabled",
                state.name,
                message(payload.as_ref())
            ),
        ))
    })
}

/// `: ` and the message of a panic's `payload`, when it has one.
fn message(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|message| (*message).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .map(|message| format!(": {message}"))
        .unwrap_or_default()
}

/// Disables the plugin of `state`: its commands leave `registry`, and its
/// bindings go.
fn disable(state: &mut State, registry: &mut Registry, bindings_changed: &mut bool) {
    state.status = Status::Disabled;
    for command in state.commands.drain(..) {
        registry.unregister(&command.name);
    }
    if !state.bindings.is_empty() {
        state.bindings.clear();
        *bindings_changed = true;
    }
}

/// The host of one plugin's call.
struct Host<'h, 'c> {
    state: &'h mut State,
    context: &'h mut Context<'c>,
    requests: &'h mut Vec<Request>,
    bindings_changed: &'h mut bool,
}

impl Host<'_, '_> {
    /// The pane of `pane`'s handle.
    fn pane(&self, pane: host::PaneId) -> Result<&Pane, Error> {
        self.context
            .workspace
            .pane(PaneId::of(pane.0))
            .ok_or_else(|| Error::new(Code::Failed, format!("there is no pane {}", pane.0)))
    }
}

impl HostApi for Host<'_, '_> {
    fn register_command(&mut self, command: Command) -> Result<(), Error> {
        self.context.registry.register(command.clone())?;
        self.state.commands.push(command);
        Ok(())
    }

    fn run_command(&mut self, name: &str, args: Args) -> Result<(), Error> {
        let args = self.context.registry.check(name, args)?;
        self.requests.push(Request::Run(Invocation {
            command: name.to_owned(),
            args,
        }));
        Ok(())
    }

    fn bind(&mut self, binding: host::Binding) -> Result<(), Error> {
        let invalid = |message: String| Error::new(Code::InvalidBinding, message);
        let keys = keymap::parse_keys(&binding.keys).map_err(invalid)?;
        if !commands::is_name(&binding.command) {
            return Err(invalid(format!(
                "`{}` is not a command's name, `namespace.action`",
                binding.command
            )));
        }
        let when = binding
            .when
            .as_deref()
            .map(When::parse)
            .transpose()
            .map_err(invalid)?;
        self.state.bindings.push(Binding {
            keys,
            invocation: Invocation {
                command: binding.command,
                args: binding.args,
            },
            when,
        });
        *self.bindings_changed = true;
        Ok(())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, Error> {
        Ok(self.context.clipboard.text())
    }

    fn set_clipboard_text(&mut self, text: &str) -> Result<(), Error> {
        self.context.clipboard.set_text(text);
        Ok(())
    }

    fn focused_window(&self) -> Option<host::WindowId> {
        self.context
            .focused
            .map(|window| host::WindowId(window.get()))
    }

    fn focused_pane(&self) -> Option<host::PaneId> {
        let window = self.context.focused?;
        let pane = self.context.workspace.active_pane(window)?;
        Some(host::PaneId(pane.get()))
    }

    fn selection_text(&mut self, pane: host::PaneId) -> Result<Option<String>, Error> {
        Ok(self.pane(pane)?.selection_text())
    }

    fn select_all(&mut self, pane: host::PaneId) -> Result<(), Error> {
        self.pane(pane)?.select_all();
        Ok(())
    }

    fn clear_scrollback(&mut self, pane: host::PaneId) -> Result<(), Error> {
        self.pane(pane)?.clear_scrollback();
        Ok(())
    }

    fn clear(&mut self, pane: host::PaneId) -> Result<(), Error> {
        self.pane(pane)?.clear();
        Ok(())
    }

    fn paste(&mut self, pane: host::PaneId, text: &str) -> Result<(), Error> {
        let found = self.pane(pane)?;
        let bracketed = found
            .input_modes()
            .is_some_and(|modes| modes.bracketed_paste);
        match paste::decide(text, bracketed, self.context.confirm_paste) {
            Paste::Now(text) => found.paste(&text),
            Paste::Ask => self.requests.push(Request::ConfirmPaste {
                pane: PaneId::of(pane.0),
                text: text.to_owned(),
            }),
        }
        Ok(())
    }

    fn send_text(&mut self, pane: host::PaneId, text: &str) -> Result<(), Error> {
        self.pane(pane)?.send_text(text);
        Ok(())
    }

    fn scroll(&mut self, pane: host::PaneId, scroll: host::Scroll) -> Result<(), Error> {
        let rows = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
        self.pane(pane)?.scroll(match scroll {
            host::Scroll::Up(n) => Scroll::Up(rows(n)),
            host::Scroll::Down(n) => Scroll::Down(rows(n)),
            host::Scroll::PageUp => Scroll::PageUp,
            host::Scroll::PageDown => Scroll::PageDown,
            host::Scroll::Top => Scroll::Top,
            host::Scroll::Bottom => Scroll::Bottom,
        });
        Ok(())
    }

    fn font_size(&self) -> f32 {
        self.context.font_size
    }

    fn set_font_size(&mut self, size: Option<f32>) -> Result<(), Error> {
        if let Some(size) = size {
            Size::try_from(f64::from(size)).map_err(|message| Error::new(Code::Failed, message))?;
        }
        self.requests.push(Request::FontSize(size));
        Ok(())
    }

    fn open_window(&mut self) -> Result<(), Error> {
        self.requests.push(Request::OpenWindow);
        Ok(())
    }

    fn close_window(&mut self, window: host::WindowId) -> Result<(), Error> {
        let id = WindowId::of(window.0);
        if self.context.workspace.window(id).is_none() {
            return Err(Error::new(
                Code::Failed,
                format!("there is no window {}", window.0),
            ));
        }
        self.requests.push(Request::CloseWindow(id));
        Ok(())
    }

    fn quit(&mut self) -> Result<(), Error> {
        self.requests.push(Request::Quit);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
