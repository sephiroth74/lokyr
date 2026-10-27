//! Hostile plugins against the Lua runtimes. Can the host stop an infinite
//! loop, a memory bomb or a runaway recursion? What can a plugin reach
//! outside its computation? And what does the time limit cost?

use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use mlua::{Function, Lua, Table, VmState, ffi};
use wasmtime::Result;

use crate::bench::{FUZZY_QUERY, WORKLOAD_CALLS, WORKLOAD_LINES, median_of, us};
use crate::lua_bench::{RUNTIME, lua_error, new_state};

/// How long a call may run before the host stops it.
const DEADLINE: Duration = Duration::from_millis(100);
/// How long the host waits for a call before it gives up on it.
const GIVE_UP: Duration = Duration::from_secs(2);
/// The heap a plugin may use.
const MEMORY_LIMIT: usize = 32 << 20;

/// Plugins that would never return on their own, or would take the host down.
const HOSTILE: [(&str, &str); 4] = [
    ("infinite loop", "while true do end"),
    (
        "memory bomb",
        "local t, i = {}, 0 while true do i = i + 1; t[i] = string.rep('x', 1024) .. i end",
    ),
    ("huge string", "local s = string.rep('x', 1e9)"),
    (
        "runaway recursion",
        "local function f() return 1 + f() end f()",
    ),
];

/// Tries what a plugin could reach outside its computation, and returns one
/// line per attempt. No attempt changes anything outside the Lua state.
const ESCAPES: &str = r#"
local results = {}
local function probe(name, attempt)
    local ok, err = pcall(attempt)
    local outcome = ok and "REACHABLE" or ("blocked: " .. tostring(err):match("[^\n]*"))
    results[#results + 1] = "  " .. name .. ": " .. outcome
end
probe("read a file (io.open)", function() assert(io.open("/etc/hosts")):close() end)
probe("read the environment (os.getenv)", function() assert(os.getenv("HOME")) end)
probe("ask for a shell (os.execute)", function() assert(os.execute()) end)
probe("delete files (os.remove)", function() os.remove("/nonexistent/lokyr-probe") end)
probe("search files for modules (require)", function()
    local ok, err = pcall(require, "./lokyr-probe")
    assert(ok or tostring(err):find("no file"), err)
end)
probe("load C libraries (package.loadlib)", function()
    assert(package.loadlib("/usr/lib/libSystem.B.dylib", "getpid"))
end)
probe("raw memory (require 'ffi')", function() assert(require("ffi")) end)
probe("remove the host's hook (debug.sethook)", function() debug.sethook() end)
probe("reach the registry (debug.getregistry)", function() assert(debug.getregistry()) end)
probe("turn the JIT back on (jit.on)", function() jit.on() end)
probe("load source code (load)", function() assert(load("return 1"))() end)
probe("load source code (loadstring)", function() assert(loadstring("return 1"))() end)
probe("load bytecode (load + string.dump)", function()
    assert(load(string.dump(function() return 1 end)))()
end)
probe("change environments (getfenv)", function() assert(getfenv(0)) end)
probe("stop the collector (collectgarbage)", function()
    collectgarbage("stop")
    collectgarbage("restart")
end)
return table.concat(results, "\n")
"#;

/// How the host enforces the time limit.
#[derive(Clone, Copy)]
enum Guard {
    /// A callback that checks the clock while the code runs.
    Check,
    /// A thread that steps in only once the deadline has passed.
    Watchdog,
}

/// Runs the hostile plugins and the escape probes, then times the fuzzy
/// search of `script` under each guard. Prints the outcomes.
pub fn run(script: &Path, jit: bool) -> Result<()> {
    let (_, jit_active) = new_state(jit)?;
    println!("runtime: {RUNTIME}, JIT on: {jit_active}");
    println!(
        "limits: {} ms per call, {} MiB of heap",
        DEADLINE.as_millis(),
        MEMORY_LIMIT >> 20
    );
    for (name, source) in HOSTILE {
        println!("{name}: {}", contain(jit, Guard::Check, source));
    }
    println!(
        "infinite loop, with the watchdog: {}",
        contain(jit, Guard::Watchdog, HOSTILE[0].1)
    );
    println!("what a plugin reaches with mlua's Lua::new():");
    println!("{}", escapes(&Lua::new())?);
    println!("what a plugin reaches in the plugin state:");
    println!("{}", escapes(&new_state(jit)?.0)?);
    let plain = time_fuzzy(script, jit, None)?;
    let checked = time_fuzzy(script, jit, Some(Guard::Check))?;
    let watched = time_fuzzy(script, jit, Some(Guard::Watchdog))?;
    println!(
        "fuzzy search: {:.0} µs; {:.0} µs with the clock check; {:.0} µs with the watchdog",
        us(plain),
        us(checked),
        us(watched)
    );
    Ok(())
}

/// Runs `source` in a plugin state with the memory limit and the time limit,
/// on a thread of its own. Tells how the call ended, or that it did not end.
fn contain(jit: bool, guard: Guard, source: &'static str) -> String {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let outcome = (|| -> Result<String> {
            let (lua, _) = new_state(jit)?;
            lua.set_memory_limit(MEMORY_LIMIT).map_err(lua_error)?;
            let start = Instant::now();
            let _watchdog = arm(&lua, guard, start + DEADLINE)?;
            let result = lua.load(source).set_name("hostile").exec();
            let ms = start.elapsed().as_secs_f64() * 1e3;
            Ok(match result {
                Ok(()) => format!("returned after {ms:.0} ms"),
                Err(error) => format!("stopped after {ms:.0} ms ({})", first_line(&error)),
            })
        })();
        // The host may have given up on the call already.
        let _ = sender.send(outcome);
    });
    match receiver.recv_timeout(GIVE_UP) {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => format!("host error: {error}"),
        Err(RecvTimeoutError::Timeout) => {
            format!("NOT STOPPED, still running after {} s", GIVE_UP.as_secs())
        }
        Err(RecvTimeoutError::Disconnected) => "the plugin thread panicked".to_owned(),
    }
}

