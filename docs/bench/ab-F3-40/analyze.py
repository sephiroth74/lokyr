"""Tables of the A/B of F3-40, from a session's reports, of the first
session's directory or of the one given, as the second's: each measure of
each bundle, and the latency's means by bundle and by place in a round, as
the place moves the tail more than a bundle does.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F3-40"
BUNDLES = ("v040", "v050")


def load(name):
    path = os.path.join(OUT, name)
    return json.load(open(path)) if os.path.exists(path) else None


def order(round_):
    return ("v040", "v050") if (round_ - 1) % 4 in (0, 3) else ("v050", "v040")


print("## Startup, warm, first frame (ms): median, spread; floor's first window; gap")
for r in (1, 2, 3, 4):
    floor = load(f"startup-floor-{r}.json")
    fm = floor["summary"]["from_start_ms"]["median"] if floor else None
    for v in order(r):
        d = load(f"startup-{v}-{r}.json")
        if not d:
            continue
        s = d["summary"]
        m = s["from_start_ms"]["median"]
        print(f"round {r} {v}: {m:.1f} ms, p90 {s['from_start_ms']['p90']:.1f}, "
              f"spread {s['spread'] * 100:.1f}%; floor {fm:.1f}; gap {m - fm:.1f}")

print("## Latency (ms), and its histogram in refreshes")
by_bundle = {v: {"p50": [], "p99": []} for v in BUNDLES}
by_place = {p: {"p50": [], "p99": []} for p in (1, 2)}
for r in range(1, 9):
    for place, v in enumerate(order(r), start=1):
        d = load(f"latency-{v}-{r}.json")
        if not d:
            continue
        l = d["latency_ms"]
        print(f"round {r} place {place} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} "
              f"p99 {l['p99']:.2f} max {l['max']:.2f}; refresh {d['refresh_ms']}; {d['histogram']}")
        for key in ("p50", "p99"):
            by_bundle[v][key].append(l[key])
            by_place[place][key].append(l[key])
for v, values in by_bundle.items():
    if values["p50"]:
        print(f"{v}: p50 mean {st.mean(values['p50']):.2f} (sd {st.pstdev(values['p50']):.2f}), "
              f"p99 mean {st.mean(values['p99']):.2f} (sd {st.pstdev(values['p99']):.2f}), "
              f"of {len(values['p50'])} rounds")
for p, values in by_place.items():
    if values["p50"]:
        print(f"place {p}: p50 mean {st.mean(values['p50']):.2f}, p99 mean {st.mean(values['p99']):.2f}")
for v in BUNDLES:
    tail = total = 0
    for r in range(1, 9):
        d = load(f"latency-{v}-{r}.json")
        if d:
            total += sum(d["histogram"].values())
            tail += sum(n for k, n in d["histogram"].items() if int(k) >= 4)
    if total:
        print(f"{v}: {tail} of {total} samples in the fourth refresh or later")

print("## Idle (per s) and memory (MB)")
for r in (1, 2):
    for v in order(r):
        i = load(f"idle-{v}-{r}.json")
        m = load(f"memory-{v}-{r}.json")
        if not i or not m:
            continue
        s = i["summary"]
        ms = m["summary"]
        print(f"round {r} {v}: package idle {s['package_idle_wakeups_per_s']:.3f}/s, "
              f"interrupt {s['interrupt_wakeups_per_s']:.3f}/s; footprint w/o GPU "
              f"{ms['footprint_without_gpu']['median'] / 1e6:.1f} MB, GPU {ms['gpu']['median'] / 1e6:.1f} MB")

print("## Throughput on screen (MiB/s, median of the runs of both rounds) and frames (ms)")
for f in ("ascii", "seq", "sgr", "cjk"):
    for v in BUNDLES:
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
