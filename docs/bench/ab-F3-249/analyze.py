"""Tables of the inline images' cost of no image shown, from a session's
reports: 0.9.0's app against the bundle of the images, idle and memory at
rest, and the throughput on screen with its frames.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F3-249"


def load(name):
    path = os.path.join(OUT, name)
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def order(round_, a, b):
    return (a, b) if (round_ - 1) % 4 in (0, 3) else (b, a)


def rest(name):
    i = load(f"idle-{name}.json")
    m = load(f"memory-{name}.json")
    if not i or not m:
        return
    s = i["summary"]
    ms = m["summary"]
    path = os.path.join(OUT, f"threads-{name}.txt")
    threads = open(path).read().strip() if os.path.exists(path) else "?"
    print(f"{name}: package idle {s['package_idle_wakeups_per_s']:.3f}/s, "
          f"interrupt {s['interrupt_wakeups_per_s']:.3f}/s; footprint w/o GPU "
          f"{ms['footprint_without_gpu']['median'] / 1e6:.1f} MB, GPU {ms['gpu']['median'] / 1e6:.1f} MB; "
          f"{threads} threads")


print("## Idle (per s) and memory (MB)")
for r in (1, 2):
    for v in order(r, "v090", "next"):
        rest(f"{v}-{r}")

print("## Throughput on screen (MiB/s, median of the runs of both rounds) and frames (ms)")
for f in ("ascii", "seq", "sgr", "cjk"):
    for v in ("v090", "next"):
        rates, frames = [], []
        for r in (1, 2):
            path = os.path.join(OUT, f"onscreen-{f}-{v}-{r}.jsonl")
            if os.path.exists(path):
                rates += [json.loads(line)["mib_s"] for line in open(path) if line.strip()]
            fr = load(f"frames-{f}-{v}-{r}.json")
            if fr:
                frames.append(fr)
        if not rates:
            continue
        frame = ", ".join(f"total p50 {fr['total_ms']['p50']:.3f} p99 {fr['total_ms']['p99']:.3f}" for fr in frames)
        print(f"{f} {v}: {st.median(rates):.1f} MiB/s (runs {', '.join(f'{x:.1f}' for x in rates)}); {frame}")
