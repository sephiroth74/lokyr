"""Tables of the second session of the latency's tail, from its reports."""
import json, os, statistics as st
OUT = "/tmp/lokyr/bench-F3-135b"
NAMES = {"o": "0.3.0", "a": "0.4.0", "h": "the head since"}
ORDERS = ["o a h", "a h o", "h o a", "h a o", "a o h", "o h a"]
def load(name):
    path = os.path.join(OUT, name)
    return json.load(open(path)) if os.path.exists(path) else None
print("## Each round (ms), and its histogram in refreshes")
for r, order in enumerate(ORDERS, 1):
    for v in order.split():
        d = load(f"latency-{v}-{r}.json")
        l = d["latency_ms"]
        print(f"round {r} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} p99 {l['p99']:.2f} max {l['max']:.2f}; {d['histogram']}")
print("## Each bundle over the rounds")
for v in ("o", "a", "h"):
    ds = [load(f"latency-{v}-{r}.json") for r in range(1, 7)]
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
print("## By the place in the round, of every bundle")
for place in range(3):
    ds = [load(f"latency-{order.split()[place]}-{r}.json") for r, order in enumerate(ORDERS, 1)]
    p50 = [d["latency_ms"]["p50"] for d in ds]
    p99 = [d["latency_ms"]["p99"] for d in ds]
    print(f"place {place + 1}: p50 mean {st.mean(p50):.2f}, p99 mean {st.mean(p99):.2f}")
print("## The p99's difference from 0.3.0's in each round")
for r in range(1, 7):
    o = load(f"latency-o-{r}.json")["latency_ms"]["p99"]
    a = load(f"latency-a-{r}.json")["latency_ms"]["p99"]
    h = load(f"latency-h-{r}.json")["latency_ms"]["p99"]
    print(f"round {r}: a {a - o:+.1f}, h {h - o:+.1f}")
