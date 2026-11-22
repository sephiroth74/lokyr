"""Tables of 1.4.0's startup of its release's DMG against 1.3.0's, both
notarized, from a session's reports: each round's startups and their gaps
to the minimal app, and the differences between them by round.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/startup-F5a-79"
COPIES = ("v130", "v140")
ROUNDS = range(1, 13)


def load(name):
    path = os.path.join(OUT, name)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def mean_sd(values):
    return f"{st.mean(values):.1f} (sd {st.pstdev(values):.1f})"


medians = {v: {} for v in COPIES}
gaps = {v: [] for v in COPIES}
floors = []
print("== startup, warm, first frame, 10 runs after 2, of notarized DMGs")
for r in ROUNDS:
    fl = load(f"startup-floor-{r}.json")
    fm = fl["summary"]["from_start_ms"]["median"] if fl else None
    if fm:
        floors.append(fm)
    line = []
    for v in COPIES:
        d = load(f"startup-{v}-{r}.json")
        if not d:
            continue
        s = d["summary"]
        m = s["from_start_ms"]["median"]
        medians[v][r] = m
        if fm:
            gaps[v].append(m - fm)
        line.append(f"{v} {m:.1f} (p90 {s['from_start_ms']['p90']:.1f}, spread {s['spread'] * 100:.1f}%)")
    if line:
        print(f"round {r}: {'; '.join(line)}; " + (f"floor {fm:.1f}" if fm else "no floor"))
print()
if floors:
    print(f"floor: median {st.median(floors):.1f}, mean {mean_sd(floors)}, from {min(floors):.1f} to {max(floors):.1f}")
for v in COPIES:
    values = list(medians[v].values())
    if values:
        print(f"{v}: median of rounds {st.median(values):.1f}, mean {mean_sd(values)}; "
              f"gap median {st.median(gaps[v]):.1f}, mean {mean_sd(gaps[v])}")
both = [r for r in ROUNDS if r in medians["v130"] and r in medians["v140"]]
if both:
    diffs = [medians["v140"][r] - medians["v130"][r] for r in both]
    print(f"v140 − v130 by round: {', '.join(f'{d:+.1f}' for d in diffs)}; mean {st.mean(diffs):+.1f} "
          f"(sd {st.pstdev(diffs):.1f}), median {st.median(diffs):+.1f}; "
          f"v140 longer in {sum(1 for d in diffs if d > 0)} of {len(diffs)}")
