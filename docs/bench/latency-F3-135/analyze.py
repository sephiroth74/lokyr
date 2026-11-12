"""Tables of F3-135's latency session, from its reports."""
import json, os, statistics as st
OUT = "/tmp/lokyr/bench-F3-135"
NAMES = {"a": "0.4.0", "b": "0.4.0 without the periodic trees", "o": "0.3.0"}
def load(name):
    path = os.path.join(OUT, name)
    return json.load(open(path)) if os.path.exists(path) else None
print("## Each round (ms), and its histogram in refreshes")
for r in range(1, 7):
    for v in ("a", "b", "o"):
        d = load(f"latency-{v}-{r}.json")
        if not d: continue
        l = d["latency_ms"]
        print(f"round {r} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} p99 {l['p99']:.2f} max {l['max']:.2f}; {d['histogram']}")
print("## Each bundle over the rounds")
for v in ("a", "b", "o"):
    ds = [d for r in range(1, 7) if (d := load(f"latency-{v}-{r}.json"))]
    if not ds: continue
    p50 = [d["latency_ms"]["p50"] for d in ds]
    p99 = [d["latency_ms"]["p99"] for d in ds]
    n = sum(d["samples"] for d in ds)
    hist = {}
    for d in ds:
        for k, c in d["histogram"].items():
            hist[int(k)] = hist.get(int(k), 0) + c
    late = sum(c for k, c in hist.items() if k >= 3)
    later = sum(c for k, c in hist.items() if k >= 4)
    print(f"{v} ({NAMES[v]}): p50 mean {st.mean(p50):.2f}, p99 mean {st.mean(p99):.2f} "
          f"(min {min(p99):.1f}, max {max(p99):.1f}); of {n} samples, {late} in the third refresh or later, "
          f"{later} in the fourth or later; {dict(sorted(hist.items()))}")
