"""Tables of the window's motion of 0.8.0 against 0.9.0, of each run's
figures, which `tools/bench/motion_extract.py` read of its trace as the run
ended, before the trace went."""
import csv, json, os, statistics
OUT = "/tmp/lokyr/motion-F4-42"
NAMES = ["v080-opaque", "v090-opaque", "v080-blur", "v090-blur"]

FIELDS = ["frames_per_second", "missed", "delay_p50_ms", "delay_p95_ms", "windowserver_cpu", "app_cpu", "farthest"]
rows = []
for name in NAMES:
    for kind in ("drag", "resize"):
        for round_ in (1, 2):
            base = os.path.join(OUT, f"{name}-{kind}-{round_}")
            path = base + ".figures.json"
            if not os.path.exists(path) or os.path.getsize(path) == 0:
                continue
            rows.append({"name": name, "kind": kind, "round": round_, **json.load(open(path))})
with open(os.path.join(OUT, "figures.csv"), "w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
    writer.writeheader()
    writer.writerows(rows)
for kind in ("drag", "resize"):
    print(f"## {kind}: means of the two rounds")
    print("| Configuration | Frames a second | Missed | Delay p50 | p95 | WindowServer CPU | App CPU | Farthest |")
    print("|---|---|---|---|---|---|---|---|")
    for name in NAMES:
        runs = [row for row in rows if row["name"] == name and row["kind"] == kind]
        if not runs:
            continue
        mean = lambda field: statistics.mean(run[field] for run in runs)
        print(f"| {name} | {mean('frames_per_second'):.1f} | {mean('missed'):.1f} | {mean('delay_p50_ms'):.2f} | "
              f"{mean('delay_p95_ms'):.2f} | {mean('windowserver_cpu'):.0f} % | {mean('app_cpu'):.0f} % | {mean('farthest'):.0f} |")
    print()
