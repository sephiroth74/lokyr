"""Tables of F4-58's phases: for each signature, the means of its 9 rounds'
latency p50 and p99, and of each phase's p50 and p99: the press to the
app's handling of its key, that to the frame of its echo, and that to its
presentation.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/phases-F4-58"
print("| Signature | Latency p50 | p99 | Handled p50 | p99 | Framed p50 | p99 | Presented p50 | p99 | Fourth refresh or later |")
print("|---|---|---|---|---|---|---|---|---|---|")
for build in ("jit", "nojit", "unsigned"):
    runs = []
    for r in range(1, 10):
        path = os.path.join(OUT, f"latency-{build}-{r}.json")
        if os.path.exists(path) and os.path.getsize(path) > 0:
            runs.append(json.load(open(path)))
    if not runs:
        continue
    mean = lambda f: st.mean(f(run) for run in runs)
    late = sum(n for run in runs for k, n in run["histogram"].items() if int(k) >= 4)
    total = sum(sum(run["histogram"].values()) for run in runs)
    cells = [mean(lambda r: r["latency_ms"]["p50"]), mean(lambda r: r["latency_ms"]["p99"])]
    for phase in ("handled", "framed", "presented"):
        cells += [mean(lambda r: r["phases_ms"][phase]["p50"]), mean(lambda r: r["phases_ms"][phase]["p99"])]
    print(f"| {build} | " + " | ".join(f"{c:.2f}" for c in cells) + f" | {late} of {total} |")
