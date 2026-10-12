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

impl saetta::spike::host_log::Host for HostState {
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

/// Runs the guest and prints the metrics as JSON.
///
/// With `cached`, the component is loaded from the artifact a previous run
/// left in the cache, as on every launch after the first one.
pub fn run(path: &Path, cached: bool) -> Result<()> {
    let footprint_start = footprint::current()?;
    let bytes = std::fs::read(path)?;

    let started = Instant::now();
    let engine = engine()?;
    let engine_time = started.elapsed();
    let footprint_engine = footprint::current()?;

    let cache_dir = std::env::temp_dir().join("saetta-spike");
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
    // SAFETY: the file was written by `precompile_component` with an engine
    // of the same version and configuration, in this run or a previous one.
    let component = unsafe { Component::deserialize_file(&engine, &cache_file)? };
    let deserialize_time = started.elapsed();
    let footprint_deserialized = footprint::current()?;

    // The host's own interfaces first; then, instead of a WASI
    // implementation, every other import traps if it is ever called.
    let mut linker = Linker::<HostState>::new(&engine);
    saetta::spike::host_log::add_to_linker::<_, HasSelf<_>>(&mut linker, |state| state)?;
    linker.define_unknown_imports_as_traps(&component)?;
    let mut store = Store::new(&engine, HostState::default());

    let started = Instant::now();
    let plugin = Plugin::instantiate(&mut store, &component, &linker)?;
    let instantiate_time = started.elapsed();
    let footprint_instantiated = footprint::current()?;
    let guest = plugin.saetta_spike_bench();
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

/// Times `calls` calls of `call` one by one and returns the median.
fn median_of(calls: usize, mut call: impl FnMut() -> Result<()>) -> Result<Duration> {
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

fn us(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}
