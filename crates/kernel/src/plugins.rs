//! The plugins that the app runs, built in, and the host that they reach
//! the app through. Each call of a plugin runs within `catch_unwind`, so
//! that one that panics is disabled, with its commands and its bindings,
//! rather than taking the app down.
//!
//! A plugin reaches the host through messages: its reads answer with the
//! state as the host handles them; its writes, as a pane's selection, are
//! queued, and applied in their order as its call returns, a plugin on the
//! main thread too; what changes the app, as a window to open or another
//! command to run, comes as a request, which the app takes after them, as
//! a plugin is never called while it runs. Each request needs its
//! capability, which the plugin's grants hold.

use std::any::Any;
use std::panic::{self, AssertUnwindSafe};

pub use lokyr_plugin_sdk::host;
use lokyr_plugin_sdk::message::{Call, Effect, Request as Asked, Response};
pub use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_text_core::Size;
use lokyr_vt_core::terminal::Scroll;

use crate::commands::{self, Args, Capability, Code, Command, Error, Registry};
use crate::config::ConfirmPaste;
use crate::keymap::{self, Binding, Invocation, When};
use crate::l10n::Texts;
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
    /// Open a URL, or an absolute path, with the system's handler.
    Open(String),
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

/// What the host keeps of a plugin: its name, its status, the
/// capabilities that it was granted, and the commands and the bindings that
/// it gave.
struct State {
    name: String,
    status: Status,
    /// Its capabilities; `None` for every one, as a built-in plugin's.
    granted: Option<Vec<Capability>>,
    commands: Vec<Command>,
    bindings: Vec<Binding>,
    /// Its catalogs, of its commands' titles.
    catalogs: &'static [(&'static str, &'static str)],
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

    /// Adds `plugin`, by its `name`, to activate, with every capability,
    /// as a built-in plugin has them.
    pub fn add(&mut self, name: &str, plugin: Box<dyn Plugin>) {
        self.add_granted(name, plugin, None);
    }

