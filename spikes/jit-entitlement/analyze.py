"""Tables of the measures of each signature: the median over the rounds of
each measure's p50, p99 and p999, in microseconds.

    python3 analyze.py [build/results | build/results-region]
"""
import glob, json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "build/results"
VARIANTS = ["plain", "hr", "hr-jit", "hr-unsigned", "hr-both"]
MEASURES = ["pingpong", "wake-rest", "wake-busy", "faults", "syscall"]
rows = {}
for variant in VARIANTS:
    for path in sorted(glob.glob(os.path.join(OUT, f"{variant}-*.jsonl"))):
        for line in open(path):
            data = json.loads(line)
            if "measure" in data:
                rows.setdefault((variant, data["measure"]), []).append(data)
print("| Measure | " + " | ".join(VARIANTS) + " |")
print("|---|" + "---|" * len(VARIANTS))
for measure in MEASURES:
    for key in ("p50", "p99", "p999"):
        cells = []
        for variant in VARIANTS:
            runs = rows.get((variant, measure), [])
            cells.append(f"{st.median(run[key] for run in runs) / 1000:.2f}" if runs else "—")
        print(f"| {measure} {key}, µs | " + " | ".join(cells) + " |")
