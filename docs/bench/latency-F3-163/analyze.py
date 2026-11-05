"""Tables of F3-163's session: each bundle's latency, by round and over
the rounds, its tail's samples, and the means by place in a round."""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F3-163"
BUNDLES = ("o", "k", "m", "h")
ORDERS = {0: "o k h m", 1: "k m o h", 2: "m h k o", 3: "h o m k"}


def load(name):
    path = os.path.join(OUT, name)
    return json.load(open(path)) if os.path.exists(path) else None


rows = {v: [] for v in BUNDLES}
places = {p: [] for p in range(1, 5)}
for r in range(1, 9):
    for place, v in enumerate(ORDERS[(r - 1) % 4].split(), start=1):
        d = load(f"latency-{v}-{r}.json")
        if not d:
            continue
        rows[v].append(d)
        places[place].append(d["latency_ms"]["p99"])
        l = d["latency_ms"]
        print(f"round {r} place {place} {v}: p50 {l['p50']:.2f} p99 {l['p99']:.2f} "
              f"max {l['max']:.2f}; {d['histogram']}")
print("| Bundle | p50, mean | p99, mean | its sd | its range | third refresh or later | fourth or later |")
for v in BUNDLES:
    if not rows[v]:
        continue
    p50 = [d["latency_ms"]["p50"] for d in rows[v]]
    p99 = [d["latency_ms"]["p99"] for d in rows[v]]
    third = sum(n for d in rows[v] for k, n in d["histogram"].items() if int(k) >= 3)
    fourth = sum(n for d in rows[v] for k, n in d["histogram"].items() if int(k) >= 4)
    total = sum(sum(d["histogram"].values()) for d in rows[v])
    print(f"| {v} | {st.mean(p50):.2f} | {st.mean(p99):.2f} | {st.pstdev(p99):.2f} | "
          f"{min(p99):.1f} to {max(p99):.1f} | {third} of {total} | {fourth} |")
for p, values in places.items():
    if values:
        print(f"place {p}: p99 mean {st.mean(values):.2f}")
