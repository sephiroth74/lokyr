"""Tables of the A/B of 0.9.0, from a session's reports: each measure of
0.8.0's app and of 0.9.0's, the latency's means by bundle and by place in
a round; and the footprint of `snap` and `awake` against no plugin.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F4-42"


def load(name):
    path = os.path.join(OUT, name)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def order(round_, a, b):
    return (a, b) if (round_ - 1) % 4 in (0, 3) else (b, a)


def startup(rounds, a, b, floor):
    for r in rounds:
        fl = load(f"startup-floor-{r}.json") if floor else None
        fm = fl["summary"]["from_start_ms"]["median"] if fl else None
        for v in order(r, a, b):
            d = load(f"startup-{v}-{r}.json")
            if not d:
                continue
            s = d["summary"]
            m = s["from_start_ms"]["median"]
            gap = f"; floor {fm:.1f}; gap {m - fm:.1f}" if fm else ""
            print(f"round {r} {v}: {m:.1f} ms, p90 {s['from_start_ms']['p90']:.1f}, "
                  f"spread {s['spread'] * 100:.1f}%{gap}")


def latency(rounds, a, b):
    by_bundle = {v: {"p50": [], "p99": []} for v in (a, b)}
    by_place = {p: {"p50": [], "p99": []} for p in (1, 2)}
    tails = {v: [0, 0] for v in (a, b)}
    for r in rounds:
        for place, v in enumerate(order(r, a, b), start=1):
            d = load(f"latency-{v}-{r}.json")
            if not d:
                continue
            l = d["latency_ms"]
            print(f"round {r} place {place} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} "
                  f"p99 {l['p99']:.2f} max {l['max']:.2f}; refresh {d['refresh_ms']}; {d['histogram']}")
            for key in ("p50", "p99"):
                by_bundle[v][key].append(l[key])
                by_place[place][key].append(l[key])
            tails[v][0] += sum(n for k, n in d["histogram"].items() if int(k) >= 4)
            tails[v][1] += sum(d["histogram"].values())
    for v, values in by_bundle.items():
        if values["p50"]:
            print(f"{v}: p50 mean {st.mean(values['p50']):.2f} (sd {st.pstdev(values['p50']):.2f}), "
                  f"p99 mean {st.mean(values['p99']):.2f} (sd {st.pstdev(values['p99']):.2f}), "
                  f"of {len(values['p50'])} rounds; {tails[v][0]} of {tails[v][1]} samples in the fourth refresh or later")
    for p, values in by_place.items():
        if values["p50"]:
            print(f"place {p}: p50 mean {st.mean(values['p50']):.2f}, p99 mean {st.mean(values['p99']):.2f}")


print("## Startup, warm, first frame (ms): median, spread; floor's first window; gap")
startup((1, 2, 3, 4), "v080", "v090", True)
print("## Latency (ms), and its histogram in refreshes")
latency(range(1, 9), "v080", "v090")

print("## Idle (per s) and memory (MB)")


def rest(name):
    i = load(f"idle-{name}.json")
    m = load(f"memory-{name}.json")
    if not i or not m:
        return None
    s = i["summary"]
    ms = m["summary"]
    path = os.path.join(OUT, f"threads-{name}.txt")
    threads = open(path).read().strip() if os.path.exists(path) else "?"
    print(f"{name}: package idle {s['package_idle_wakeups_per_s']:.3f}/s, "
          f"interrupt {s['interrupt_wakeups_per_s']:.3f}/s; footprint w/o GPU "
          f"{ms['footprint_without_gpu']['median'] / 1e6:.1f} MB, GPU {ms['gpu']['median'] / 1e6:.1f} MB; "
          f"{threads} threads")
    return ms["footprint_without_gpu"]["median"]


for r in (1, 2):
    for v in order(r, "v080", "v090"):
        rest(f"{v}-{r}")

print("## Throughput on screen (MiB/s, median of the runs of both rounds) and frames (ms)")
for f in ("ascii", "seq", "sgr", "cjk"):
    for v in ("v080", "v090"):
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

print("## snap and awake installed against no plugin, 0.9.0")
for r in (1, 2):
    sizes = {}
    for home in order(r, "none", "two"):
        sizes[home] = rest(f"plugins-{home}-{r}")
    if sizes.get("none") and sizes.get("two"):
        print(f"round {r}: {(sizes['two'] - sizes['none']) / 1e6:.2f} MB of both")
