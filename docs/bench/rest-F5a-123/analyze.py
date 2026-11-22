"""Tables of 1.4's startup and throughput at rest, from a session's
reports: each round's startup of 1.3.0's app of its DMG, of 1.3.0's tag of
a bundle and of 1.4.0's bundle, their gaps to the minimal app, the
differences between them by round, and the four workloads' throughput of
1.3.0's app and 1.4.0's.

    python3 analyze.py [<reports' directory>]
"""
import json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lokyr/rest-F5a-123"
COPIES = ("v130", "tag130", "v140")
ROUNDS = range(1, 13)


def load(name):
    path = os.path.join(OUT, name)
    # A run that failed left its report empty.
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        return None
    return json.load(open(path))


def mean_sd(values):
    return f"{st.mean(values):.1f} (sd {st.pstdev(values):.1f})"


def startup():
    medians = {v: {} for v in COPIES}
    gaps = {v: [] for v in COPIES}
    floors = []
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
        if not line:
            continue
        floor = f"floor {fm:.1f}" if fm else "no floor"
        print(f"round {r}: {'; '.join(line)}; {floor}")
    print()
    if floors:
        print(f"floor: median {st.median(floors):.1f}, mean {mean_sd(floors)}, "
              f"from {min(floors):.1f} to {max(floors):.1f}")
    for v in COPIES:
        values = list(medians[v].values())
        if values:
            print(f"{v}: median of rounds {st.median(values):.1f}, mean {mean_sd(values)}; "
                  f"gap median {st.median(gaps[v]):.1f}, mean {mean_sd(gaps[v])}")
    print()
    for a, b in (("v130", "v140"), ("tag130", "v140"), ("v130", "tag130")):
        both = [r for r in ROUNDS if r in medians[a] and r in medians[b]]
        if not both:
            continue
        diffs = [medians[b][r] - medians[a][r] for r in both]
        longer = sum(1 for d in diffs if d > 0)
        print(f"{b} − {a} by round: {', '.join(f'{d:+.1f}' for d in diffs)}; "
              f"mean {st.mean(diffs):+.1f} (sd {st.pstdev(diffs):.1f}), median {st.median(diffs):+.1f}; "
              f"{b} longer in {longer} of {len(diffs)}")


def throughput():
    for f, rounds in (("ascii", 8), ("seq", 4), ("sgr", 4), ("cjk", 4)):
        for v in ("v130", "v140"):
            rates, frames = [], []
            for r in range(1, rounds + 1):
                path = os.path.join(OUT, f"onscreen-{f}-{v}-{r}.jsonl")
                if os.path.exists(path):
                    rates += [json.loads(line)["mib_s"] for line in open(path) if line.strip()]
                fr = load(f"frames-{f}-{v}-{r}.json")
                if fr:
                    frames.append(fr["total_ms"]["p50"])
            if not rates:
                continue
            frame = f"; frame total p50 median {st.median(frames):.3f} ms" if frames else ""
            print(f"{f} {v}: median {st.median(rates):.1f} MiB/s of {len(rates)} runs, "
                  f"from {min(rates):.1f} to {max(rates):.1f}{frame}")


print("== startup, warm, first frame, 10 runs after 2")
startup()
print()
print("== throughput on screen")
throughput()