/// Sets up `guard` so that calls fail once `deadline` has passed. A watchdog
/// is returned, and must be dropped before `lua`.
fn arm(lua: &Lua, guard: Guard, deadline: Instant) -> Result<Option<Watchdog>> {
    match guard {
        Guard::Check => {
            set_clock_check(lua, deadline)?;
            Ok(None)
        }
        Guard::Watchdog => Watchdog::arm(lua, deadline).map(Some),
    }
}

/// Checks the clock while the code runs: with LuaJIT, from a hook called
/// every 1000 instructions; with Luau, from the interrupt, which runs at
/// every loop iteration and function call.
fn set_clock_check(lua: &Lua, deadline: Instant) -> Result<()> {
    let check = move || {
        if Instant::now() < deadline {
            Ok(VmState::Continue)
        } else {
            Err(mlua::Error::runtime("deadline exceeded"))
        }
    };
    #[cfg(feature = "luajit")]
    lua.set_hook(
        mlua::HookTriggers::new().every_nth_instruction(1000),
        move |_, _| check(),
    )
    .map_err(lua_error)?;
    #[cfg(feature = "luau")]
    lua.set_interrupt(move |_| check());
    Ok(())
}

/// A deadline that costs nothing until it passes. Then a thread installs the
/// runtime's way of stopping code: a LuaJIT hook, or Luau's interrupt. The
/// standalone `luajit` and `luau` programs handle Ctrl-C the same way.
struct Watchdog {
    cancel: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Watchdog {
    fn arm(lua: &Lua, deadline: Instant) -> Result<Self> {
        let mut address = 0;
        // SAFETY: the closure only records the address of the state.
        unsafe { lua.exec_raw::<()>((), |state| address = state as usize) }.map_err(lua_error)?;
        let (cancel, cancelled) = mpsc::channel::<()>();
        let thread = thread::spawn(move || {
            let wait = deadline.saturating_duration_since(Instant::now());
            if cancelled.recv_timeout(wait) == Err(RecvTimeoutError::Timeout) {
                // SAFETY: the state outlives the watchdog, whose drop joins
                // this thread. Writing the hook or the interrupt from another
                // thread races with the reads of the running code, as it does
                // from a signal handler in the standalone programs.
                unsafe { stop_soon(address as *mut ffi::lua_State) };
            }
        });
        Ok(Self {
            cancel: Some(cancel),
            thread: Some(thread),
        })
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        // Dropping the sender wakes the thread up.
        drop(self.cancel.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Makes the code running in `state` fail at its next check: a hook on every
/// instruction.
#[cfg(feature = "luajit")]
unsafe fn stop_soon(state: *mut ffi::lua_State) {
    unsafe extern "C-unwind" fn stop(state: *mut ffi::lua_State, _: *mut ffi::lua_Debug) {
        // SAFETY: LuaJIT calls hooks with a valid state, and they may raise
        // errors.
        unsafe {
            ffi::lua_sethook(state, None, 0, 0);
            ffi::luaL_error(state, c"deadline exceeded".as_ptr());
        }
    }
    // SAFETY: the caller guarantees that `state` is alive.
    unsafe { ffi::lua_sethook(state, Some(stop), ffi::LUA_MASKCOUNT, 1) };
}

/// Makes the code running in `state` fail at its next check: the interrupt,
/// at the next loop iteration or function call.
#[cfg(feature = "luau")]
unsafe fn stop_soon(state: *mut ffi::lua_State) {
    unsafe extern "C-unwind" fn stop(state: *mut ffi::lua_State, gc: std::ffi::c_int) {
        // The collector calls the interrupt too, where errors are not allowed.
        if gc >= 0 {
            return;
        }
        // SAFETY: Luau calls the interrupt with a valid state, and it may
        // raise errors.
        unsafe {
            (*ffi::lua_callbacks(state)).interrupt = None;
            ffi::lua_rawcheckstack(state, 1);
            ffi::luaL_error(state, c"deadline exceeded".as_ptr());
        }
    }
    // SAFETY: the caller guarantees that `state` is alive, so its callbacks
    // are too.
    unsafe {
        std::ptr::addr_of_mut!((*ffi::lua_callbacks(state)).interrupt).write_volatile(Some(stop));
    }
}

/// Runs the escape probes in `lua`.
fn escapes(lua: &Lua) -> Result<String> {
    lua.load(ESCAPES)
        .set_name("escapes")
        .eval()
        .map_err(lua_error)
}

/// The median time of the byte-loop fuzzy search of `script`, under `guard`
/// with a deadline that never passes.
fn time_fuzzy(script: &Path, jit: bool, guard: Option<Guard>) -> Result<Duration> {
    let (lua, _) = new_state(jit)?;
    let _watchdog = match guard {
        Some(guard) => arm(&lua, guard, Instant::now() + Duration::from_secs(3600))?,
        None => None,
    };
    let module: Table = lua
        .load(std::fs::read_to_string(script)?)
        .set_name("workloads")
        .eval()
        .map_err(lua_error)?;
    let fuzzy: Function = module.get("fuzzy").map_err(lua_error)?;
    let lines = spike_workload::sample_lines(WORKLOAD_LINES);
    median_of(WORKLOAD_CALLS, || {
        let table = lua
            .create_sequence_from(lines.iter().map(String::as_str))
            .map_err(lua_error)?;
        fuzzy
            .call::<Table>((table, FUZZY_QUERY))
            .map(drop)
            .map_err(lua_error)
    })
}

/// The first line of an error, without the stack traceback.
fn first_line(error: &mlua::Error) -> String {
    let message = error.to_string();
    message.lines().next().unwrap_or_default().to_owned()
}
