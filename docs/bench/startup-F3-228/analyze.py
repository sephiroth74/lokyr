"""The start's medians and gaps of each app of the session, by round, and
their means.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/startup-F3-228"
APPS = ("v051", "v070", "head", "delay")
gaps = {app: [] for app in APPS}
for r in range(1, 5):
    floor = json.load(open(os.path.join(OUT, f"startup-floor-{r}.json")))["summary"]["from_start_ms"]["median"]
    for app in APPS:
        s = json.load(open(os.path.join(OUT, f"startup-{app}-{r}.json")))["summary"]["from_start_ms"]
        gaps[app].append((s["median"], s["median"] - floor))
        print(f"round {r} {app}: {s['median']:.1f} ms, p90 {s['p90']:.1f}; floor {floor:.1f}; gap {s['median'] - floor:.1f}")
for app, values in gaps.items():
    print(f"{app}: median's mean {st.mean(m for m, _ in values):.1f} ms, gap's mean {st.mean(g for _, g in values):.1f} ms")
