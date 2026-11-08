//! Runs third-party plugins written in Luau.
//!
//! Each plugin gets a Luau VM of its own, with only the libraries that
//! compute, a memory limit and a deadline per call. Everything outside the
//! computation goes through host functions that check the plugin's
//! capabilities, and calls are forwarded as the same messages that native
//! plugins send.
//!
//! A plugin's package holds its sources, its entry `src/init.luau`, which
//! returns the table of its callbacks. Its VM is made as it first
//! activates, so that an app of no plugin of Luau makes none, and runs in
//! Luau's sandbox mode: the libraries `string`, `table`, `math`, `bit32`,
//! `utf8` and `buffer`, and a `require` of the package's own modules alone.

// `deny` rather than `forbid`: stopping a plugin at its deadline and reading
// bulk results go through Lua's C API, which is `unsafe`, and each of those
// call sites is allowed explicitly.
#![deny(unsafe_code)]

mod vm;
mod watchdog;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use lokyr_plugin_sdk::command::{Code, Error};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

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
}

impl Plugin for LuauPlugin {
    /// Makes its VM, runs its entry, and calls its callbacks' `activate`,
    /// if it has one: the error of an entry or a callback that fails.
    fn activate(&mut self, _cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.vm.is_none() {
            let made = vm::Vm::new(&self.package, &self.name, self.execution, self.deadlines)
                .map_err(|failure| self.error(Callback::Activate, failure))?;
            VMS.fetch_add(1, Ordering::Relaxed);
            self.vm = Some(made);
        }
        let Some(vm) = &self.vm else {
            return Ok(());
        };
        vm.call(Callback::Activate)
            .map_err(|failure| self.error(Callback::Activate, failure))
    }
}
