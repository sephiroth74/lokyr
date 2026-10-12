//! Runs the plugin-like workloads as Lua code, with LuaJIT or Luau, so Lua
//! plugins can be compared with WebAssembly ones on the same work.

use std::cell::Cell;
use std::path::Path;

use mlua::{Function, Lua, LuaOptions, Nil, StdLib, Table, ffi};
use wasmtime::Result;

use crate::bench::{
    FUZZY_QUERY, VISIBLE_LINES, WORKLOAD_CALLS, WORKLOAD_LINES, median_native, median_of,
    native_fuzzy, native_highlight, us,
};
use crate::footprint;

/// The Lua runtime this build links.
pub const RUNTIME: &str = if cfg!(feature = "luajit") {
    "luajit"
} else if cfg!(feature = "luau-jit") {
    "luau-jit"
} else {
    "luau"
};

/// Spans of one line, as `(start, len, kind)`.
type Spans = Vec<(u32, u32, u8)>;

/// Base library functions a plugin must not reach: they load code, read
/// files, change environments or drive the garbage collector. Luau's
/// `require` is a stub, without a module loader, but it goes too.
const REMOVED_GLOBALS: [&str; 10] = [
    "collectgarbage",
    "dofile",
    "gcinfo",
    "getfenv",
    "jit",
    "load",
    "loadfile",
    "loadstring",
    "require",
    "setfenv",
];

/// A Lua state set up the way a plugin host would run plugins. It has only
/// the libraries that compute (strings, tables, math, bits) and none of the
/// functions in [`REMOVED_GLOBALS`], nor `string.dump`. Luau also gets its
/// sandbox and its optimizing compiler. `jit` turns the JIT compiler on or
/// off, where the runtime has one. Also returns whether the JIT is on.
pub fn new_state(jit: bool) -> Result<(Lua, bool)> {
    #[cfg(feature = "luajit")]
    let (lua, jit_active) = {
        let libs = StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::BIT | StdLib::JIT;
        let lua = Lua::new_with(libs, LuaOptions::default()).map_err(lua_error)?;
        // The host sets the compiler's mode, then takes the switch away.
        let switch = if jit { "jit.on()" } else { "jit.off()" };
        lua.load(switch).exec().map_err(lua_error)?;
        let jit_active: bool = lua
            .load("return (jit.status())")
            .eval()
            .map_err(lua_error)?;
        (lua, jit_active)
    };
    #[cfg(feature = "luau")]
    let (lua, jit_active) = {
        let libs = StdLib::STRING
            | StdLib::TABLE
            | StdLib::MATH
            | StdLib::BIT
            | StdLib::UTF8
            | StdLib::BUFFER;
        let lua = Lua::new_with(libs, LuaOptions::default()).map_err(lua_error)?;
        lua.set_compiler(mlua::chunk::Compiler::new().set_optimization_level(2));
        // Native code generation applies to the chunks loaded afterwards.
        #[cfg(feature = "luau-jit")]
        lua.enable_jit(jit);
        (lua, cfg!(feature = "luau-jit") && jit)
    };
    let globals = lua.globals();
    for name in REMOVED_GLOBALS {
        globals.raw_set(name, Nil).map_err(lua_error)?;
    }
    let string: Table = globals.raw_get("string").map_err(lua_error)?;
    string.raw_set("dump", Nil).map_err(lua_error)?;
    // Luau's sandbox makes the libraries and globals read-only, so it comes
    // last.
    #[cfg(feature = "luau")]
    lua.sandbox(true).map_err(lua_error)?;
    Ok((lua, jit_active))
}

