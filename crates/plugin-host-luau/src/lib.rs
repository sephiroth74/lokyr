//! Runs third-party plugins written in Luau.
//!
//! Each plugin gets a Luau VM of its own, with only the libraries that
//! compute, a memory limit and a deadline per call. Everything outside the
//! computation goes through host functions that check the plugin's
//! capabilities, and calls are forwarded as the same messages that native
//! plugins send.
//!
//! A plugin's package holds its sources, its entry `src/init.luau`, which
//! returns the table of its callbacks, each optional, as the SDK's
//! `Plugin` has them: `activate(cx)`, `on_command(cx, name, args)`,
//! `applies(cx, name, args)`, `on_event(cx, events)`, `render(cx, view)`
//! and `on_ui_event(cx, event)`. Its VM is made as it first activates, so
//! that an app of no plugin of Luau makes none, and runs in Luau's sandbox
//! mode: the libraries `string`, `table`, `math`, `bit32`, `utf8` and
//! `buffer`, and a `require` of the package's own modules alone.
//!
//! `cx`, which each callback takes first, holds a function of each of the
//! SDK's requests, by its Rust name, as `cx.focused_pane()` or
//! `cx.line_text(pane, row, reach)`, read from the SDK's own type: a
//! request of fields takes them in their order, a `nil` leaving an optional
//! one out. It answers with the response's value, `true` of a write, or
//! with `nil`, the error's message and its code, as Lua's standard library
//! answers; an argument of another shape raises an error, as a Lua error
//! raised by a callback ends its call, a failure of the plugin's.
//!
//! A view's nodes and the events cross as flat tables, of their `kind`
//! beside their fields, as `{ kind = "text", text = "hi" }` and `{ kind =
//! "tab-created", tab = 3, window = 1, index = 0 }`; a view converts
//! without recursion, bounded at the host's depth as it converts.

// `deny` rather than `forbid`: stopping a plugin at its deadline and reading
// bulk results go through Lua's C API, which is `unsafe`, and each of those
// call sites is allowed explicitly.
#![deny(unsafe_code)]

mod events;
mod requests;
mod shape;
mod tree;
mod vm;
mod watchdog;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use lokyr_plugin_sdk::command::{Args, Code, Error};
use lokyr_plugin_sdk::message::Event;
use lokyr_plugin_sdk::plugin::{Cx, Plugin};
use lokyr_plugin_sdk::ui::{UiEvent, ViewId};
use lokyr_plugin_sdk::view::Node;
use mlua::{IntoLua, Lua, MultiValue, Value};

use crate::shape::ShapeError;

pub use vm::{ENTRY, MOST_MEMORY};

/// A kind of call of a plugin's callbacks, which its deadline is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Callback {
    /// Its activation, its entry's run too.
    Activate,
    /// A command of its.
    Command,
    /// A menu's question of whether a command of its would do something.
    Applies,
    /// A batch of events.
    Event,
    /// A render of one of its views.
    Render,
    /// An event of a node of one of its views.
    Ui,
}

impl Callback {
    /// The callback's name in the plugin's table.
    pub fn name(self) -> &'static str {
        match self {
            Self::Activate => "activate",
            Self::Command => "on_command",
            Self::Applies => "applies",
            Self::Event => "on_event",
            Self::Render => "render",
            Self::Ui => "on_ui_event",
        }
    }
}

/// The deadline of each kind of call, past which its code fails and its
/// plugin faults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Deadlines {
    /// Its activation's, its entry's run's too.
    pub activate: Duration,
    /// A command's, an event's and a node's event's.
    pub command: Duration,
    /// A render's and a menu's question's, which a frame waits for.
    pub render: Duration,
}

impl Default for Deadlines {
    /// A second of an activation, 250 ms of a command and 16 ms of a
    /// render, a frame's time at 60 Hz.
    fn default() -> Self {
        Self {
            activate: Duration::from_secs(1),
            command: Duration::from_millis(250),
            render: Duration::from_millis(16),
        }
    }
}

impl Deadlines {
    /// The deadline of `callback`.
    pub fn of(self, callback: Callback) -> Duration {
        match callback {
            Callback::Activate => self.activate,
            Callback::Command | Callback::Event | Callback::Ui => self.command,
            Callback::Render | Callback::Applies => self.render,
        }
    }
}

