//! The host of a plugin's own tests, which `sleip test` runs: each test
//! file of the package, `tests/*.luau`, returns a table of functions, each
//! a test, which takes a `TestHost` of its own, the plugin made anew, of a
//! fresh VM, on the SDK's fake host, of a pane that has the focus. A test
//! fails at its first error, as `assert` raises it, of its file and line,
//! and as one of the host's calls fails, as the plugin's error tells; a
//! file of no test fails.
//!
//! The plugin is set up as the app sets it up: of its manifest's
//! capabilities, commands and slots, of its package's `src/` alone, beside
//! the app's commands and its first-party plugins', which
//! `luau/commands.json` holds; its commands run as the registry runs them,
//! their arguments checked, and the events reach it as the app delivers
//! them, of its subscriptions.
//!
//! The tests run in a VM of their own, of the plugin's sandbox, its
//! `require` within the package, so that a test reads the plugin's modules,
//! and of a watchdog: a test runs [`DEADLINE`] at most.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use lokyr_plugin_sdk::command::{Args, Capability, Command};
use lokyr_plugin_sdk::fake::FakeHost;
use lokyr_plugin_sdk::host::{CommandInfo, PaneId};
use lokyr_plugin_sdk::message::Data;
use lokyr_plugin_sdk::ui::{Slot, ViewId};
use mlua::{Function, Lua, Table, Value};
use serde::Deserialize;

use crate::vm::{self, Modules};
use crate::{Execution, LuauPlugin, Package, events, shape, tree};

/// The longest that a test runs, its plugin's calls among it.
pub const DEADLINE: Duration = Duration::from_secs(10);

/// What became of a test.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// Its file, by its path in the package, as `tests/hello.luau`.
    pub file: String,
    /// Its name, its function's in the file's table.
    pub name: String,
    /// Why it failed, if it did.
    pub failure: Option<Failure>,
}

/// Why a test failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    /// The line of its file where it failed, if its error tells it.
    pub line: Option<u32>,
    /// Its error, the place of its file left out.
    pub message: String,
}

/// What a plugin's tests need: its name, what its manifest declares, its
/// package of its sources and its tests, and its messages in English, each
/// its id and its text, which the plugin's calls read.
pub struct Plugin<'a> {
    /// Its name, `owner/name`.
    pub name: &'a str,
    /// The capabilities that its manifest declares.
    pub capabilities: &'a [Capability],
    /// The commands of others that its manifest's `[uses]` says it runs.
    pub uses: &'a [String],
    /// The slots that its manifest says it fills.
    pub slots: &'a [Slot],
    /// Its package, `src/**/*.luau` and `tests/**/*.luau`: the plugin's of
    /// `src/` alone, the tests' whole.
    pub package: &'a Package,
    /// Its messages of no argument, in English.
    pub messages: &'a [(String, String)],
}

/// Runs the tests of `plugin`, of each of its files `tests/*.luau` in their
/// order and each of its tests in their names' order: what became of each;
/// a file that does not load, or returns no table of functions, a test of
/// its own that failed, named `(file)`.
pub fn run(plugin: &Plugin<'_>) -> Vec<Outcome> {
    run_within(plugin, DEADLINE)
}

/// Runs the tests of `plugin`, as [`run`] does, each within `deadline`.
fn run_within(plugin: &Plugin<'_>, deadline: Duration) -> Vec<Outcome> {
    let package = Arc::new(plugin.package.clone());
    let mut outcomes = Vec::new();
    let files: Vec<String> = plugin
        .package
        .paths()
        .filter(|path| {
            // The files of `tests/` alone, those of its folders its modules.
            path.strip_prefix("tests/")
                .is_some_and(|name| !name.contains('/') && name.ends_with(".luau"))
        })
        .map(str::to_owned)
        .collect();
    for file in files {
        let failed = |message: String| Outcome {
            file: file.clone(),
            name: "(file)".to_owned(),
            failure: Some(failure(&file, &message)),
        };
        let lua = match vm::test_state() {
            Ok(lua) => lua,
            Err(error) => {
                outcomes.push(failed(error.to_string()));
                continue;
            }
        };
        let modules = Arc::new(Mutex::new(Modules::default()));
        let loaded = vm::watched_of(&lua, plugin.name, deadline, || {
            vm::load(&lua, &package, &modules, &file)
        });
        let tests = match loaded {
            Ok(Value::Table(tests)) => tests,
            Ok(other) => {
                outcomes.push(failed(format!(
                    "it returns a table of its tests, not {}",
                    other.type_name()
                )));
                continue;
            }
            Err(error) => {
                outcomes.push(failed(error));
                continue;
            }
        };
        let mut named: Vec<(String, Function)> = tests
            .pairs::<Value, Value>()
            .filter_map(Result::ok)
            .filter_map(|(key, value)| match (key, value) {
                (Value::String(key), Value::Function(test)) => {
                    Some((key.to_str().ok()?.to_owned(), test))
                }
                _ => None,
            })
            .collect();
        named.sort_by(|left, right| left.0.cmp(&right.0));
        if named.is_empty() {
            outcomes.push(failed(
                "it returns a table of no test, a function of a name".to_owned(),
            ));
            continue;
        }
        for (name, test) in named {
            let ran = test_host(&lua, plugin).and_then(|host| {
                vm::watched_of(&lua, plugin.name, deadline, || test.call::<()>(host))
            });
            outcomes.push(Outcome {
                file: file.clone(),
                name,
                failure: ran.err().map(|message| failure(&file, &message)),
            });
        }
    }
    outcomes
}

