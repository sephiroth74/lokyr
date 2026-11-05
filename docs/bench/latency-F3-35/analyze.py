"""Tables of F3-35's session, from its reports and its log."""
import json, os, re, statistics as st
OUT = "/tmp/lokyr/bench-F3-35"
NAMES = {"h": "the app as it ships", "w": "beside a plugin on its thread that renders without pause"}
ORDERS = ["h w", "w h", "w h", "h w", "h w", "w h"]
def load(name):
    return json.load(open(os.path.join(OUT, name)))
cpu = {}
for line in open(os.path.join(OUT, "session.log")):
    m = re.search(r"cpu (\w+) (\d+): ([\d.]+)", line)
    if m:
        cpu[(m.group(1), int(m.group(2)))] = float(m.group(3))
print("## Each round (ms), its histogram in refreshes, and the process's CPU")
for r, order in enumerate(ORDERS, 1):
    for v in order.split():
        d = load(f"latency-{v}-{r}.json")
        l = d["latency_ms"]
        print(f"round {r} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} p99 {l['p99']:.2f} max {l['max']:.2f}; "
              f"{d['histogram']}; CPU {cpu.get((v, r), float('nan')):.0f} %")
print("## Each bundle over the rounds")
for v in ("h", "w"):
    ds = [load(f"latency-{v}-{r}.json") for r in range(1, 7)]
    p50 = [d["latency_ms"]["p50"] for d in ds]
    p99 = [d["latency_ms"]["p99"] for d in ds]
    n = sum(d["samples"] for d in ds)
    hist = {}
    for d in ds:
        for k, c in d["histogram"].items():
            hist[int(k)] = hist.get(int(k), 0) + c
    late = sum(c for k, c in hist.items() if k >= 3)
    later = sum(c for k, c in hist.items() if k >= 5)
    cpus = [cpu[(v, r)] for r in range(1, 7) if (v, r) in cpu]
    print(f"{v} ({NAMES[v]}): p50 mean {st.mean(p50):.2f}, p99 mean {st.mean(p99):.2f} "
          f"(min {min(p99):.1f}, max {max(p99):.1f}); of {n} samples, {late} in the third refresh or later, "
          f"{later} in the fifth or later; CPU mean {st.mean(cpus) if cpus else float('nan'):.0f} %; "
          f"{dict(sorted(hist.items()))}")
print("## By the place in the round")
for place in range(2):
    ds = [load(f"latency-{order.split()[place]}-{r}.json") for r, order in enumerate(ORDERS, 1)]
    print(f"place {place + 1}: p50 mean {st.mean(d['latency_ms']['p50'] for d in ds):.2f}, "
          f"p99 mean {st.mean(d['latency_ms']['p99'] for d in ds):.2f}")
