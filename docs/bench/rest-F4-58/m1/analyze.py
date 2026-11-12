"""Tables of F4-58's session on the M1: the latency of each of the four
apps over 16 rounds, `allow-jit`'s against none of each version, round by
round, with a sign test; the startup against the minimal app; and the
window's motion.

    python3 analyze.py <results' directory>

The micro-benchmarks' tables come of `spikes/jit-entitlement/analyze.py`
of `micro` and `micro-region`.
"""
import json, math, os, statistics as st, sys

OUT = sys.argv[1]
BUILDS = ["v070", "v070-jit", "v080", "v080-nojit"]
ROUNDS = range(1, 17)
# Each version's copy of `allow-jit` and its copy of none.
PAIRS = [("v070-jit", "v070"), ("v080", "v080-nojit")]


def load(*parts):
    path = os.path.join(OUT, *parts)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def sign_test(wins, n):
    """Two-sided p of `wins` of `n` under even odds."""
    if n == 0:
        return 1.0
    k = max(wins, n - wins)
    tail = sum(math.comb(n, i) for i in range(k, n + 1)) / 2 ** n
    return min(1.0, 2 * tail)


def latency():
    runs = {b: {r: load("latency", f"latency-{b}-{r}.json") for r in ROUNDS} for b in BUILDS}
    refresh = next(d["refresh_ms"] for b in BUILDS for d in runs[b].values() if d)
    print(f"## Latency, of 200 keys a run, the display's refresh {refresh} ms")
    print()
    print("| App | Rounds | p50, mean | p99, mean | p99, median | Fourth refresh or later |")
    print("|---|---|---|---|---|---|")
    for b in BUILDS:
        done = [d for d in runs[b].values() if d]
        if not done:
            continue
        p50 = [d["latency_ms"]["p50"] for d in done]
        p99 = [d["latency_ms"]["p99"] for d in done]
        late = sum(n for d in done for k, n in d["histogram"].items() if int(k) >= 4)
        total = sum(sum(d["histogram"].values()) for d in done)
        print(f"| {b} | {len(done)} | {st.mean(p50):.2f} (sd {st.stdev(p50):.2f}) | "
              f"{st.mean(p99):.2f} (sd {st.stdev(p99):.2f}) | {st.median(p99):.2f} | "
              f"{late} of {total} ({late / total * 100:.1f} %) |")
    print()
    print("### `allow-jit` against none, round by round")
    print()
    print("| Version | Rounds where `allow-jit`'s p99 is longer | Sign test, p | p99's difference, mean | median |")
    print("|---|---|---|---|---|")
    every = []
    for jit, none in PAIRS:
        diffs = [runs[jit][r]["latency_ms"]["p99"] - runs[none][r]["latency_ms"]["p99"]
                 for r in ROUNDS if runs[jit][r] and runs[none][r]]
        every += diffs
        longer = sum(1 for d in diffs if d > 0)
        n = sum(1 for d in diffs if d != 0)
        print(f"| {jit.split('-')[0]} | {longer} of {len(diffs)} | {sign_test(longer, n):.3f} | "
              f"{st.mean(diffs):+.2f} ms | {st.median(diffs):+.2f} ms |")
    longer = sum(1 for d in every if d > 0)
    n = sum(1 for d in every if d != 0)
    print(f"| both | {longer} of {len(every)} | {sign_test(longer, n):.3f} | "
          f"{st.mean(every):+.2f} ms | {st.median(every):+.2f} ms |")
    print()
    for b in BUILDS:
        values = [f"{runs[b][r]['latency_ms']['p99']:.0f}" if runs[b][r] else "—" for r in ROUNDS]
        print(f"{b}: p99 of each round {' '.join(values)}")
    print()


def startup():
    print("## Startup, warm, the first frame's median of 10 (ms)")
    print()
    print("| Round | Floor's first window | 0.7.0 | 0.8.0 | 0.7.0's gap | 0.8.0's gap |")
    print("|---|---|---|---|---|---|")
    gaps = {"v070": [], "v080": []}
    medians = {"v070": [], "v080": []}
    for r in (1, 2, 3, 4):
        floor = load("startup", f"startup-floor-{r}.json")
        apps = {v: load("startup", f"startup-{v}-{r}.json") for v in gaps}
        if not floor or not all(apps.values()):
            continue
        fm = floor["summary"]["from_start_ms"]["median"]
        cells = []
        for v in gaps:
            m = apps[v]["summary"]["from_start_ms"]["median"]
            medians[v].append(m)
            gaps[v].append(m - fm)
            cells.append(m)
        print(f"| {r} | {fm:.1f} | {cells[0]:.1f} | {cells[1]:.1f} | "
              f"{gaps['v070'][-1]:.1f} | {gaps['v080'][-1]:.1f} |")
    if gaps["v070"]:
        print(f"| mean | | {st.mean(medians['v070']):.1f} | {st.mean(medians['v080']):.1f} | "
              f"{st.mean(gaps['v070']):.1f} | {st.mean(gaps['v080']):.1f} |")
    print()


def motion():
    print("## The window's motion")
    print()
    print("| Run | Frames a second | Missed | Compositor's delay p50 / p95 | Window server's CPU | App's CPU |")
    print("|---|---|---|---|---|---|")
    folder = os.path.join(OUT, "motion")
    if not os.path.isdir(folder):
        return
    for name in sorted(os.listdir(folder)):
        if not name.endswith(".figures.json"):
            continue
        d = load("motion", name)
        if not d:
            print(f"| {name[:-13]} | — | | | | |")
            continue
        print(f"| {name[:-13]} | {d['frames_per_second']:.1f} | {d['missed']} | "
              f"{d['delay_p50_ms']:.1f} / {d['delay_p95_ms']:.1f} ms | "
              f"{d['windowserver_cpu']:.1f} % | {d['app_cpu']:.1f} % |")
    print()


latency()
startup()
motion()
