"""Tables of startup-TB-23's session: ttybench's window, floor and gap of
the copy, of its last session's report, and lokyr-bench's first frame
and its gap to the minimal app's window, the mean of its rounds.

    python3 analyze.py [<the session's directory>]
"""
import glob, json, os, statistics as st, sys

OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(os.path.abspath(__file__)), "../../../target/bench/startup-TB-23")
COPIES = ("v150",)
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
    print("| Metric | Median | 95 % interval of the median | Mean ± sd | Samples |")
    print("|---|---|---|---|---|")
    for metric in METRICS:
        for group in report["groups"]:
            if group["tool"] != "startup" or group["metric"] != metric:
                continue
            for row in group["rows"]:
                if row["terminal"] not in COPIES:
                    continue
                m = row["summary"]
                low, high = m["median_interval"]
                print(f"| `{metric}` | {m['median']:.1f} | {low:.1f} … {high:.1f} | "
                      f"{m['mean']:.1f} ± {m['stddev']:.1f} | {m['count']} |")
    print()


def lokyr_bench():
    medians = {v: [] for v in COPIES}
    gaps = {v: [] for v in COPIES}
    floors = []
    for round in range(1, 7):
        path = f"{OUT}/lokyr-bench-floor-{round}.json"
        if not os.path.exists(path):
            continue
        floor = json.load(open(path))["summary"]["from_start_ms"]["median"]
        floors.append(floor)
        for v in COPIES:
            path = f"{OUT}/lokyr-bench-{v}-{round}.json"
            if not os.path.exists(path):
                continue
            median = json.load(open(path))["summary"]["from_start_ms"]["median"]
            medians[v].append(median)
            gaps[v].append(median - floor)
    print("lokyr-bench, the mean of its rounds' medians:")
    print(f"- the minimal app's window: {mean_sd(floors)}")
    for v in COPIES:
        print(f"- {v}: first frame {mean_sd(medians[v])}; gap {mean_sd(gaps[v])}")
    print()


ttybench()
lokyr_bench()
