//! A plugin's VM: Luau's state, set up as a sandbox, of the libraries that
//! compute, its package's modules through its own `require`, and the table
//! of the plugin's callbacks that its entry returns.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use mlua::{Function, Lua, LuaOptions, StdLib, Table, Value};

use crate::watchdog::Watchdog;
use crate::{Callback, Deadlines, Execution, Package};

/// The most memory of a VM, of Luau's allocator, past which an allocation
/// fails, which ends the call.
pub const MOST_MEMORY: usize = 64 * 1024 * 1024;

/// Why a call of a VM failed: the plugin's code, or a limit of the VM's,
/// which faults the plugin.
#[derive(Debug)]
pub(crate) enum Failure {
    /// The code failed, of its error.
    Code(mlua::Error),
    /// The call ran past its deadline.
    PastDeadline,
    /// The VM ran out of its memory.
    OutOfMemory,
}

/// The path of a package's entry, which returns its callbacks' table.
pub const ENTRY: &str = "src/init.luau";

/// The globals of Luau's base library that reach past the computation, or
/// past the sandbox: code of text, a function's environment, and the
/// collector's control; and the output of `print`, which goes nowhere a
/// plugin's user sees.
const TAKEN: [&str; 6] = [
    "loadstring",
    "getfenv",
    "setfenv",
    "collectgarbage",
    "gcinfo",
    "print",
];

/// Whether Luau's native code generator runs on this system and CPU.
pub(crate) fn native_supported() -> bool {
    // SAFETY: a query of Luau's of no argument, which reads the CPU's
    // features.
    #[allow(unsafe_code)]
    unsafe {
        mlua::ffi::luau_codegen_supported() != 0
    }
}

/// A plugin's VM, and the table of its callbacks.
pub(crate) struct Vm {
    /// Its watchdog, which ends before the state.
    watchdog: Watchdog,
    callbacks: Table,
    native: bool,
    deadlines: Deadlines,
    /// The state, which the VM owns as long as its plugin runs.
    _lua: Lua,
}

/// The modules that a VM's `require` loaded, by their paths, and those that
/// it is loading, which a cycle meets.
#[derive(Default)]
struct Modules {
    loaded: BTreeMap<String, Value>,
    loading: Vec<String>,
}

impl Vm {
    /// The VM of `package`, of the plugin `name`, run as `execution` says,
    /// its calls within `deadlines`: its sandbox, its memory's limit, its
    /// `require`, and its entry run, within the activation's deadline,
    /// which returns the table of its callbacks; why it failed, of an entry
    /// that fails or returns none.
    pub(crate) fn new(
        package: &Arc<Package>,
        name: &str,
        execution: Execution,
        deadlines: Deadlines,
    ) -> Result<Self, Failure> {
        let lua = Self::state(execution).map_err(Failure::Code)?;
        let native = execution == Execution::Native && native_supported();
        let mut state = 0;
        // SAFETY: the closure only keeps the state's address, which the
        // watchdog, which the VM drops before the state, takes.
        #[allow(unsafe_code)]
        let watchdog = unsafe {
            lua.exec_raw::<()>((), |raw| state = raw as usize)
                .map_err(Failure::Code)?;
            Watchdog::new(state, name)
                .map_err(|error| Failure::Code(mlua::Error::external(error)))?
        };
        let modules = Arc::new(Mutex::new(Modules::default()));
        let entry = watched(&watchdog, deadlines.of(Callback::Activate), || {
            load(&lua, package, &modules, ENTRY)
        })?;
        let Value::Table(callbacks) = entry else {
            return Err(Failure::Code(mlua::Error::runtime(format!(
                "{ENTRY} returns the table of the plugin's callbacks, not {}",
                entry.type_name()
            ))));
        };
        Ok(Self {
            watchdog,
            callbacks,
            native,
            deadlines,
            _lua: lua,
        })
    }

    /// The state of a VM run as `execution` says: of the libraries that
    /// compute, the others taken away, its memory's limit, and Luau's
    /// sandbox mode.
    fn state(execution: Execution) -> mlua::Result<Lua> {
        let libraries = StdLib::STRING
            | StdLib::TABLE
            | StdLib::MATH
            | StdLib::BIT
            | StdLib::UTF8
            | StdLib::BUFFER;
        let lua = Lua::new_with(libraries, LuaOptions::default())?;
        lua.enable_jit(execution == Execution::Native && native_supported());
        lua.set_memory_limit(MOST_MEMORY)?;
        let globals = lua.globals();
        for name in TAKEN {
            globals.raw_set(name, Value::Nil)?;
        }
        // The libraries and the globals read-only from here, each thread of
        // an environment of its own.
        lua.sandbox(true)?;
        Ok(lua)
    }

    /// Whether it runs its code of the native code generator.
    pub(crate) fn runs_native(&self) -> bool {
        self.native
    }