/// How many VMs the plugins of Luau made, in this process: none of an app
/// of no plugin of Luau, which the app's tests tell.
static VMS: AtomicUsize = AtomicUsize::new(0);

/// How many VMs the plugins of Luau made so far, in this process.
pub fn vms_made() -> usize {
    VMS.load(Ordering::Relaxed)
}

/// How a VM runs a plugin's code: of Luau's native code generator, as the
/// app signed with `allow-jit` maps its code, or of its interpreter, as a
/// system or a CPU without the generator runs it, the same code slower.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Execution {
    /// The native code generator, where the system has it, else the
    /// interpreter.
    #[default]
    Native,
    /// The interpreter alone.
    Interpreter,
}

/// Whether Luau's native code generator runs on this system and CPU.
pub fn native_supported() -> bool {
    vm::native_supported()
}

/// A plugin's package: its sources, by their paths in the package, as
/// `src/init.luau`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Package {
    sources: BTreeMap<String, String>,
}

impl Package {
    /// The package of `sources`, by their paths in the package, of `/`.
    pub fn new(sources: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            sources: sources.into_iter().collect(),
        }
    }

    /// The source at `path`, if the package holds it.
    pub fn source(&self, path: &str) -> Option<&str> {
        self.sources.get(path).map(String::as_str)
    }
}

/// A plugin of Luau: its package, run in a VM of its own, made as it first
/// activates.
pub struct LuauPlugin {
    name: String,
    package: Arc<Package>,
    execution: Execution,
    deadlines: Deadlines,
    vm: Option<vm::Vm>,
    /// The failure of its last render or look, which the host takes.
    failure: Option<Error>,
}

impl LuauPlugin {
    /// The plugin `name`, as its manifest names it, of `package`, run as
    /// `execution` says: of no VM until it activates.
    pub fn new(name: impl Into<String>, package: Package, execution: Execution) -> Self {
        Self {
            name: name.into(),
            package: Arc::new(package),
            execution,
            deadlines: Deadlines::default(),
            vm: None,
            failure: None,
        }
    }

    /// The plugin, its calls within `deadlines`.
    #[must_use]
    pub fn with_deadlines(mut self, deadlines: Deadlines) -> Self {
        self.deadlines = deadlines;
        self
    }

    /// Whether its VM was made, as it activated.
    pub fn has_vm(&self) -> bool {
        self.vm.is_some()
    }

    /// Whether its VM runs its code of the native code generator, as the
    /// system has it and its execution asks for it.
    pub fn runs_native(&self) -> bool {
        self.vm.as_ref().is_some_and(vm::Vm::runs_native)
    }

    /// The error of `failure` of a call of `callback`: `failed` of the
    /// plugin's code; a fault of a limit of its VM's, which the host takes
    /// as a fault of the plugin's.
    fn error(&self, callback: Callback, failure: vm::Failure) -> Error {
        let name = &self.name;
        let call = callback.name();
        match failure {
            vm::Failure::Code(error) => Error::new(
                Code::Failed,
                format!("the plugin `{name}` failed in `{call}`: {error}"),
            ),
            vm::Failure::PastDeadline => Error::new(
                Code::Fault,
                format!(
                    "the plugin `{name}` ran past its deadline of {} ms in `{call}`",
                    self.deadlines.of(callback).as_millis()
                ),
            ),
            vm::Failure::OutOfMemory => Error::new(
                Code::Fault,
                format!(
                    "the plugin `{name}` ran out of its {} MB of memory in `{call}`",
                    MOST_MEMORY / (1024 * 1024)
                ),
            ),
        }
    }

    /// Calls its callback of `callback` of `cx` and the arguments that
    /// `args` makes, if it has a VM and the callback: what it returns.
    fn call(
        &self,
        callback: Callback,
        cx: &mut Cx<'_>,
        args: impl FnOnce(&Lua) -> Result<Vec<Value>, ShapeError>,
    ) -> Result<Option<MultiValue>, Error> {
        let Some(vm) = &self.vm else {
            return Ok(None);
        };
        vm.call(callback, cx, args)
            .map_err(|failure| self.error(callback, failure))
    }

