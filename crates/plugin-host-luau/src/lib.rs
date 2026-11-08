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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use lokyr_plugin_sdk::command::{Code, Error};
use lokyr_plugin_sdk::plugin::{Cx, Plugin};

pub use vm::ENTRY;

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
            vm: None,
        }
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

    /// The error of `message`, a failure of the plugin's code, as its
    /// calls tell it.
    fn failed(&self, message: impl std::fmt::Display) -> Error {
        Error::new(
            Code::Failed,
            format!("the plugin `{}` failed: {message}", self.name),
        )
    }
}

impl Plugin for LuauPlugin {
    /// Makes its VM, runs its entry, and calls its callbacks' `activate`,
    /// if it has one: the error of an entry or a callback that fails.
    fn activate(&mut self, _cx: &mut Cx<'_>) -> Result<(), Error> {
        if self.vm.is_none() {
            let made =
                vm::Vm::new(&self.package, self.execution).map_err(|error| self.failed(error))?;
            VMS.fetch_add(1, Ordering::Relaxed);
            self.vm = Some(made);
        }
        let Some(vm) = &self.vm else {
            return Ok(());
        };
        vm.call("activate").map_err(|error| self.failed(error))
    }
}