    /// Calls the plugin's callback of `callback`, if its table has one,
    /// within the deadline of its kind.
    pub(crate) fn call(&self, callback: Callback) -> Result<(), Failure> {
        let name = callback.name();
        match self.callbacks.get::<Value>(name).map_err(Failure::Code)? {
            Value::Nil => Ok(()),
            Value::Function(function) => {
                watched(&self.watchdog, self.deadlines.of(callback), || {
                    function.call::<()>(())
                })
            }
            other => Err(Failure::Code(mlua::Error::runtime(format!(
                "the callback `{name}` is a function, not {}",
                other.type_name()
            )))),
        }
    }

    /// How many times its watchdog's thread woke.
    #[cfg(test)]
    pub(crate) fn watchdog_wakes(&self) -> usize {
        self.watchdog.wakes()
    }

    /// The Lua state, as the tests reach its globals.
    #[cfg(test)]
    pub(crate) fn lua(&self) -> &Lua {
        &self._lua
    }
}

/// Runs `run` within `deadline` from now, as `watchdog` watches it: what it
/// gives, or why it failed, past its deadline, out of memory, or of its
/// code's error.
fn watched<T>(
    watchdog: &Watchdog,
    deadline: std::time::Duration,
    run: impl FnOnce() -> mlua::Result<T>,
) -> Result<T, Failure> {
    watchdog.arm(Instant::now() + deadline);
    let ran = run();
    let fired = watchdog.disarm();
    ran.map_err(|error| {
        if fired {
            Failure::PastDeadline
        } else if is_memory(&error) {
            Failure::OutOfMemory
        } else {
            Failure::Code(error)
        }
    })
}

/// Whether `error` is of the VM's memory's limit, of a callback's too.
fn is_memory(error: &mlua::Error) -> bool {
    match error {
        mlua::Error::MemoryError(_) => true,
        mlua::Error::CallbackError { cause, .. } => is_memory(cause),
        _ => false,
    }
}

/// Runs the module at `path` of `package`, once, of a `require` of its own
/// directory: what it returns, which later requires of it share.
fn load(
    lua: &Lua,
    package: &Arc<Package>,
    modules: &Arc<Mutex<Modules>>,
    path: &str,
) -> mlua::Result<Value> {
    {
        let mut held = modules.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(value) = held.loaded.get(path) {
            return Ok(value.clone());
        }
        if held.loading.iter().any(|loading| loading == path) {
            return Err(mlua::Error::runtime(format!(
                "{path} requires itself, through {}",
                held.loading.join(" → ")
            )));
        }
        held.loading.push(path.to_owned());
    }
    let ran = run(lua, package, modules, path);
    let mut held = modules.lock().unwrap_or_else(PoisonError::into_inner);
    held.loading.retain(|loading| loading != path);
    let value = ran?;
    held.loaded.insert(path.to_owned(), value.clone());
    Ok(value)
}

/// Runs the module at `path`, of an environment of its own over the
/// globals, which holds its `require`.
fn run(
    lua: &Lua,
    package: &Arc<Package>,
    modules: &Arc<Mutex<Modules>>,
    path: &str,
) -> mlua::Result<Value> {
    let source = package
        .source(path)
        .ok_or_else(|| mlua::Error::runtime(format!("the package holds no {path}")))?;
    let environment = lua.create_table()?;
    let lookup = lua.create_table()?;
    lookup.raw_set("__index", lua.globals())?;
    environment.set_metatable(Some(lookup))?;
    environment.raw_set("require", requirer(lua, package, modules, path)?)?;
    lua.load(source)
        .set_name(format!("@{path}"))
        .set_environment(environment)
        .into_function()?
        .call::<Value>(())
}

/// The `require` of the module at `from`: a module of the package, of a
/// path from `from`'s directory that starts with `./` or `../` and stays
/// within the package, its `.luau` left out, or a directory's `init.luau`.
fn requirer(
    lua: &Lua,
    package: &Arc<Package>,
    modules: &Arc<Mutex<Modules>>,
    from: &str,
) -> mlua::Result<Function> {
    let (package, modules, from) = (Arc::clone(package), Arc::clone(modules), from.to_owned());
    lua.create_function(move |lua, name: String| {
        let path = resolve(&from, &name, &package).map_err(mlua::Error::runtime)?;
        load(lua, &package, &modules, &path)
    })
}

/// The path in `package` of the module `name` that the module at `from`
/// requires, or why none.
pub(crate) fn resolve(from: &str, name: &str, package: &Package) -> Result<String, String> {
    if !(name.starts_with("./") || name.starts_with("../")) {
        return Err(format!(
            "`{name}` is not a module of the package, of a path from `./` or `../`"
        ));
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for part in name.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("`{name}` leaves the package"));
                }
            }
            part => parts.push(part),
        }
    }
    let path = parts.join("/");
    [format!("{path}.luau"), format!("{path}/init.luau")]
        .into_iter()
        .find(|candidate| package.source(candidate).is_some())
        .ok_or_else(|| format!("the package holds no module `{name}` of {from}"))
}

#[cfg(test)]
mod tests;
