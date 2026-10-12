#!/usr/bin/env bash
# Builds the host with each Lua runtime, times the workloads of
# lua/workloads.lua `runs` times per configuration, and prints the medians as
# Markdown tables. Run it from a terminal, where the process keeps its normal
# priority.
#
#   ./measure-lua.sh [runs]
set -euo pipefail

cd "$(dirname "$0")"
runs="${1:-5}"
out="build/measure-lua"
mkdir -p build/bin
rm -rf "$out"
mkdir -p "$out"

for runtime in luajit luau luau-jit; do
    cargo build --release --locked -p spike --features "$runtime"
    cp target/release/spike "build/bin/spike-$runtime"
done

# Name, binary, and the flag that turns the JIT off, if any.
configs=(
    "luajit-jit spike-luajit"
    "luajit-interpreter spike-luajit --no-jit"
    "luau spike-luau"
    "luau-jit spike-luau-jit"
)
for config in "${configs[@]}"; do
    read -r name binary flag <<< "$config"
    for run in $(seq "$runs"); do
        "build/bin/$binary" --lua-workloads lua/workloads.lua ${flag:+"$flag"} \
            > "$out/$name-$run.json"
    done
done

python3 -I - "$out" "$runs" <<'EOF'
import json, pathlib, statistics, sys

out, runs = pathlib.Path(sys.argv[1]), sys.argv[2]
cases = {}
for path in sorted(out.glob("*.json")):
    name = path.stem.rsplit("-", 1)[0]
    cases.setdefault(name, []).append(json.loads(path.read_text()))
order = ["luajit-jit", "luajit-interpreter", "luau", "luau-jit"]

def median(rows, key):
    return statistics.median(row[key] for row in rows)

print(f"Medians of {runs} runs per configuration, in µs.\n")
columns = [
    ("Copy the lines in", "copy_lines_us", "—"),
    ("Fuzzy, byte loop", "fuzzy_us", "fuzzy_native_us"),
    ("Fuzzy, pattern", "fuzzy_pattern_us", "fuzzy_native_us"),
    ("Highlight all, patterns", "highlight_all_us", "highlight_all_native_us"),
    ("Highlight all, byte loop", "highlight_bytes_all_us", "highlight_all_native_us"),
    ("Read the spans back, mlua", "read_spans_us", "—"),
    ("Read the spans back, C API", "read_spans_raw_us", "—"),
    ("Highlight 40, patterns", "highlight_visible_us", "highlight_visible_native_us"),
    ("Highlight 40, byte loop", "highlight_bytes_visible_us", "highlight_visible_native_us"),
]
print("| Work | Native | " + " | ".join(order) + " |")
print("|---" * (len(order) + 2) + "|")
everything = [row for rows in cases.values() for row in rows]
for label, key, native in columns:
    native_cell = "—" if native == "—" else f"{median(everything, native):.1f}"
    cells = [f"{median(cases[name], key):.1f}" for name in order]
    print(f"| {label} | {native_cell} | " + " | ".join(cells) + " |")

print("\nMemory, in KB.\n")
print("| Configuration | Heap after loading | Heap peak | Footprint growth after loading | Footprint at the end |")
print("|---|---|---|---|---|")
for name in order:
    rows = cases[name]
    growth = median(rows, "footprint_loaded_kb") - median(rows, "footprint_start_kb")
    print(
        f"| {name} | {median(rows, 'heap_loaded_kb'):.0f} | {median(rows, 'heap_peak_kb'):.0f}"
        f" | {growth:.0f} | {median(rows, 'footprint_end_kb'):.0f} |"
    )
EOF
