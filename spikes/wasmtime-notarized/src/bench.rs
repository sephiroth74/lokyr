//! Hosts the guest component with wasmtime and measures each phase: engine
//! creation, ahead-of-time compilation, loading from the cache, instantiation,
//! and the cost of calls into the guest.

use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Result, Store};

use crate::footprint;

wasmtime::component::bindgen!({
    path: "wit",
    world: "plugin",
});

/// What the host keeps for one plugin instance.
#[derive(Debug, Default)]
struct HostState {
    /// Messages the plugin sent through `host-log`.
    logs: Vec<String>,
}

impl bolt::spike::host_log::Host for HostState {
    fn log(&mut self, message: String) {
        self.logs.push(message);
    }
}

/// Calls timed for the `noop` latency.
const NOOP_CALLS: usize = 10_000;
/// Calls timed for the `render` latency.
const RENDER_CALLS: usize = 1_000;
/// Nodes per `render` call.
const RENDER_NODES: u32 = 100;
/// Iterations of the CPU-bound loop.
const WORK_ITERATIONS: u32 = 50_000_000;
/// Lines of the buffer the workloads run on.
pub(crate) const WORKLOAD_LINES: usize = 5_000;
/// Lines visible on screen at once.
pub(crate) const VISIBLE_LINES: usize = 40;
/// Calls timed per workload.
pub(crate) const WORKLOAD_CALLS: usize = 20;
/// The fuzzy query; it matches about one line in sixteen.
pub(crate) const FUZZY_QUERY: &str = "users500";

/// How guest code runs in this build: native code from Cranelift, or
/// Pulley bytecode run by wasmtime's interpreter.
const VARIANT: &str = if cfg!(feature = "pulley") {
    "pulley"
} else {
    "cranelift"
};

/// The engine configuration of this build's variant.
fn engine() -> Result<Engine> {
    let mut config = Config::new();
    if cfg!(feature = "pulley") {
        config.target("pulley64")?;
    }
    Engine::new(&config)
}

/// Lists the imports of a component, one per line.
pub fn print_imports(path: &Path) -> Result<()> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, path)?;
    let component_type = component.component_type();
    let imports: Vec<&str> = component_type
        .imports(&engine)
        .map(|(name, _)| name)
        .collect();
    if imports.is_empty() {
        println!("(no imports)");
    }
    for name in imports {
        println!("{name}");
    }
    Ok(())
}

/// How [`run`] gets the component.
#[derive(Debug, Default, Clone, Copy)]
pub struct Options {
    /// Load the artifact a previous run left in the cache, as on every launch
    /// after the first one, instead of compiling the component.
    pub cached: bool,
    /// Read the artifact into anonymous memory instead of mapping the file.
    /// The hardened runtime treats the two differently.
    pub from_memory: bool,
}

