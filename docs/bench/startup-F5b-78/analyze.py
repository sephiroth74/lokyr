"""Tables of startup-F5b-78's session: ttybench's window, floor and gap of
each copy, of its last session's report, with the A/B's change, and
lokyr-bench's first frame of each copy, the mean of its rounds' medians.

    python3 analyze.py [<the session's directory>]
"""
import glob, json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../target/bench/startup-F5b-78")
COPIES = ("a", "b")
METRICS = ("window_ms", "floor_window_ms", "window_gap_ms", "ready_ms")


def mean_sd(values):
    if not values:
        return "—"
    sd = st.stdev(values) if len(values) > 1 else 0.0
    return f"{st.mean(values):.1f} ± {sd:.1f} (n {len(values)})"


def ttybench():
    folders = sorted(glob.glob(f"{OUT}/ttybench-*"))
    if not folders:
        print("ttybench: no session")
        return
    folder = folders[-1]
    report = json.load(open(f"{folder}/report.json"))
    print(f"ttybench, {os.path.basename(folder)}:")
    print("| Metric | a, median | b, median | b − a, its 95 % interval | |")
    print("|---|---|---|---|---|")
    for metric in METRICS:
        for group in report["groups"]:
            if group["tool"] != "startup" or group["metric"] != metric:
                continue
            rows = {row["terminal"]: row["summary"] for row in group["rows"]}
            change = group.get("change") or {}
            interval = change.get("interval", [float("nan")] * 2)
            print(f"| `{metric}` | {rows['a']['median']:.1f} | {rows['b']['median']:.1f} | "
                  f"{change.get('change', float('nan')):+.1f}, {interval[0]:+.1f} to {interval[1]:+.1f} | "
                  f"{change.get('verdict', '')} |")
    print()


def lokyr_bench():
    medians = {v: [] for v in COPIES}
    print("lokyr-bench, each round's first frame, the median of ten starts:")
    for round in range(1, 5):
        cells = []
        for v in COPIES:
            path = f"{OUT}/lokyr-bench-{v}-{round}.json"
            if not os.path.exists(path):
                continue
            median = json.load(open(path))["summary"]["from_start_ms"]["median"]
            medians[v].append(median)
            cells.append(f"{v} {median:.1f}")
        if cells:
            print(f"- round {round}: " + ", ".join(cells))
    print("lokyr-bench, the mean of the rounds' medians:")
    for v in COPIES:
        print(f"- {v}: {mean_sd(medians[v])}")
    pairs = list(zip(medians["a"], medians["b"]))
    if pairs:
        print(f"- b − a, of each round: {mean_sd([b - a for a, b in pairs])}")
    print()


ttybench()
lokyr_bench()