/// The failure of `message`, an error of the test file `file`: its line,
/// of the first place of the file that it or its traceback tells, as a
/// call of the host's that failed tells it there alone, and its first
/// line, of mlua's words and the file's place left out.
fn failure(file: &str, message: &str) -> Failure {
    let place = format!("{file}:");
    let line = message.lines().find_map(|line| {
        let at = line.find(&place)?;
        let rest = &line[at + place.len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    });
    let mut first = message.lines().next().unwrap_or_default().trim();
    for words in ["callback error:", "runtime error:", "error converting"] {
        first = first.strip_prefix(words).map_or(first, str::trim);
    }
    if let Some(at) = first.find(&place) {
        let rest = &first[at + place.len()..];
        if let Some((_, after)) = rest.split_once(':') {
            first = after.trim();
        }
    }
    Failure {
        line,
        message: first.to_owned(),
    }
}

/// The plugin of a test, and its host.
struct Hosted {
    host: FakeHost,
    plugin: LuauPlugin,
}

/// A command of the app's registry, and the plugin that registered it, if
/// one did, as `luau/commands.json` holds it.
#[derive(Deserialize)]
struct Registered {
    #[serde(flatten)]
    command: Command,
    plugin: Option<String>,
}

/// The app's commands and its first-party plugins', as the registry of an
/// app of the default distribution holds them, which `lokyr-app-core`'s
/// tests write and check.
fn registry() -> &'static [Registered] {
    static REGISTRY: OnceLock<Vec<Registered>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        serde_json::from_str(include_str!("../luau/commands.json"))
            .expect("luau/commands.json reads, as the app's tests wrote it")
    })
}

/// The `TestHost` of a test of `plugin`, in `lua`: the plugin made anew, of
/// a fresh VM, of its package's `src/` alone, on a fake host of its
/// manifest's capabilities, commands and slots, of the app's registry, and
/// of its messages, of the pane 1, which has the focus.
fn test_host(lua: &Lua, plugin: &Plugin<'_>) -> Result<Table, String> {
    let mut host = FakeHost::new(plugin.name)
        .with_capabilities(plugin.capabilities.to_vec())
        .with_uses(plugin.uses.to_vec())
        .with_slots(plugin.slots.to_vec());
    host.set_registry(
        registry()
            .iter()
            .map(|Registered { command, plugin }| CommandInfo {
                name: command.name.clone(),
                title: command.title.clone(),
                category: command.category,
                params: command.params.clone(),
                plugin: plugin.clone(),
                keys: None,
                when: command.when.clone(),
                enabled: Some(true),
            })
            .collect(),
    );
    // Of their capabilities, which a run of them needs.
    let mut host = host.with_commands(
        registry()
            .iter()
            .map(|registered| registered.command.clone())
            .collect(),
    );
    host.pane(PaneId(1));
    host.focus(Some(PaneId(1)));
    for (id, text) in plugin.messages {
        host.set_message(id, &[], Some(text));
    }
    let package = Package::new(plugin.package.paths().filter_map(|path| {
        let source = plugin.package.source(path)?;
        path.starts_with("src/")
            .then(|| (path.to_owned(), source.to_owned()))
    }));
    let hosted = Arc::new(Mutex::new(Hosted {
        host,
        plugin: LuauPlugin::new(plugin.name, package, Execution::Native),
    }));
    functions(lua, &hosted).map_err(|error| error.to_string())
}

/// Runs `call` of the test's plugin and host.
fn with<T>(hosted: &Arc<Mutex<Hosted>>, call: impl FnOnce(&mut Hosted) -> T) -> T {
    call(&mut hosted.lock().unwrap_or_else(PoisonError::into_inner))
}

/// The error of the plugin's call that failed, as a test's error.
fn failed(error: &lokyr_plugin_sdk::command::Error) -> mlua::Error {
    mlua::Error::runtime(format!("{} ({})", error.message, error.code.number()))
}

