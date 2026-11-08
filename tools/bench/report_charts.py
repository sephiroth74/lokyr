"""The charts of the benches' reports, made again from their raw data:
`python3 tools/bench/report_charts.py [report...]`, all of them without
an argument.
"""
import csv
import json
import os
import statistics
import sys

import charts

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "docs", "bench")


def load(path):
    with open(os.path.join(BENCH, path)) as file:
        return json.load(file)


def save(path, svg):
    path = os.path.join(BENCH, path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as file:
        file.write(svg + "\n")
    print(os.path.relpath(path, os.getcwd()))


def latency():
    """A key to its frame on screen, before and after the keys' frames came
    sooner."""
    cases = [("120 Hz", "120hz", 2), ("120 Hz, flood", "120hz-flood", 3),
             ("60 Hz", "60hz", 2), ("60 Hz, flood", "60hz-flood", 3)]
    before = {key: load(f"latency-F2-54/synthetic-{key}.json") for _, key, _ in cases}
    after = {key: load(f"latency-F2-54/after-F2-144-{key}.json") for _, key, _ in cases}
    labels = ("c121058, before", "7796b52, after")
    for stat, name, budget_frames in (("p50", "median", None), ("p99", "99th percentile", None)):
        rows = [(label, before[key]["latency_ms"][stat], after[key]["latency_ms"][stat])
                for label, key, _ in cases]
        # The budgets in frames of each rate: p50 2 frames and p99 4.5, one more next to
        # a flood.
        budgets = []
        for label, key, median_frames in cases:
            frames = median_frames if stat == "p50" else median_frames + 2.5
            budgets.append(frames * before[key]["refresh_ms"])
        save(f"latency-F2-54/charts/{stat}.svg", charts.before_after(
            f"A key to its frame on screen, the {name}",
            "200 synthetic keys each, milliseconds; fewer is better",
            rows, "ms", labels=labels, digits=1, budgets=budgets))
    frames = range(1, 6)
    histogram = lambda report: [report["histogram"].get(str(n), 0) for n in frames]
    save("latency-F2-54/charts/frames-120hz.svg", charts.paired_columns(
        "Keys by the frames they took to show, at 120 Hz",
        "200 synthetic keys, each frame 8.33 ms",
        [f"{n} frame{'s' if n > 1 else ''}" for n in frames],
        histogram(before["120hz"]), histogram(after["120hz"]), "keys", labels=labels))


TERMINALS = [("lokyr", "Lokyr"), ("ghostty", "Ghostty"), ("alacritty", "Alacritty"),
             ("kitty", "kitty"), ("wezterm", "WezTerm"), ("iterm2", "iTerm2"),
             ("terminal", "Terminal.app")]


def median(values):
    values = sorted(values)
    middle = len(values) // 2
    return values[middle] if len(values) % 2 else (values[middle - 1] + values[middle]) / 2


# The other terminals are measured once of their versions, in F2-134;
# Lokyr again, of the same tool and conditions, in the latest session of
# its own, whose figures take the place of F2-134's.
OTHERS = "compare-F2-134"
LOKYR = "compare-F3-72"


def compare_figures():
    """Each terminal's figures in the comparison, from its raw results:
    Lokyr's of its latest session, the others' of F2-134."""
    figures = {}
    for key, _ in TERMINALS:
        root = LOKYR if key == "lokyr" else OTHERS
        runs = [run["from_start_ms"] for round_ in (1, 2)
                for run in load(f"{root}/startup/startup-{key}-{round_}.json")["runs"]]
        idle = load(f"{root}/idle/idle-{key}.json")
        wakeups = sum(sum(i["interrupt_wakeups"] for i in r["intervals"]) / len(r["intervals"])
                      for r in idle["idle"] if r)
        footprint = sum(median([s["footprint_without_gpu"] for s in m["samples"]])
                        for m in idle["memory"] if m) / 1e6
        if key == "iterm2":
            footprint += load(f"{root}/idle/iterm2-server.json")["footprint_without_gpu"] / 1e6
        with open(os.path.join(BENCH, f"{root}/throughput/{key}/cat.jsonl")) as file:
            cat = median([json.loads(line)["mib_s"] for line in file])
        figures[key] = {
            "startup": median(runs),
            "wakeups": wakeups,
            "footprint": footprint,
            "cat": cat,
            "energy": load(f"{root}/energy/energycat-{key}.json")["joules"],
        }
    return figures


def typometer():
    """Typometer's keys, of each terminal, in milliseconds: `{case:
    {terminal: [ms]}}`, the cases `60`, `60-flood`, `120` and
    `120-flood`."""
    cases = {}
    for name in ["60", "60-flood", "120"]:
        with open(os.path.join(BENCH, f"compare-F2-134/latency/typometer-{name}.csv")) as file:
            rows = list(csv.reader(file))
        for column, title in enumerate(rows[0]):
            # Typometer's own names of its runs, as `typo-kitty-60-flood`.
            _, terminal, *case = title.lower().split("-")
            values = [float(row[column]) for row in rows[1:] if row[column].strip()]
            cases.setdefault("-".join(case), {})[terminal] = values
    return cases


def nearest_rank(values, percent):
    """The `percent`th percentile of `values` by its nearest rank, the
    ceil(percent n / 100)th, in integers."""
    values = sorted(values)
    return values[-(-len(values) * percent // 100) - 1]


def compare():
    """Lokyr against the other terminals of macOS: Lokyr of its latest
    session, the others of F2-134, which the subtitles tell. The README
    shows the charts one under another, of one height, the tallest's."""
    figures = compare_figures()
    rows = lambda measure: [(name, figures[key][measure], key == "lokyr")
                            for key, name in TERMINALS]
    of = "; Lokyr of the build after F3-72, the others of F2-134"
    bars = [
        ("startup", "Startup", "From the process's start to its first window, median of 10 runs; "
         "less is better" + of, rows("startup"), "ms", {}),
        ("memory", "Memory at rest", "Footprint without the GPU's, a window of 120 × 40 cells; "
         "less is better" + of, rows("footprint"), "MB", {"digits": 1}),
        ("wakeups", "Wakeups at rest", "Interrupt wakeups a second, over 60 s with a steady cursor; "
         "less is better" + of, rows("wakeups"), "a second", {"digits": 2}),
        ("throughput", "Throughput", "cat of 1 GiB of ASCII lines, on screen, median of 5 runs; "
         "more is better" + of, rows("cat"), "MiB/s", {"order": "descending"}),
        ("energy", "CPU energy", "Joules that the terminal's CPU spends to show cat of 1 GiB; "
         "less is better" + of, rows("energy"), "J", {"digits": 1}),
    ]
    cases = typometer()
    latency = [
        (rate, f"Latency at {rate} Hz",
         "A key to its echo on screen, in ms, of 200 keys of Typometer's, the window alone; "
         "less is better",
         [(name, statistics.median(cases[rate][key]),
           nearest_rank(cases[rate][key], 99), key == "lokyr")
          for key, name in TERMINALS if key in cases[rate]])
        for rate in ("60", "120")
    ]
    height = max(
        [charts.top_of(subtitle) + charts.BAND * len(data) + 34
         for _, _, subtitle, data, _, _ in bars]
        + [charts.top_of(subtitle) + 22 + charts.BAND * len(data) + 34
           for _, _, subtitle, data in latency])
    for name, title, subtitle, data, unit, options in bars:
        save(f"{LOKYR}/charts/{name}.svg",
             charts.bars(title, subtitle, data, unit, height=height, **options))
    for rate, title, subtitle, data in latency:
        save(f"{OTHERS}/charts/latency-{rate}hz.svg",
             charts.ranges(title, subtitle, data, "ms", height=height))


REPORTS = {"latency": latency, "compare": compare}


def main():
    for name in sys.argv[1:] or REPORTS:
        REPORTS[name]()


if __name__ == "__main__":
    main()