    /// Calls its callback of `callback`, as [`LuauPlugin::call`] does, and
    /// converts what it returns of `convert` within the call's deadline.
    fn call_converting<R>(
        &self,
        callback: Callback,
        cx: &mut Cx<'_>,
        args: impl FnOnce(&Lua) -> Result<Vec<Value>, ShapeError>,
        convert: impl FnOnce(MultiValue, &shape::Budget) -> Result<R, ShapeError>,
    ) -> Result<Option<Result<R, ShapeError>>, Error> {
        let Some(vm) = &self.vm else {
            return Ok(None);
        };
        vm.call_converting(callback, cx, args, convert)
            .map_err(|failure| self.error(callback, failure))
    }

    /// The error of a value of the wrong shape that its callback of
    /// `callback` returned, as `wanted` tells the right one.
    fn returned(&self, callback: Callback, wanted: &str, error: &ShapeError) -> Error {
        Error::new(
            Code::Failed,
            format!(
                "the plugin `{}`'s `{}` returns {wanted}: {error}",
                self.name,
                callback.name()
            ),
        )
    }
}

/// The arguments of a command's callback: its name and its arguments.
fn command_args(lua: &Lua, name: &str, args: &Args) -> Result<Vec<Value>, ShapeError> {
    Ok(vec![name.into_lua(lua)?, shape::to_lua(lua, args)?])
}

impl Plugin for LuauPlugin {
    /// Makes its VM, runs its entry, and calls its callbacks' `activate`,
    /// if it has one: the error of an entry or a callback that fails.
    fn activate(&mut self, cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.vm.is_none() {
            let made = vm::Vm::new(&self.package, &self.name, self.execution, self.deadlines)
                .map_err(|failure| self.error(Callback::Activate, failure))?;
            VMS.fetch_add(1, Ordering::Relaxed);
            self.vm = Some(made);
        }
        self.call(Callback::Activate, cx, |_| Ok(Vec::new()))
            .map(drop)
    }

    /// Without its callback, a command of its fails, as a plugin's of no
    /// such command.
    fn on_command(&mut self, cx: &mut Cx<'_>, name: &str, args: Args) -> Result<(), Error> {
        match self.call(Callback::Command, cx, |lua| command_args(lua, name, &args))? {
            Some(_) => Ok(()),
            None => Err(Error::new(
                Code::NotFound,
                format!(
                    "the plugin `{}` has no `on_command` for `{name}`",
                    self.name
                ),
            )),
        }
    }

    fn on_event(&mut self, cx: &mut Cx<'_>, events: &[Event]) -> Result<(), Error> {
        self.call(Callback::Event, cx, |lua| {
            Ok(vec![Value::Table(events::list_to_lua(lua, events)?)])
        })
        .map(drop)
    }

    /// Without its callback, every command would.
    fn applies(&mut self, cx: &mut Cx<'_>, name: &str, args: &Args) -> bool {
        match self.call(Callback::Applies, cx, |lua| command_args(lua, name, args)) {
            Ok(None) => true,
            Ok(Some(returned)) => match returned.front() {
                Some(Value::Boolean(applies)) => *applies,
                other => {
                    let what = other.map_or("nothing", Value::type_name);
                    let error = ShapeError(format!("it returned {what}"));
                    self.failure = Some(self.returned(Callback::Applies, "true or false", &error));
                    false
                }
            },
            Err(error) => {
                self.failure = Some(error);
                false
            }
        }
    }

    /// Its tree of a table of a node, or nothing of `nil`, without its
    /// callback too.
    fn render(&mut self, cx: &mut Cx<'_>, view: &ViewId) -> Option<Node> {
        let rendered = self.call_converting(
            Callback::Render,
            cx,
            |lua| Ok(vec![shape::to_lua(lua, view)?]),
            |returned, budget| match returned.into_iter().next().unwrap_or(Value::Nil) {
                Value::Nil => Ok(None),
                tree => tree::from_lua(tree, budget).map(Some),
            },
        );
        match rendered {
            Ok(None) => None,
            Ok(Some(Ok(tree))) => tree,
            Ok(Some(Err(error))) => {
                self.failure = Some(self.returned(
                    Callback::Render,
                    "a view, a table of a node, or nil",
                    &error,
                ));
                None
            }
            Err(error) => {
                self.failure = Some(error);
                None
            }
        }
    }