/// Runs the guest and prints the metrics as JSON.
pub fn run(path: &Path, options: Options) -> Result<()> {
    let Options {
        cached,
        from_memory,
    } = options;
    let footprint_start = footprint::current()?;
    let bytes = std::fs::read(path)?;

    let started = Instant::now();
    let engine = engine()?;
    let engine_time = started.elapsed();
    let footprint_engine = footprint::current()?;

    let cache_dir = std::env::temp_dir().join("bolt-spike");
    std::fs::create_dir_all(&cache_dir)?;
    // One artifact per guest and variant: they are not interchangeable.
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("guest");
    let cache_file = cache_dir.join(format!("{stem}-{VARIANT}.cwasm"));
    let compile_time = if cached {
        None
    } else {
        let started = Instant::now();
        let compiled = engine.precompile_component(&bytes)?;
        let elapsed = started.elapsed();
        std::fs::write(&cache_file, &compiled)?;
        Some(elapsed)
    };
    let cwasm_bytes = std::fs::metadata(&cache_file)?.len();
    let footprint_compiled = footprint::current()?;

    let started = Instant::now();
    let component = if from_memory {
        let artifact = std::fs::read(&cache_file)?;
        // SAFETY: the bytes come from `precompile_component` with an engine of
        // the same version and configuration, in this run or a previous one.
        unsafe { Component::deserialize(&engine, &artifact)? }
    } else {
        // SAFETY: as above, for the file itself.
        unsafe { Component::deserialize_file(&engine, &cache_file)? }
    };
    let deserialize_time = started.elapsed();
    let footprint_deserialized = footprint::current()?;

    // The host's own interfaces first; then, instead of a WASI
    // implementation, every other import traps if it is ever called.
    let mut linker = Linker::<HostState>::new(&engine);
    bolt::spike::host_log::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
    linker.define_unknown_imports_as_traps(&component)?;
    let mut store = Store::new(&engine, HostState::default());

    let started = Instant::now();
    let plugin = Plugin::instantiate(&mut store, &component, &linker)?;
    let instantiate_time = started.elapsed();
    let footprint_instantiated = footprint::current()?;
    let guest = plugin.bolt_spike_bench();
    guest.call_init(&mut store)?;

    let noop = median_of(NOOP_CALLS, || guest.call_noop(&mut store))?;
    let render = median_of(RENDER_CALLS, || {
        guest.call_render(&mut store, RENDER_NODES).map(|nodes| {
            assert_eq!(nodes.len(), RENDER_NODES as usize);
        })
    })?;

    let started = Instant::now();
    let guest_sum = guest.call_work(&mut store, WORK_ITERATIONS)?;
    let work_time = started.elapsed();

    let started = Instant::now();
    let native_sum = native_work(black_box(WORK_ITERATIONS));
    let native_time = started.elapsed();
    assert_eq!(
        guest_sum, native_sum,
        "the guest and the host compute the same loop"
    );

    let footprint_end = footprint::current()?;

    // Last, because a trap leaves the instance unusable.
    let panic_outcome = match guest.call_fail(&mut store) {
        Ok(()) => "returned".to_owned(),
        Err(error) => error.root_cause().to_string().replace('"', "'"),
    };
    let panic_message = store.data().logs.last().map_or_else(
        || "(none)".to_owned(),
        |message| message.replace('"', "'").replace('\n', " / "),
    );

    println!("{{");
    println!(r#"  "variant": "{VARIANT}","#);
    println!(r#"  "guest": "{}","#, path.display());
    println!(r#"  "cached": {cached},"#);
    println!(
        r#"  "load": "{}","#,
        if from_memory { "memory" } else { "file" }
    );
    println!(r#"  "component_bytes": {},"#, bytes.len());
    println!(r#"  "cwasm_bytes": {cwasm_bytes},"#);
    println!(r#"  "engine_ms": {:.3},"#, ms(engine_time));
    match compile_time {
        Some(time) => println!(r#"  "compile_ms": {:.3},"#, ms(time)),
        None => println!(r#"  "compile_ms": null,"#),
    }
    println!(r#"  "deserialize_ms": {:.3},"#, ms(deserialize_time));
    println!(r#"  "instantiate_ms": {:.3},"#, ms(instantiate_time));
    println!(r#"  "noop_median_ns": {},"#, noop.as_nanos());
    println!(r#"  "render_{RENDER_NODES}_median_us": {:.1},"#, us(render));
    println!(r#"  "work_ms": {:.1},"#, ms(work_time));
    println!(r#"  "native_work_ms": {:.1},"#, ms(native_time));
    println!(
        r#"  "work_slowdown": {:.2},"#,
        work_time.as_secs_f64() / native_time.as_secs_f64()
    );
    println!(r#"  "footprint_start_kb": {},"#, footprint_start / 1024);
    println!(r#"  "footprint_engine_kb": {},"#, footprint_engine / 1024);
    println!(
        r#"  "footprint_compiled_kb": {},"#,
        footprint_compiled / 1024
    );
    println!(
        r#"  "footprint_deserialized_kb": {},"#,
        footprint_deserialized / 1024
    );
    println!(
        r#"  "footprint_instantiated_kb": {},"#,
        footprint_instantiated / 1024
    );
    println!(r#"  "footprint_end_kb": {},"#, footprint_end / 1024);
    println!(r#"  "panic_outcome": "{panic_outcome}","#);
    println!(r#"  "panic_message": "{panic_message}""#);
    println!("}}");
    Ok(())
}

/// Times plugin-like work on a buffer of lines, in the guest and natively,
/// and prints the medians as JSON.
pub fn workloads(path: &Path) -> Result<()> {
    let engine = engine()?;
    let compiled = engine.precompile_component(&std::fs::read(path)?)?;
    // SAFETY: the bytes were produced just above by `precompile_component`
    // with this engine.
    let component = unsafe { Component::deserialize(&engine, &compiled)? };
    let mut linker = Linker::<HostState>::new(&engine);
    bolt::spike::host_log::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
    linker.define_unknown_imports_as_traps(&component)?;
    let mut store = Store::new(&engine, HostState::default());
    let plugin = Plugin::instantiate(&mut store, &component, &linker)?;
    let guest = plugin.bolt_spike_bench();
    guest.call_init(&mut store)?;

    let lines = spike_workload::sample_lines(WORKLOAD_LINES);
    let visible = &lines[..VISIBLE_LINES];
    let bytes: usize = lines.iter().map(String::len).sum();

    let copy = median_of(WORKLOAD_CALLS, || {
        guest.call_line_count(&mut store, &lines).map(drop)
    })?;
    let fuzzy = median_of(WORKLOAD_CALLS, || {
        guest.call_fuzzy(&mut store, &lines, FUZZY_QUERY).map(drop)
    })?;
    let fuzzy_native = median_native(WORKLOAD_CALLS, || native_fuzzy(&lines));
    let highlight_all = median_of(WORKLOAD_CALLS, || {
        guest.call_highlight(&mut store, &lines).map(drop)
    })?;
    let highlight_all_native = median_native(WORKLOAD_CALLS, || native_highlight(&lines));
    let highlight_visible = median_of(WORKLOAD_CALLS, || {
        guest.call_highlight(&mut store, visible).map(drop)
    })?;
    let highlight_visible_native = median_native(WORKLOAD_CALLS, || native_highlight(visible));

    let matches = guest.call_fuzzy(&mut store, &lines, FUZZY_QUERY)?;
    assert_eq!(matches.len(), native_fuzzy(&lines), "guest and host agree");
    let spans: usize = guest
        .call_highlight(&mut store, &lines)?
        .iter()
        .map(Vec::len)
        .sum();

    println!("{{");
    println!(r#"  "variant": "{VARIANT}","#);
    println!(r#"  "lines": {WORKLOAD_LINES},"#);
    println!(r#"  "bytes": {bytes},"#);
    println!(r#"  "fuzzy_matches": {},"#, matches.len());
    println!(r#"  "spans": {spans},"#);
    println!(r#"  "copy_lines_us": {:.0},"#, us(copy));
    println!(r#"  "fuzzy_us": {:.0},"#, us(fuzzy));
    println!(r#"  "fuzzy_native_us": {:.0},"#, us(fuzzy_native));
    println!(r#"  "highlight_all_us": {:.0},"#, us(highlight_all));
    println!(
        r#"  "highlight_all_native_us": {:.0},"#,
        us(highlight_all_native)
    );
    println!(r#"  "highlight_visible_us": {:.1},"#, us(highlight_visible));
    println!(
        r#"  "highlight_visible_native_us": {:.1}"#,
        us(highlight_visible_native)
    );
    println!("}}");
    Ok(())
}

/// The guest's fuzzy search, natively: the number of matching lines.
pub(crate) fn native_fuzzy(lines: &[String]) -> usize {
    lines
        .iter()
        .filter(|line| spike_workload::fuzzy_score(line, FUZZY_QUERY).is_some())
        .count()
}

/// The guest's highlighter, natively: the number of spans.
pub(crate) fn native_highlight(lines: &[String]) -> usize {
    lines
        .iter()
        .map(|line| spike_workload::highlight(line).len())
        .sum()
}

/// Times `calls` calls of `work` one by one and returns the median.
pub(crate) fn median_native<T>(calls: usize, mut work: impl FnMut() -> T) -> Duration {
    let mut samples = Vec::with_capacity(calls);
    for _ in 0..calls {
        let started = Instant::now();
        black_box(work());
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Times `calls` calls of `call` one by one and returns the median.
pub(crate) fn median_of(calls: usize, mut call: impl FnMut() -> Result<()>) -> Result<Duration> {
    let mut samples = Vec::with_capacity(calls);
    for _ in 0..calls {
        let started = Instant::now();
        call()?;
        samples.push(started.elapsed());
    }
    samples.sort_unstable();
    Ok(samples[samples.len() / 2])
}

/// The guest's CPU-bound loop, compiled natively, as a reference.
fn native_work(iterations: u32) -> u64 {
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut sum: u64 = 0;
    for _ in 0..iterations {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        sum = sum.wrapping_add(state);
    }
    black_box(sum)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

pub(crate) fn us(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}