/// Times the workloads defined by `script` and prints the medians as JSON.
/// `jit` turns the runtime's JIT compiler on or off.
pub fn workloads(script: &Path, jit: bool) -> Result<()> {
    let footprint_start = footprint::current()?;
    let (lua, jit_active) = new_state(jit)?;
    let source = std::fs::read_to_string(script)?;
    let module: Table = lua
        .load(source)
        .set_name("workloads")
        .eval()
        .map_err(lua_error)?;
    let function = |name: &str| -> Result<Function> { module.get(name).map_err(lua_error) };
    let line_count = function("line_count")?;
    let fuzzy = function("fuzzy")?;
    let fuzzy_pattern = function("fuzzy_pattern")?;
    let highlight = function("highlight")?;
    let highlight_bytes = function("highlight_bytes")?;
    lua.gc_collect().map_err(lua_error)?;
    let heap_loaded = lua.used_memory();
    let footprint_loaded = footprint::current()?;

    let lines = spike_workload::sample_lines(WORKLOAD_LINES);
    let visible = &lines[..VISIBLE_LINES];
    // The largest heap seen right after a call, while its result is alive.
    let heap_peak = Cell::new(0);
    let sample_heap = || heap_peak.set(heap_peak.get().max(lua.used_memory()));
    // Every call hands the lines over afresh, as the WebAssembly host does.
    let table = |lines: &[String]| -> Result<Table> {
        lua.create_sequence_from(lines.iter().map(String::as_str))
            .map_err(lua_error)
    };
    let find = |function: &Function| -> Result<Vec<u32>> {
        let matches: Table = function
            .call((table(&lines)?, FUZZY_QUERY))
            .map_err(lua_error)?;
        sample_heap();
        matches
            .sequence_values::<u32>()
            .collect::<mlua::Result<_>>()
            .map_err(lua_error)
    };
    let spans = |function: &Function, lines: &[String]| -> Result<Vec<Spans>> {
        let result: Table = function.call(table(lines)?).map_err(lua_error)?;
        sample_heap();
        read_spans(&result)
    };

    // The Lua code must produce exactly what the Rust code produces.
    let expected_matches: Vec<u32> = (0..)
        .zip(&lines)
        .filter(|(_, line)| spike_workload::fuzzy_score(line, FUZZY_QUERY).is_some())
        .map(|(index, _)| index)
        .collect();
    let expected_spans: Vec<Spans> = lines
        .iter()
        .map(|line| spike_workload::highlight(line))
        .collect();
    assert_eq!(find(&fuzzy)?, expected_matches, "fuzzy differs from Rust");
    assert_eq!(
        find(&fuzzy_pattern)?,
        expected_matches,
        "fuzzy_pattern differs from Rust"
    );
    assert_eq!(
        spans(&highlight, &lines)?,
        expected_spans,
        "highlight differs from Rust"
    );
    assert_eq!(
        spans(&highlight_bytes, &lines)?,
        expected_spans,
        "highlight_bytes differs from Rust"
    );

    let copy = median_of(WORKLOAD_CALLS, || {
        let count: usize = line_count.call(table(&lines)?).map_err(lua_error)?;
        assert_eq!(count, lines.len());
        Ok(())
    })?;
    let fuzzy_time = median_of(WORKLOAD_CALLS, || find(&fuzzy).map(drop))?;
    let fuzzy_pattern_time = median_of(WORKLOAD_CALLS, || find(&fuzzy_pattern).map(drop))?;
    let fuzzy_native = median_native(WORKLOAD_CALLS, || native_fuzzy(&lines));
    let highlight_all = median_of(WORKLOAD_CALLS, || spans(&highlight, &lines).map(drop))?;
    let highlight_bytes_all =
        median_of(WORKLOAD_CALLS, || spans(&highlight_bytes, &lines).map(drop))?;
    let highlight_all_native = median_native(WORKLOAD_CALLS, || native_highlight(&lines));
    let highlight_visible = median_of(WORKLOAD_CALLS, || spans(&highlight, visible).map(drop))?;
    let highlight_bytes_visible = median_of(WORKLOAD_CALLS, || {
        spans(&highlight_bytes, visible).map(drop)
    })?;
    let highlight_visible_native = median_native(WORKLOAD_CALLS, || native_highlight(visible));
    // The part of a highlighter's time spent reading its result back.
    let produced: Table = highlight_bytes.call(table(&lines)?).map_err(lua_error)?;
    let read_back = median_of(WORKLOAD_CALLS, || read_spans(&produced).map(drop))?;
    assert_eq!(read_spans_raw(&lua, &produced)?, expected_spans);
    let read_back_raw = median_of(WORKLOAD_CALLS, || read_spans_raw(&lua, &produced).map(drop))?;
    drop(produced);
    lua.gc_collect().map_err(lua_error)?;
    let heap_end = lua.used_memory();
    let footprint_end = footprint::current()?;

    println!("{{");
    println!(r#"  "runtime": "{RUNTIME}","#);
    println!(r#"  "jit_active": {jit_active},"#);
    println!(r#"  "lines": {WORKLOAD_LINES},"#);
    println!(r#"  "fuzzy_matches": {},"#, expected_matches.len());
    println!(
        r#"  "spans": {},"#,
        expected_spans.iter().map(Vec::len).sum::<usize>()
    );
    println!(r#"  "heap_loaded_kb": {},"#, heap_loaded / 1024);
    println!(r#"  "heap_peak_kb": {},"#, heap_peak.get() / 1024);
    println!(r#"  "heap_end_kb": {},"#, heap_end / 1024);
    println!(r#"  "footprint_start_kb": {},"#, footprint_start / 1024);
    println!(r#"  "footprint_loaded_kb": {},"#, footprint_loaded / 1024);
    println!(r#"  "footprint_end_kb": {},"#, footprint_end / 1024);
    println!(r#"  "copy_lines_us": {:.0},"#, us(copy));
    println!(r#"  "fuzzy_us": {:.0},"#, us(fuzzy_time));
    println!(r#"  "fuzzy_pattern_us": {:.0},"#, us(fuzzy_pattern_time));
    println!(r#"  "fuzzy_native_us": {:.0},"#, us(fuzzy_native));
    println!(r#"  "highlight_all_us": {:.0},"#, us(highlight_all));
    println!(
        r#"  "highlight_bytes_all_us": {:.0},"#,
        us(highlight_bytes_all)
    );
    println!(
        r#"  "highlight_all_native_us": {:.0},"#,
        us(highlight_all_native)
    );
    println!(r#"  "read_spans_us": {:.0},"#, us(read_back));
    println!(r#"  "read_spans_raw_us": {:.0},"#, us(read_back_raw));
    println!(r#"  "highlight_visible_us": {:.1},"#, us(highlight_visible));
    println!(
        r#"  "highlight_bytes_visible_us": {:.1},"#,
        us(highlight_bytes_visible)
    );
    println!(
        r#"  "highlight_visible_native_us": {:.1}"#,
        us(highlight_visible_native)
    );
    println!("}}");
    Ok(())
}

/// Reads the spans a highlighter returned: one table per line, holding
/// `{start, len, kind}` tables.
fn read_spans(result: &Table) -> Result<Vec<Spans>> {
    result
        .sequence_values::<Table>()
        .map(|line| {
            let line = line.map_err(lua_error)?;
            line.sequence_values::<Table>()
                .map(|span| {
                    let span = span.map_err(lua_error)?;
                    Ok((
                        span.raw_get(1).map_err(lua_error)?,
                        span.raw_get(2).map_err(lua_error)?,
                        span.raw_get(3).map_err(lua_error)?,
                    ))
                })
                .collect()
        })
        .collect()
}

/// Reads the same spans as [`read_spans`] through Lua's C API, without
/// mlua's safe wrappers: the floor of what reading tables costs.
fn read_spans_raw(lua: &Lua, result: &Table) -> Result<Vec<Spans>> {
    let mut spans = Vec::new();
    // SAFETY: the closure runs with `result` at the top of the stack, pushes
    // at most five values at a time and pops all of them. The values it
    // reads are integers within range, as `highlight_bytes` produced them.
    unsafe {
        lua.exec_raw::<()>(result, |state| {
            for line in 1..=ffi::lua_rawlen(state, -1) {
                ffi::lua_rawgeti(state, -1, line as ffi::lua_Integer);
                let count = ffi::lua_rawlen(state, -1);
                let mut line_spans = Vec::with_capacity(count);
                for span in 1..=count {
                    ffi::lua_rawgeti(state, -1, span as ffi::lua_Integer);
                    ffi::lua_rawgeti(state, -1, 1);
                    ffi::lua_rawgeti(state, -2, 2);
                    ffi::lua_rawgeti(state, -3, 3);
                    line_spans.push((
                        ffi::lua_tointeger(state, -3) as u32,
                        ffi::lua_tointeger(state, -2) as u32,
                        ffi::lua_tointeger(state, -1) as u8,
                    ));
                    ffi::lua_pop(state, 4);
                }
                spans.push(line_spans);
                ffi::lua_pop(state, 1);
            }
        })
    }
    .map_err(lua_error)?;
    Ok(spans)
}

/// Turns a Lua error into the host's error type.
pub fn lua_error(error: mlua::Error) -> wasmtime::Error {
    wasmtime::Error::msg(error.to_string())
}