    /// Adds `plugin`, by its `name`, to activate, with the capabilities
    /// `granted`, or every one with `None`: a request that needs another
    /// fails.
    pub fn add_granted(
        &mut self,
        name: &str,
        plugin: Box<dyn Plugin>,
        granted: Option<Vec<Capability>>,
    ) {
        // A plugin whose catalogs panic has none.
        let catalogs = panic::catch_unwind(AssertUnwindSafe(|| plugin.catalogs())).unwrap_or(&[]);
        self.entries.push(Entry {
            plugin,
            state: State {
                name: name.to_owned(),
                status: Status::Inactive,
                granted,
                commands: Vec::new(),
                bindings: Vec::new(),
                catalogs,
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

    /// Whether `command`, which an active plugin registered, would do
    /// something with `args`, which the registry checked, as its plugin
    /// tells: what the call asks of the app is dropped; a fault when it
    /// panicked, which disables it.
    pub fn applies(
        &mut self,
        command: &str,
        args: &Args,
        context: &mut Context<'_>,
    ) -> Result<bool, Error> {
        let Some(at) = self.owner(command) else {
            return Err(Error::new(
                Code::NotFound,
                format!("no plugin runs `{command}`"),
            ));
        };
        let before = self.requests.len();
        let applies = look(
            &mut self.entries[at],
            context,
            &mut self.bindings_changed,
            |plugin, cx| Ok(plugin.applies(cx, command, args)),
        );
        self.requests.truncate(before);
        applies
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

    /// The commands of the active plugins, as [`Plugins::commands`], titled
    /// in `language` from their plugins' catalogs: by their messages, as
    /// [`title_message`] names them, the language's or English's; or by the
    /// titles that they registered.
    pub fn commands_in(&self, language: &str) -> Vec<Command> {
        let mut commands = Vec::new();
        for state in self.active() {
            let texts = Texts::new(language, state.catalogs);
            commands.extend(state.commands.iter().map(|command| {
                Command {
                    title: texts
                        .get(&title_message(&command.name))
                        .unwrap_or_else(|| command.title.clone()),
                    ..command.clone()
                }
            }));
        }
        commands
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

/// The message of a plugin's catalogs that titles its command `name`:
/// `command-` and the name, with `-` for its `.`, as `command-pane-copy`.
pub fn title_message(name: &str) -> String {
    format!("command-{}", name.replace('.', "-"))
}

/// Calls `call` on `entry`'s plugin, within `catch_unwind`, and applies
/// its writes, in their order, as it returns: a plugin that panics is
/// disabled, and the writes and the requests of the call go.
fn call<T>(
    entry: &mut Entry,
    context: &mut Context<'_>,
    requests: &mut Vec<Request>,
    bindings_changed: &mut bool,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let before = requests.len();
    let mut writes = Vec::new();
    let outcome = within(
        entry,
        context,
        (requests, &mut writes),
        bindings_changed,
        call,
    );
    match outcome {
        Ok(result) => {
            apply(writes, context, requests);
            result
        }
        Err(fault) => {
            requests.truncate(before);
            fault
        }
    }
}

/// Calls `call` on `entry`'s plugin, as [`call`] does, but drops its
/// writes and its requests, as the call only looks.
fn look<T>(
    entry: &mut Entry,
    context: &mut Context<'_>,
    bindings_changed: &mut bool,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<T, Error> {
    let (mut requests, mut writes) = (Vec::new(), Vec::new());
    let outcome = within(
        entry,
        context,
        (&mut requests, &mut writes),
        bindings_changed,
        call,
    );
    outcome.unwrap_or_else(|fault| fault)
}

/// Calls `call` on `entry`'s plugin within `catch_unwind`, its writes and
/// its requests into `queues`: the call's result, or, as it panicked, which
/// disables the plugin, the fault's error.
fn within<T>(
    entry: &mut Entry,
    context: &mut Context<'_>,
    (requests, writes): (&mut Vec<Request>, &mut Vec<Asked>),
    bindings_changed: &mut bool,
    call: impl FnOnce(&mut dyn Plugin, &mut Cx<'_>) -> Result<T, Error>,
) -> Result<Result<T, Error>, Result<T, Error>> {
    let Entry { plugin, state } = entry;
    let name = state.name.clone();
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        let mut host = Host {
            state: &mut *state,
            context: &mut *context,
            requests: &mut *requests,
            writes: &mut *writes,
            bindings_changed: &mut *bindings_changed,
        };
        call(plugin.as_mut(), &mut Cx::new(&mut host, &name))
    }));
    outcome.map_err(|payload| {
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

/// Applies a call's `writes` to the kernel's state, in their order: the
/// writes to a pane that closed since do nothing; a paste that asks the
/// user first becomes a request of the app's.
fn apply(writes: Vec<Asked>, context: &mut Context<'_>, requests: &mut Vec<Request>) {
    for write in writes {
        let pane = |pane: host::PaneId| context.workspace.pane(PaneId::of(pane.0));
        match write {
            Asked::SetClipboardText(text) => context.clipboard.set_text(&text),
            Asked::SelectAll(id) => {
                if let Some(pane) = pane(id) {
                    pane.select_all();
                }
            }
            Asked::ClearScrollback(id) => {
                if let Some(pane) = pane(id) {
                    pane.clear_scrollback();
                }
            }
            Asked::Clear(id) => {
                if let Some(pane) = pane(id) {
                    pane.clear();
                }
            }
            Asked::Paste { pane: id, text } => {
                let Some(found) = pane(id) else {
                    continue;
                };
                let bracketed = found
                    .input_modes()
                    .is_some_and(|modes| modes.bracketed_paste);
                match paste::decide(&text, bracketed, context.confirm_paste) {
                    Paste::Now(text) => found.paste(&text),
                    Paste::Ask => requests.push(Request::ConfirmPaste {
                        pane: PaneId::of(id.0),
                        text,
                    }),
                }
            }
            Asked::SendText { pane: id, text } => {
                if let Some(pane) = pane(id) {
                    pane.send_text(&text);
                }
            }
            Asked::Scroll { pane: id, scroll } => {
                if let Some(pane) = pane(id) {
                    pane.scroll(scrolled(scroll));
                }
            }
            // The host queues writes alone.
            _ => {}
        }
    }
}

/// The terminal's scroll of the SDK's.
fn scrolled(scroll: host::Scroll) -> Scroll {
    let rows = |n: u32| usize::try_from(n).unwrap_or(usize::MAX);
    match scroll {
        host::Scroll::Up(n) => Scroll::Up(rows(n)),
        host::Scroll::Down(n) => Scroll::Down(rows(n)),
        host::Scroll::PageUp => Scroll::PageUp,
        host::Scroll::PageDown => Scroll::PageDown,
        host::Scroll::Top => Scroll::Top,
        host::Scroll::Bottom => Scroll::Bottom,
    }
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

/// The schemes of the URLs that a plugin can open: none that starts another
/// application on the user's behalf.
const SCHEMES: [&str; 4] = ["http", "https", "mailto", "file"];

/// What opening `target` opens: a URL of one of [`SCHEMES`], or a path,
/// absolute or from `~`, which becomes the home directory.
fn opening(target: &str) -> Result<String, String> {
    if target.starts_with('/') {
        return Ok(target.to_owned());
    }
    if target == "~" || target.starts_with("~/") {
        let home = std::env::home_dir().ok_or("there is no home directory")?;
        return Ok(format!("{}{}", home.to_string_lossy(), &target[1..]));
    }
    let scheme = target
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| {
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        });
    match scheme {
        Some(scheme) if SCHEMES.contains(&scheme.to_ascii_lowercase().as_str()) => {
            Ok(target.to_owned())
        }
        Some(scheme) => Err(format!(
            "`{scheme}:` does not open: only {}",
            SCHEMES.join(", ")
        )),
        None => Err(format!(
            "`{target}` is neither a URL nor a path that is absolute or from `~`"
        )),
    }
}

/// Whether opening `target` would open something: a URL of one of
/// [`SCHEMES`], or a path, absolute or from `~`, that is there, as a link
/// that leads somewhere.
fn opens(target: &str) -> bool {
    opening(target)
        .is_ok_and(|target| !target.starts_with('/') || std::path::Path::new(&target).exists())
}

/// The host of one plugin's call.
struct Host<'h, 'c> {
    state: &'h mut State,
    context: &'h mut Context<'c>,
    requests: &'h mut Vec<Request>,
    /// The call's writes, which it applies as the call returns.
    writes: &'h mut Vec<Asked>,
    bindings_changed: &'h mut bool,
}

impl Host<'_, '_> {
    /// The pane of `pane`'s handle; `not-found` once it closed.
    fn pane(&self, pane: host::PaneId) -> Result<&Pane, Error> {
        self.context
            .workspace
            .pane(PaneId::of(pane.0))
            .ok_or_else(|| Error::new(Code::NotFound, format!("there is no pane {}", pane.0)))
    }

    /// Whether the plugin may make `request`: it has the capability that
    /// the request needs, if it needs one.
    fn allows(&self, request: &Asked) -> Result<(), Error> {
        let Some(needed) = request.capability() else {
            return Ok(());
        };
        match &self.state.granted {
            Some(granted) if !granted.contains(&needed) => Err(Error::new(
                Code::MissingCapability,
                format!(
                    "the plugin `{}` lacks the capability `{needed}`",
                    self.state.name
                ),
            )),
            _ => Ok(()),
        }
    }

    /// Answers `request`, a read, with the state as it is now.
    fn read(&mut self, request: Asked) -> Result<Response, Error> {
        Ok(match request {
            Asked::ClipboardText => Response::Text(self.context.clipboard.text()),
            Asked::FocusedWindow => Response::Window(
                self.context
                    .focused
                    .map(|window| host::WindowId(window.get())),
            ),
            Asked::FocusedPane => Response::Pane(
                self.context
                    .focused
                    .and_then(|window| self.context.workspace.active_pane(window))
                    .map(|pane| host::PaneId(pane.get())),
            ),
            Asked::SelectionText(pane) => Response::Text(self.pane(pane)?.selection_text()),
            Asked::LineText { pane, row, reach } => Response::Line(
                self.pane(pane)?
                    .line_text(row, reach.min(host::MOST_REACH))
                    .map(|(text, cells)| host::LineText { text, cells }),
            ),
            Asked::LinkAt { pane, column, row } => {
                Response::Text(self.pane(pane)?.link_at(column, row))
            }
            Asked::WorkingDirectory(pane) => Response::Text(
                self.pane(pane)?
                    .working_directory()
                    .map(|directory| directory.to_string_lossy().into_owned()),
            ),
            Asked::Opens(target) => Response::Boolean(opens(&target)),
            Asked::FontSize => Response::FontSize(self.context.font_size),
            other => {
                return Err(Error::new(Code::Fault, format!("{other:?} is not a read")));
            }
        })
    }

    /// Checks `request`, a write, and queues it: one to a pane that is not
    /// there fails now.
    fn queue(&mut self, request: Asked) -> Result<Response, Error> {
        match &request {
            Asked::SelectAll(pane)
            | Asked::ClearScrollback(pane)
            | Asked::Clear(pane)
            | Asked::Paste { pane, .. }
            | Asked::SendText { pane, .. }
            | Asked::Scroll { pane, .. } => {
                self.pane(*pane)?;
            }
            _ => {}
        }
        self.writes.push(request);
        Ok(Response::Done)
    }

    /// Checks `request`, of the app's, and passes it on, for the app to do
    /// once the call returns.
    fn ask_app(&mut self, request: Asked) -> Result<Response, Error> {
        let asked = match request {
            Asked::RunCommand { name, args } => {
                let args = self.context.registry.check(&name, args)?;
                Request::Run(Invocation {
                    command: name,
                    args,
                })
            }
            Asked::Open(target) => Request::Open(
                opening(&target).map_err(|message| Error::new(Code::Failed, message))?,
            ),
            Asked::SetFontSize(size) => {
                if let Some(size) = size {
                    Size::try_from(f64::from(size))
                        .map_err(|message| Error::new(Code::Failed, message))?;
                }
                Request::FontSize(size)
            }
            Asked::OpenWindow => Request::OpenWindow,
            Asked::CloseWindow(window) => {
                let id = WindowId::of(window.0);
                if self.context.workspace.window(id).is_none() {
                    return Err(Error::new(
                        Code::NotFound,
                        format!("there is no window {}", window.0),
                    ));
                }
                Request::CloseWindow(id)
            }
            Asked::Quit => Request::Quit,
            other => {
                return Err(Error::new(
                    Code::Fault,
                    format!("{other:?} is not a request of the app's"),
                ));
            }
        };
        self.requests.push(asked);
        Ok(Response::Done)
    }

    /// Takes `request`, a declaration of the plugin's commands or keys, at
    /// once.
    fn declare(&mut self, request: Asked) -> Result<Response, Error> {
        match request {
            Asked::RegisterCommand(command) => {
                self.context.registry.register(command.clone())?;
                self.state.commands.push(command);
            }
            Asked::Bind(binding) => {
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
            }
            other => {
                return Err(Error::new(
                    Code::Fault,
                    format!("{other:?} is not a declaration"),
                ));
            }
        }
        Ok(Response::Done)
    }
}

impl host::Host for Host<'_, '_> {
    fn handle(&mut self, call: Call) -> Response {
        let request = call.request;
        let answered = if call.caller == self.state.name {
            self.allows(&request).and_then(|()| match request.effect() {
                Effect::Read => self.read(request),
                Effect::Write => self.queue(request),
                Effect::App => self.ask_app(request),
                Effect::Declaration => self.declare(request),
            })
        } else {
            Err(Error::new(
                Code::Fault,
                format!(
                    "a call of the plugin `{}` came as `{}`'s",
                    self.state.name, call.caller
                ),
            ))
        };
        answered.unwrap_or_else(Response::Failed)
    }
}

#[cfg(test)]
mod tests;