/// The arguments of `value`, a table of a command's, or none.
fn args_of(value: Option<Table>) -> mlua::Result<Args> {
    match value {
        Some(table) => shape::from_lua(Value::Table(table)).map_err(mlua::Error::runtime),
        None => Ok(Args::new()),
    }
}

/// The table of a `TestHost`'s functions, of `hosted`.
fn functions(lua: &Lua, hosted: &Arc<Mutex<Hosted>>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    let of = || Arc::clone(hosted);

    let hosted = of();
    table.raw_set(
        "activate",
        lua.create_function(move |_, ()| {
            with(&hosted, |Hosted { host, plugin }| {
                host.activate(plugin).map_err(|error| failed(&error))
            })
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "command",
        lua.create_function(move |_, (name, args): (String, Option<Table>)| {
            let args = args_of(args)?;
            with(&hosted, |Hosted { host, plugin }| {
                host.run_command(plugin, &name, args)
                    .map_err(|error| failed(&error))
            })
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "applies",
        lua.create_function(move |_, (name, args): (String, Option<Table>)| {
            let args = args_of(args)?;
            Ok(with(&hosted, |Hosted { host, plugin }| {
                host.command_applies(plugin, &name, &args)
            }))
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "events",
        lua.create_function(move |lua, list: Table| {
            let mut taken = Vec::new();
            for event in list.sequence_values::<Value>() {
                taken.push(events::from_lua(lua, event?).map_err(mlua::Error::runtime)?);
            }
            // Delivered as the app delivers them: those that it subscribed
            // to, and that it may read.
            with(&hosted, |Hosted { host, plugin }| {
                for event in taken {
                    host.post(event);
                }
                host.deliver(plugin).map_err(|error| failed(&error))
            })
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "ui_event",
        lua.create_function(move |lua, event: Value| {
            let event = events::ui_from_lua(lua, event).map_err(mlua::Error::runtime)?;
            with(&hosted, |Hosted { host, plugin }| {
                host.ui_event(plugin, &event)
                    .map_err(|error| failed(&error))
            })
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "render",
        lua.create_function(move |lua, view: Value| {
            let view: ViewId = shape::from_lua(view).map_err(mlua::Error::runtime)?;
            let tree = with(&hosted, |Hosted { host, plugin }| {
                host.render(plugin, &view)
            });
            match tree {
                Some(tree) => Ok(Value::Table(
                    tree::to_lua(lua, &tree).map_err(mlua::Error::runtime)?,
                )),
                None => Ok(Value::Nil),
            }
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "panels",
        lua.create_function(move |lua, ()| {
            let panels: Vec<String> = with(&hosted, |Hosted { host, .. }| {
                host.panels().keys().map(|id| id.0.clone()).collect()
            });
            lua.create_sequence_from(panels)
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "advance",
        lua.create_function(move |_, milliseconds: f64| {
            let by = Duration::from_millis(milliseconds.max(0.0) as u64);
            with(&hosted, |Hosted { host, plugin }| {
                host.advance(by);
                host.deliver(plugin).map_err(|error| failed(&error))
            })
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "done",
        lua.create_function(move |lua, ()| {
            let done = with(&hosted, |Hosted { host, .. }| host.take_done());
            let table = lua.create_table()?;
            for request in &done {
                table.raw_push(shape::to_lua(lua, request).map_err(mlua::Error::runtime)?)?;
            }
            Ok(table)
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "commands",
        lua.create_function(move |lua, ()| {
            let names: Vec<String> = with(&hosted, |Hosted { host, .. }| {
                host.commands()
                    .iter()
                    .map(|command| command.name.clone())
                    .collect()
            });
            lua.create_sequence_from(names)
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "clipboard",
        lua.create_function(move |_, ()| {
            Ok(with(&hosted, |Hosted { host, .. }| {
                host.clipboard().map(str::to_owned)
            }))
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "set_clipboard",
        lua.create_function(move |_, text: Option<String>| {
            with(&hosted, |Hosted { host, .. }| {
                host.set_clipboard(text.as_deref());
            });
            Ok(())
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "set_message",
        lua.create_function(move |_, (id, text): (String, Option<String>)| {
            with(&hosted, |Hosted { host, .. }| {
                host.set_message(&id, &[], text.as_deref());
            });
            Ok(())
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "state",
        lua.create_function(move |lua, ()| {
            let state = with(&hosted, |Hosted { host, .. }| host.state().clone());
            shape::to_lua(lua, &state).map_err(mlua::Error::runtime)
        })?,
    )?;
    let hosted = of();
    table.raw_set(
        "set_config",
        lua.create_function(move |_, config: Value| {
            let config: Data = shape::from_lua(config).map_err(mlua::Error::runtime)?;
            with(&hosted, |Hosted { host, .. }| {
                host.set_config(config);
            });
            Ok(())
        })?,
    )?;
    table.set_readonly(true);
    Ok(table)
}

#[cfg(test)]
mod tests;
