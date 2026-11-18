"""Tables of the A/B of 1.3.0, from a session's reports: each measure of
1.2.0's app and of 1.3.0's, the latency's means by bundle and by place in
a round, the startup of its two steps, the memory of its rounds and the
window's motion of each look's rounds.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F5a-17"
A, B = "v120", "v130"


def load(name):
    path = os.path.join(OUT, name)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def order(round_, a, b):
    return (a, b) if (round_ - 1) % 4 in (0, 3) else (b, a)


def mean_sd(values):
    return f"{st.mean(values):.1f} (sd {st.pstdev(values):.1f})"


def startup(prefix, rounds):
    medians = {A: [], B: []}
    gaps = {A: [], B: []}
    for r in rounds:
        fl = load(f"{prefix}-floor-{r}.json")
        fm = fl["summary"]["from_start_ms"]["median"] if fl else None
        for v in order(r, A, B):
            d = load(f"{prefix}-{v}-{r}.json")
            if not d:
                continue
            s = d["summary"]
            m = s["from_start_ms"]["median"]
            medians[v].append(m)
            gap = ""
            if fm:
                gaps[v].append(m - fm)
                gap = f"; floor {fm:.1f}; gap {m - fm:.1f}"
            print(f"round {r} {v}: {m:.1f} ms, p90 {s['from_start_ms']['p90']:.1f}, "
                  f"spread {s['spread'] * 100:.1f}%{gap}")
    for v in (A, B):
        if medians[v]:
            print(f"{v}: median of rounds {st.median(medians[v]):.1f}, mean {mean_sd(medians[v])}; "
                  f"gap median {st.median(gaps[v]):.1f}, mean {mean_sd(gaps[v])}")
    if medians[A] and medians[B]:
        diffs = [b - a for a, b in zip(medians[A], medians[B])]
        print(f"{B} − {A} by round: {', '.join(f'{d:+.1f}' for d in diffs)}; mean {st.mean(diffs):+.1f}")


def latency(rounds):
    by_bundle = {v: {"p50": [], "p99": []} for v in (A, B)}
    by_place = {p: {"p50": [], "p99": []} for p in (1, 2)}
    tails = {v: [0, 0] for v in (A, B)}
    for r in rounds:
        for place, v in enumerate(order(r, A, B), start=1):
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


def memory_rounds(rounds):
    values = {A: [], B: []}
    for r in rounds:
        for v in order(r, A, B):
            m = load(f"vmmap-memory-{v}-{r}.json")
            if not m:
                continue
            f = m["summary"]["footprint_without_gpu"]["median"] / 1e6
            values[v].append(f)
            print(f"round {r} {v}: footprint w/o GPU {f:.1f} MB, GPU {m['summary']['gpu']['median'] / 1e6:.1f} MB")
    for v in (A, B):
        if values[v]:
            print(f"{v}: median {st.median(values[v]):.1f} MB, mean {mean_sd(values[v])}")
    if values[A] and values[B]:
        diffs = [b - a for a, b in zip(values[A], values[B])]
        print(f"{B} − {A} by round: {', '.join(f'{d:+.1f}' for d in diffs)} MB")


def throughput():
    for f in ("ascii", "seq", "sgr", "cjk"):
        for v in (A, B):
            rates, frames = [], []
            by_step = []
            for r in (1, 2, 3, 4):
                path = os.path.join(OUT, f"onscreen-{f}-{v}-{r}.jsonl")
                if os.path.exists(path):
                    runs = [json.loads(line)["mib_s"] for line in open(path) if line.strip()]
                    rates += runs
                    by_step.append(runs)
                fr = load(f"frames-{f}-{v}-{r}.json")
                if fr:
                    frames.append(fr)
            if not rates:
                continue
            frame = ", ".join(f"total p50 {fr['total_ms']['p50']:.3f} p99 {fr['total_ms']['p99']:.3f}" for fr in frames)
            steps = ""
            if len(by_step) > 2:
                first = [x for runs in by_step[:2] for x in runs]
                later = [x for runs in by_step[2:] for x in runs]
                steps = f"; rounds 1-2 {st.median(first):.1f}, rounds 3-4 {st.median(later):.1f}"
            print(f"{f} {v}: {st.median(rates):.1f} MiB/s (runs {', '.join(f'{x:.1f}' for x in rates)}{steps}); {frame}")


def motion():
    folder = os.path.join(OUT, "motion")
    if not os.path.isdir(folder):
        return
    for look in ("opaque", "blur"):
        for kind in ("drag", "resize"):
            for v in (A, B):
                rows = []
                for r in (1, 2, 3):
                    d = load(os.path.join("motion", f"{v}-{look}-{kind}-{r}.figures.json"))
                    if d:
                        rows.append((r, d))
                if not rows:
                    continue
                cells = "; ".join(
                    f"r{r} {d['frames_per_second']}/s missed {d['missed']} delay p50 {d['delay_p50_ms']} "
                    f"p95 {d['delay_p95_ms']} ws {d['windowserver_cpu']}% app {d['app_cpu']}% "
                    f"({d['seconds']} s, {d['display']}, {d['refresh_ms']} ms)"
                    for r, d in rows)
                fps = [d["frames_per_second"] for _, d in rows]
                missed = [d["missed"] for _, d in rows]
                print(f"{look} {kind} {v}: fps mean {st.mean(fps):.1f}, missed mean {st.mean(missed):.1f}; {cells}")


print("## Startup, warm, first frame (ms): median, spread; floor's first window; gap")
startup("startup", (1, 2, 3, 4))
print("## Startup again, eight rounds")
startup("startup-again", range(1, 9))
print("## Latency (ms), and its histogram in refreshes")
latency(range(1, 9))
print("## Idle (per s) and memory (MB)")
for r in (1, 2, 3, 4):
    for v in order(r, A, B):
        rest(f"{v}-{r}")
print("## Idle over 240 s")
for r in (1, 2):
    for v in order(r, A, B):
        i = load(f"idle-long-{v}-{r}.json")
        if i:
            s = i["summary"]
            when = [x["elapsed_ms"] for x in i["intervals"] if x["package_idle_wakeups"]]
            print(f"{v}-{r}: package idle {s['package_idle_wakeups_per_s']:.4f}/s at {when} ms, "
                  f"interrupt {s['interrupt_wakeups_per_s']:.3f}/s")
print("## Memory at rest, four rounds")
memory_rounds((1, 2, 3, 4))
print("## Throughput on screen (MiB/s, median of the runs of every round) and frames (ms)")
throughput()
print("## The window's motion")
motion()
