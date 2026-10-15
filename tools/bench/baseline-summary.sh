#!/usr/bin/env bash
# Prints the reports that tools/bench/baseline.sh wrote as Markdown tables,
# one row per terminal: startup, memory and idle wakeups.
#
#   tools/bench/baseline-summary.sh <dir>
set -euo pipefail

dir="${1:?usage: baseline-summary.sh <dir>}"

python3 -I - "$dir" <<'EOF'
import json, pathlib, sys

d = pathlib.Path(sys.argv[1])
names = sorted({path.name.rsplit("-", 1)[0] for path in d.glob("*-*.json")})


def load(name, probe):
    path = d / f"{name}-{probe}.json"
    return json.loads(path.read_text()) if path.exists() else None


def mb(size):
    return f"{size / 2**20:.1f} MB"


print("| Terminal | First window, median | p90 | Spread | From the launch request, median |")
print("|---|---|---|---|---|")
for name in names:
    if report := load(name, "startup"):
        s = report["summary"]
        print(
            f"| {name} | {s['from_start_ms']['median']:.0f} ms | {s['from_start_ms']['p90']:.0f} ms"
            f" | {100 * s['spread']:.1f}% | {s['from_request_ms']['median']:.0f} ms |"
        )

print("\n| Terminal | Windows (points) | Footprint | GPU memory | Footprint without GPU memory |")
print("|---|---|---|---|---|")
for name in names:
    if report := load(name, "memory"):
        s = report["summary"]
        windows = ", ".join(f"{w['width']:.0f}×{w['height']:.0f}" for w in report["windows"]) or "none"
        print(
            f"| {name} | {windows} | {mb(s['footprint']['median'])} | {mb(s['gpu']['median'])}"
            f" | {mb(s['footprint_without_gpu']['median'])} |"
        )

print("\n| Terminal | Package idle wakeups | Interrupt wakeups |")
print("|---|---|---|")
for name in names:
    if report := load(name, "idle"):
        s = report["summary"]
        print(
            f"| {name} | {s['package_idle_wakeups_per_s']:.2f}/s | {s['interrupt_wakeups_per_s']:.2f}/s |"
        )
EOF
