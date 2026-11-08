//! A plugin's VM: Luau's state, set up as a sandbox, of the libraries that
//! compute, its package's modules through its own `require`, and the table
//! of the plugin's callbacks that its entry returns.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};

use mlua::{Function, Lua, LuaOptions, StdLib, Table, Value};

use crate::{Execution, Package};

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
    /// The state, which the VM owns as long as its plugin runs.
    _lua: Lua,
    callbacks: Table,
    native: bool,
}

/// The modules that a VM's `require` loaded, by their paths, and those that
/// it is loading, which a cycle meets.
#[derive(Default)]
struct Modules {
    loaded: BTreeMap<String, Value>,
    loading: Vec<String>,
}

impl Vm {
    /// The VM of `package`, run as `execution` says: its sandbox, its
    /// `require`, and its entry run, which returns the table of its
    /// callbacks; an error of an entry that fails or returns none.
    pub(crate) fn new(package: &Arc<Package>, execution: Execution) -> mlua::Result<Self> {
        let libraries = StdLib::STRING
            | StdLib::TABLE
            | StdLib::MATH
            | StdLib::BIT
            | StdLib::UTF8
            | StdLib::BUFFER;
        let lua = Lua::new_with(libraries, LuaOptions::default())?;
        let native = execution == Execution::Native && native_supported();
        lua.enable_jit(native);
        let globals = lua.globals();
        for name in TAKEN {
            globals.raw_set(name, Value::Nil)?;
        }
        // The libraries and the globals read-only from here, each thread of
        // an environment of its own.
        lua.sandbox(true)?;
        let modules = Arc::new(Mutex::new(Modules::default()));
        let entry = load(&lua, package, &modules, ENTRY)?;
        let Value::Table(callbacks) = entry else {
            return Err(mlua::Error::runtime(format!(
                "{ENTRY} returns the table of the plugin's callbacks, not {}",
                entry.type_name()
            )));
        };
        Ok(Self {
            _lua: lua,
            callbacks,
            native,
        })
    }

    /// Whether it runs its code of the native code generator.
    pub(crate) fn runs_native(&self) -> bool {
        self.native
    }

    /// Calls the callback `name` of the plugin's table, if it has one.
    pub(crate) fn call(&self, name: &str) -> mlua::Result<()> {
        match self.callbacks.get::<Value>(name)? {
            Value::Nil => Ok(()),
            Value::Function(function) => function.call::<()>(()),
            other => Err(mlua::Error::runtime(format!(
                "the callback `{name}` is a function, not {}",
                other.type_name()
            ))),
        }
    }

    /// The Lua state, as the tests reach its globals.
    #[cfg(test)]
    pub(crate) fn lua(&self) -> &Lua {
        &self._lua
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
