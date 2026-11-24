"""Tables of the measures of `lokyr-bench` of the A/B of 1.5.0, from
run.sh's reports: the startup and its gap to the minimal app's first
window, the memory at rest without the GPU's, and the throughput on
screen of each workload with each frame's time on the CPU, of 1.4.0's tag
and of 1.5's bundle.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F5b-20"
A, B = "v140", "v150"


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
            for r in (1, 2):
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


print("## Startup, warm, first frame (ms): median, spread; floor's first window; gap")
startup("startup", (1, 2, 3, 4))
print("## Startup again, eight rounds")
startup("startup-again", range(1, 9))
print("## Memory at rest, four rounds")
memory_rounds((1, 2, 3, 4))
print("## Throughput on screen (MiB/s, median of the runs of every round) and frames (ms)")
throughput()
