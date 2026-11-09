//! The budgets of a plugin of Luau, as a release build measures them: its
//! activation, from its VM's making, its code's compilation and its
//! modules' run to its `activate`'s return, and its VM's heap once active;
//! of the template that `sleip new` makes and of a plugin of about 2000
//! lines, in both executions, each run a fresh plugin on a fresh host. The
//! process's first activation, which makes what every VM shares, apart.
//! Ignored, as a debug build's times tell nothing: `just bench-luau`.

use std::path::Path;
use std::time::{Duration, Instant};

use lokyr_plugin_host_luau::{Execution, LuauPlugin, Package, native_supported};
use lokyr_plugin_sdk::fake::FakeHost;

/// The runs of each plugin and execution, after the first.
const RUNS: usize = 50;

/// The activation's budget, its compilation within.
const MOST_ACTIVATION: Duration = Duration::from_millis(5);

/// The budget of a plugin's memory once active.
const MOST_MEMORY: usize = 2 * 1024 * 1024;

/// The package of the template that `sleip new` makes: its sources.
fn template() -> Package {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("luau/template/src");
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(&folder).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        sources.push((
            format!("src/{name}"),
            std::fs::read_to_string(&path).unwrap(),
        ));
    }
    Package::new(sources)
}

/// A plugin of `handlers` commands' functions, about ten lines each, of
/// strings and tables, which registers ten of them as it activates.
fn large(handlers: usize) -> Package {
    let mut source = String::from(
        "--!strict\nlocal Plugin = {}\nlocal handlers: { [string]: (any, any) -> () } = {}\n",
    );
    for n in 0..handlers {
        source.push_str(&format!(
            r#"handlers["big.c{n}"] = function(cx, args)
    local text = string.format("%s-%d", "c{n}", {n})
    local parts = {{}}
    for i = 1, 10 do
        table.insert(parts, string.rep(text, i % 3))
    end
    if #parts > {n} then
        cx.set_clipboard(table.concat(parts, ","))
    end
end
"#
        ));
    }
    source.push_str(
        r#"function Plugin.activate(cx)
    for n = 0, 9 do
        assert(cx.register_command({ name = "big.c" .. n, title = "Command " .. n, category = "view" }))
    end
end

function Plugin.on_command(cx, name, args)
    local handler = handlers[name]
    if handler then
        handler(cx, args)
    end
end

return Plugin
"#,
    );
    Package::new([("src/init.luau".to_owned(), source)])
}

/// The lines of `package`'s sources.
fn lines(package: &Package) -> usize {
    package
        .paths()
        .map(|path| package.source(path).unwrap_or_default().lines().count())
        .sum()
}

/// One activation of the plugin `name` of `package`, run as `execution`
/// says, on a host of its own: how long it took, and its VM's heap.
fn activation(name: &str, package: &Package, execution: Execution) -> (Duration, usize) {
    let mut plugin = LuauPlugin::new(name, package.clone(), execution);
    let mut host = FakeHost::new(name);
    let start = Instant::now();
    host.activate(&mut plugin).unwrap();
    let took = start.elapsed();
    (took, plugin.memory().unwrap())
}

/// The value at `share` of the sorted `values`.
fn at(values: &[Duration], share: f64) -> Duration {
    values[((values.len() - 1) as f64 * share).round() as usize]
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

#[test]
#[ignore = "measures a release build's times: just bench-luau"]
fn a_luau_plugin_activates_within_its_budget_of_time_and_memory() {
    let plugins = [
        ("acme/hello", "the template", template()),
        ("acme/big", "2000 lines", large(200)),
    ];
    let mut executions = vec![Execution::Interpreter];
    if native_supported() {
        executions.insert(0, Execution::Native);
    }
    println!("A plugin's activation, its compilation within, and its VM's heap once active:");
    let mut first = true;
    for (name, label, package) in &plugins {
        for &execution in &executions {
            // The process's first, which makes what every VM shares.
            let (once, _) = activation(name, package, execution);
            if first {
                println!("  the process's first activation: {:.3} ms", ms(once));
                first = false;
            }
            let mut times = Vec::with_capacity(RUNS);
            let mut heap = 0;
            for _ in 0..RUNS {
                let (took, memory) = activation(name, package, execution);
                times.push(took);
                heap = heap.max(memory);
            }
            times.sort();
            println!(
                "  {label} ({} lines), {execution:?}: median {:.3} ms, p90 {:.3} ms, max {:.3} ms of {RUNS}; heap {:.0} KB",
                lines(package),
                ms(at(&times, 0.5)),
                ms(at(&times, 0.9)),
                ms(at(&times, 1.0)),
                heap as f64 / 1024.0,
            );
            assert!(
                at(&times, 0.5) <= MOST_ACTIVATION,
                "{label}, {execution:?}: {times:?}"
            );
            assert!(heap <= MOST_MEMORY, "{label}, {execution:?}: {heap} bytes");
        }
    }
}
