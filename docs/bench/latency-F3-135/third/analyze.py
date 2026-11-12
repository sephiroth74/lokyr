"""Tables of the third session of the latency's tail, from its reports."""
import json, os, statistics as st
OUT = "/tmp/lokyr/bench-F3-135c"
NAMES = {"o": "0.3.0", "a": "0.4.0", "m2": "the slots in the windows", "m3": "the live resize's pace and the placement", "h": "the head", "t": "the head but for the placement"}
ORDERS = ["o a t m2 h m3", "a m2 o m3 t h", "m2 m3 a h o t", "m3 h m2 t a o", "h t m3 o m2 a", "t o h a m3 m2"]
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
for v in ("o", "a", "m2", "m3", "h", "t"):
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
for place in range(6):
    ds = [load(f"latency-{order.split()[place]}-{r}.json") for r, order in enumerate(ORDERS, 1)]
    p50 = [d["latency_ms"]["p50"] for d in ds]
    p99 = [d["latency_ms"]["p99"] for d in ds]
    print(f"place {place + 1}: p50 mean {st.mean(p50):.2f}, p99 mean {st.mean(p99):.2f}")
print("## The p99's difference from 0.3.0's in each round")
for r in range(1, 7):
    o = load(f"latency-o-{r}.json")["latency_ms"]["p99"]
    print(f"round {r}: " + ", ".join(
        f"{v} {load(f'latency-{v}-{r}.json')['latency_ms']['p99'] - o:+.1f}"
        for v in ("a", "m2", "m3", "h", "t")))
