"""Tables of F4-58's bisection: the latency of each of the six builds, the
means of its 12 rounds' p50 and p99 and their deviations, its samples in
the fourth refresh or later, and its frames' p50, of each round too.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bisect-F4-58"
BUILDS = ["v070", "a-F3fixes", "b-F4-06", "c-F4-13", "d-F4-56", "v080"]

print("| Build | p50 | p99 | Fourth refresh or later | Frames p50 |")
print("|---|---|---|---|---|")
for build in BUILDS:
    runs = []
    for r in range(1, 13):
        path = os.path.join(OUT, f"latency-{build}-{r}.json")
        if os.path.exists(path) and os.path.getsize(path) > 0:
            runs.append(json.load(open(path)))
    if not runs:
        continue
    p50 = [run["latency_ms"]["p50"] for run in runs]
    p99 = [run["latency_ms"]["p99"] for run in runs]
    late = sum(n for run in runs for k, n in run["histogram"].items() if int(k) >= 4)
    total = sum(sum(run["histogram"].values()) for run in runs)
    frames = [run["frames"]["p50"] for run in runs]
    sd = lambda values: st.stdev(values) if len(values) > 1 else 0
    print(f"| {build} | {st.mean(p50):.2f} (sd {sd(p50):.2f}) | {st.mean(p99):.2f} (sd {sd(p99):.2f}) | "
          f"{late} of {total} | {st.mean(frames):.2f} |")
print()
for build in BUILDS:
    values = []
    for r in range(1, 13):
        path = os.path.join(OUT, f"latency-{build}-{r}.json")
        if os.path.exists(path) and os.path.getsize(path) > 0:
            run = json.load(open(path))
            values.append(f"{run['latency_ms']['p99']:.0f}")
    print(f"{build}: p99 of each round {' '.join(values)}")
