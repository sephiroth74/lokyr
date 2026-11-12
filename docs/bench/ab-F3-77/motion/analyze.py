"""Tables of F3-152's session, of each run's figures, which
`tools/bench/motion_extract.py` reads of its trace."""
import csv, json, os, statistics, subprocess, sys
OUT = "/tmp/lokyr/motion-F3-77"
REPO = "/Users/taacral1/Documents/git/sephiroth74/rust/lokyr"
NAMES = ["v051-opaque", "v070-opaque", "v051-blur", "v070-blur"]

FIELDS = ["frames_per_second", "missed", "delay_p50_ms", "delay_p95_ms", "windowserver_cpu", "app_cpu", "farthest"]
rows = []
cache = os.path.join(OUT, "figures.json")
known = json.load(open(cache)) if os.path.exists(cache) else {}
for name in NAMES:
    for kind in ("drag", "resize"):
        for round_ in (1, 2):
            base = os.path.join(OUT, f"{name}-{kind}-{round_}")
            if not os.path.exists(base + ".trace"):
                continue
            if base not in known:
                pid = open(base + ".pid").read().strip()
                out = subprocess.run(["python3", "-I", os.path.join(REPO, "tools/bench/motion_extract.py"),
                                      base + ".trace", base + ".json", pid], capture_output=True, text=True, check=True).stdout
                known[base] = json.loads(out)
                json.dump(known, open(cache, "w"))
            rows.append({"name": name, "kind": kind, "round": round_, **known[base]})
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
