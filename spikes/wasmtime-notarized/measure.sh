#!/usr/bin/env bash
# Measures the signed bundles from a terminal, where the process keeps its
# normal priority, and prints the medians as a Markdown table. Each case (both
# variants on both guests) runs `runs` times compiled from scratch, then
# `runs` times from the cache.
#
#   ./bundle.sh pulley && ./bundle.sh cranelift entitlements/unsigned-memory.plist
#   ./measure.sh [runs]
set -euo pipefail

cd "$(dirname "$0")"
runs="${1:-10}"
out="build/measure"
cache="${TMPDIR:-/tmp}/lokyr-spike"
rm -rf "$out"
mkdir -p "$out"

for variant in pulley cranelift; do
    case "$variant" in
        pulley) app="build/Spike-Pulley.app" flags=() ;;
        cranelift) app="build/Spike-Cranelift.app" flags=(--from-memory) ;;
    esac
    for guest in unknown wasip2; do
        for run in $(seq "$runs"); do
            rm -rf "$cache"
            "$app/Contents/MacOS/spike" --bench "$app/Contents/Resources/$guest.wasm" \
                ${flags[@]+"${flags[@]}"} > "$out/$variant-$guest-compiled-$run.json"
            "$app/Contents/MacOS/spike" --bench "$app/Contents/Resources/$guest.wasm" --cached \
                ${flags[@]+"${flags[@]}"} > "$out/$variant-$guest-cached-$run.json"
        done
    done
done

python3 -I - "$out" "$runs" <<'EOF'
import json, pathlib, statistics, sys

out, runs = pathlib.Path(sys.argv[1]), sys.argv[2]
cases = {}
for path in sorted(out.glob("*.json")):
    variant, guest, mode, _ = path.stem.split("-")
    cases.setdefault((variant, guest, mode), []).append(json.loads(path.read_text()))

def median(rows, key):
    return statistics.median(key(row) for row in rows)

print(f"Medians of {runs} runs per case.\n")
print("| Variant | Guest | Mode | Compile ms | Load ms | Instantiate ms | `noop` ns | `render(100)` µs | `work` vs. native | Footprint growth MB |")
print("|---|---|---|---|---|---|---|---|---|---|")
for (variant, guest, mode), rows in sorted(cases.items()):
    compile_ms = "—" if mode == "cached" else f"{median(rows, lambda r: r['compile_ms']):.2f}"
    print(
        f"| {variant} | {guest} | {mode} | {compile_ms}"
        f" | {median(rows, lambda r: r['deserialize_ms']):.2f}"
        f" | {median(rows, lambda r: r['instantiate_ms']):.3f}"
        f" | {median(rows, lambda r: r['noop_median_ns']):.0f}"
        f" | {median(rows, lambda r: r['render_100_median_us']):.1f}"
        f" | {median(rows, lambda r: r['work_slowdown']):.2f}"
        f" | {median(rows, lambda r: (r['footprint_end_kb'] - r['footprint_start_kb']) / 1024):.2f} |"
    )
EOF
