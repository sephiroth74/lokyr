"""Tables of F4-58's session at rest: the latency of each round and the means
by bundle and by place, and the startup of each round, of 0.7.0's app and
0.8.0's.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/rest-F4-58"


def load(name):
    path = os.path.join(OUT, name)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def order(round_, a, b):
    return (a, b) if (round_ - 1) % 4 in (0, 3) else (b, a)


def startup(rounds, a, b, floor):
    for r in rounds:
        fl = load(f"startup-floor-{r}.json") if floor else None
        fm = fl["summary"]["from_start_ms"]["median"] if fl else None
        for v in order(r, a, b):
            d = load(f"startup-{v}-{r}.json")
            if not d:
                continue
            s = d["summary"]
            m = s["from_start_ms"]["median"]
            gap = f"; floor {fm:.1f}; gap {m - fm:.1f}" if fm else ""
            print(f"round {r} {v}: {m:.1f} ms, p90 {s['from_start_ms']['p90']:.1f}, "
                  f"spread {s['spread'] * 100:.1f}%{gap}")


def latency(rounds, a, b):
    by_bundle = {v: {"p50": [], "p99": []} for v in (a, b)}
    by_place = {p: {"p50": [], "p99": []} for p in (1, 2)}
    tails = {v: [0, 0] for v in (a, b)}
    for r in rounds:
        for place, v in enumerate(order(r, a, b), start=1):
            d = load(f"latency-{v}-{r}.json")
            if not d:
                continue
            l = d["latency_ms"]
            print(f"round {r} place {place} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} "
                  f"p99 {l['p99']:.2f} max {l['max']:.2f}; refresh {d['refresh_ms']}; {d['histogram']}")
            for key in ("p50", "p99"):
                by_bundle[v][key].append(l[key])
                by_place[place][key].append(l[key])
            tails[v][0] += sum(n for k, n in d["histogram"].items() if int(k) >= 4)
            tails[v][1] += sum(d["histogram"].values())
    for v, values in by_bundle.items():
        if values["p50"]:
            print(f"{v}: p50 mean {st.mean(values['p50']):.2f} (sd {st.pstdev(values['p50']):.2f}), "
                  f"p99 mean {st.mean(values['p99']):.2f} (sd {st.pstdev(values['p99']):.2f}), "
                  f"of {len(values['p50'])} rounds; {tails[v][0]} of {tails[v][1]} samples in the fourth refresh or later")
    for p, values in by_place.items():
        if values["p50"]:
            print(f"place {p}: p50 mean {st.mean(values['p50']):.2f}, p99 mean {st.mean(values['p99']):.2f}")


print("## Latency (ms), and its histogram in refreshes")
latency(range(1, 17), "v070", "v080")
print("## Startup, warm, first frame (ms): median, spread; floor's first window; gap")
startup((1, 2, 3, 4), "v070", "v080", True)