    fn on_ui_event(&mut self, cx: &mut Cx<'_>, event: &UiEvent) -> Result<(), Error> {
        self.call(Callback::Ui, cx, |lua| {
            Ok(vec![Value::Table(events::ui_to_lua(lua, event)?)])
        })
        .map(drop)
    }

    fn take_failure(&mut self) -> Option<Error> {
        self.failure.take()
    }
}

/// Makes `request` through its function of `cx`, as a plugin of Luau
/// calls it, on a host that answers with `response`: why not, as the host
/// took another request, or the answer that came back is not the one of
/// `response`, `nil` and the error of a failure.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub fn through_luau(
    request: &lokyr_plugin_sdk::message::Request,
    response: &lokyr_plugin_sdk::message::Response,
) -> Result<(), String> {
    use lokyr_plugin_sdk::host::Host;
    use lokyr_plugin_sdk::message::{Call, Request, Response};

    /// A host that answers every request with its response, and keeps the
    /// last that it took.
    struct Scripted<'r> {
        response: &'r Response,
        asked: Option<Request>,
    }

    impl Host for Scripted<'_> {
        fn handle(&mut self, call: Call) -> Response {
            self.asked = Some(call.request);
            self.response.clone()
        }
    }

    let entry = "return { call = function(cx, name, ...) return cx[name](...) end }";
    let package = Arc::new(Package::new([(ENTRY.to_owned(), entry.to_owned())]));
    let vm = vm::Vm::new(
        &package,
        "acme/through",
        Execution::Native,
        Deadlines::default(),
    )
    .map_err(|failure| format!("{failure:?}"))?;
    let (name, args) =
        requests::call_of(vm.lua(), request).map_err(|error| format!("{request:?}: {error}"))?;
    let mut host = Scripted {
        response,
        asked: None,
    };
    let answer = {
        let mut cx = Cx::new(&mut host, "acme/through");
        vm.invoke("call", Duration::from_secs(5), &mut cx, |lua| {
            let mut all = vec![name.as_str().into_lua(lua)?];
            all.extend(args);
            Ok(all)
        })
        .map_err(|failure| format!("`cx.{name}` of {request:?}: {failure:?}"))?
        .unwrap_or_default()
    };
    if host.asked.as_ref() != Some(request) {
        return Err(format!(
            "`cx.{name}` asked {:?}, not {request:?}",
            host.asked
        ));
    }
    requests::answered(vm.lua(), response, &answer).map_err(|error| format!("`cx.{name}`: {error}"))
}

/// Gives `events` to Luau, as a plugin's `on_event` takes them, and takes
/// them back: why not, as they did not come back the same.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub fn events_through_luau(events: &[Event]) -> Result<(), String> {
    let lua = Lua::new();
    let list = events::list_to_lua(&lua, events).map_err(|error| error.to_string())?;
    let back = list
        .sequence_values::<Value>()
        .map(|event| {
            event
                .map_err(shape::ShapeError::from)
                .and_then(|event| events::from_lua(&lua, event))
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    (back == events)
        .then_some(())
        .ok_or_else(|| format!("{events:?} came back as {back:?}"))
}

/// Gives `event` to Luau, as a plugin's `on_ui_event` takes it, and takes
/// it back: why not, as it did not come back the same.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub fn ui_event_through_luau(event: &UiEvent) -> Result<(), String> {
    let lua = Lua::new();
    let given = events::ui_to_lua(&lua, event).map_err(|error| error.to_string())?;
    let back = events::ui_from_lua(&lua, Value::Table(given)).map_err(|error| error.to_string())?;
    (back == *event)
        .then_some(())
        .ok_or_else(|| format!("{event:?} came back as {back:?}"))
}

/// Gives `view` to Luau, as a plugin's tests read a view, and takes it
/// back, as its render returns it: why not, as it did not come back the
/// same.
#[cfg(any(test, feature = "testing"))]
#[doc(hidden)]
pub fn view_through_luau(view: &Node) -> Result<(), String> {
    let lua = Lua::new();
    let given = tree::to_lua(&lua, view).map_err(|error| error.to_string())?;
    let back = tree::from_lua(Value::Table(given), &shape::Budget::default())
        .map_err(|error| error.to_string())?;
    (back == *view)
        .then_some(())
        .ok_or_else(|| format!("{view:?} came back as {back:?}"))
}

#[cfg(test)]
mod same;

#[cfg(test)]
mod tests;
