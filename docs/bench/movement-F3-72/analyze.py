"""Tables of the cursor's movement's measures, from a session's reports:
the latency of `[cursor] movement` "instant" and of "snappy", by setting
and by place in a round, and idle and memory.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/bench-F3-72"
A, B = "instant", "snappy"


def load(name):
    path = os.path.join(OUT, name)
    return json.load(open(path)) if os.path.exists(path) else None


def order(round_, a, b):
    return (a, b) if (round_ - 1) % 4 in (0, 3) else (b, a)


print("## Latency (ms), and its histogram in refreshes")
by_setting = {v: {"p50": [], "p99": []} for v in (A, B)}
by_place = {p: {"p50": [], "p99": []} for p in (1, 2)}
tails = {v: [0, 0] for v in (A, B)}
for r in range(1, 9):
    for place, v in enumerate(order(r, A, B), start=1):
        d = load(f"latency-{v}-{r}.json")
        if not d:
            continue
        l = d["latency_ms"]
        print(f"round {r} place {place} {v}: p50 {l['p50']:.2f} p90 {l['p90']:.2f} "
              f"p99 {l['p99']:.2f} max {l['max']:.2f}; refresh {d['refresh_ms']}; {d['histogram']}")
        for key in ("p50", "p99"):
            by_setting[v][key].append(l[key])
            by_place[place][key].append(l[key])
        tails[v][0] += sum(n for k, n in d["histogram"].items() if int(k) >= 4)
        tails[v][1] += sum(d["histogram"].values())
for v, values in by_setting.items():
    if values["p50"]:
        print(f"{v}: p50 mean {st.mean(values['p50']):.2f} (sd {st.pstdev(values['p50']):.2f}), "
              f"p99 mean {st.mean(values['p99']):.2f} (sd {st.pstdev(values['p99']):.2f}), "
              f"of {len(values['p50'])} rounds; {tails[v][0]} of {tails[v][1]} samples in the fourth refresh or later")
for p, values in by_place.items():
    if values["p50"]:
        print(f"place {p}: p50 mean {st.mean(values['p50']):.2f}, p99 mean {st.mean(values['p99']):.2f}")

print("## Idle (per s) and memory (MB)")
for r in (1, 2):
    for v in order(r, A, B):
        i = load(f"idle-{v}-{r}.json")
        m = load(f"memory-{v}-{r}.json")
        if not i or not m:
            continue
        s = i["summary"]
        ms = m["summary"]
        print(f"round {r} {v}: package idle {s['package_idle_wakeups_per_s']:.3f}/s, "
              f"interrupt {s['interrupt_wakeups_per_s']:.3f}/s; footprint w/o GPU "
              f"{ms['footprint_without_gpu']['median'] / 1e6:.1f} MB, GPU {ms['gpu']['median'] / 1e6:.1f} MB")
