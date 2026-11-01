#!/usr/bin/env python3
"""The tables of the comparison's report, docs/bench/compare-F2-134.md, of
its raw results, in Markdown:

    python3 tools/bench/compare_tables.py
"""
import glob
import json
import math
import os
import re
import statistics

import report_charts

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "docs", "bench",
                    "compare-F2-134")
TERMINALS = report_charts.TERMINALS
KITTEN_TESTS = ["Only ASCII chars", "Unicode chars", "Unique multi-codepoint Unicode cells",
                "CSI codes with few chars", "Long escape codes", "Images"]
UNICODE = ["unicode_wide_results", "emoji_zwj_results", "emoji_vs16_results",
           "emoji_vs15_results", "narrow_results", "ri_results", "sri_results"]


def load(path):
    with open(os.path.join(ROOT, path)) as file:
        return json.load(file)


def lines(path):
    with open(os.path.join(ROOT, path)) as file:
        return [json.loads(line) for line in file if line.strip()]


def ranks(values):
    """The 10th and the 90th percentiles of `values`, as `lokyr-bench`
    takes them: the 90th's nearest rank, ceil(0.9 n), and its mirror."""
    values = sorted(values)
    high = math.ceil(9 * len(values) / 10)
    return values[len(values) - high], values[high - 1]


def startup():
    print("| Terminal | Median | p90 | Spread | Fastest | Slowest |\n|---|---|---|---|---|---|")
    rows = [(name, f"startup-{key}") for key, name in TERMINALS]
    rows.append(("Lokyr, first frame", "startup-lokyr-first-frame"))
    for name, stem in rows:
        runs = [run["from_start_ms"] for round_ in (1, 2)
                for run in load(f"startup/{stem}-{round_}.json")["runs"]]
        median = statistics.median(runs)
        p10, p90 = ranks(runs)
        print(f"| {name} | {median:.1f} ms | {p90:.1f} | {(p90 - p10) / median:.2f} "
              f"| {min(runs):.1f} | {max(runs):.1f} |")


def idle():
    print("| Terminal | Processes | Package idle wakeups | Interrupt wakeups "
          "| Footprint without the GPU's | The GPU's | Energy |\n|---|---|---|---|---|---|---|")
    for key, name in TERMINALS:
        report = load(f"idle/idle-{key}.json")
        rate = lambda counter: sum(
            sum(interval[counter] for interval in run["intervals"]) / len(run["intervals"])
            for run in report["idle"] if run)
        memory = lambda field: sum(
            statistics.median(sample[field] for sample in process["samples"])
            for process in report["memory"] if process) / 1e6
        footprint = memory("footprint_without_gpu")
        processes = [process["name"] for process in report["memory"] if process]
        if key == "iterm2":
            footprint += load("idle/iterm2-server.json")["footprint_without_gpu"] / 1e6
            processes.append("its session's server, in its footprint alone")
        print(f"| {name} | {', '.join(processes)} | {rate('package_idle_wakeups'):.2f} "
              f"| {rate('interrupt_wakeups'):.2f} | {footprint:.1f} MB "
              f"| {memory('gpu'):.1f} MB | {report['energy']['milliwatts']:.2f} mW |")


def kitten(mode):
    """kitty's benchmark's median rates, of `mode`, `kitten` or
    `kitten-render`; none of iTerm2, which never answers its query."""
    print(f"| Terminal, {mode} | " + " | ".join(KITTEN_TESTS) + " |\n|---"
          + "|---" * len(KITTEN_TESTS) + "|")
    for key, name in TERMINALS:
        if key == "iterm2":
            continue
        rates = {test: [] for test in KITTEN_TESTS}
        for path in glob.glob(os.path.join(ROOT, f"throughput/{key}/{mode}-[0-9].txt")):
            with open(path) as file:
                for line in file:
                    found = re.match(r"\s*(.+?)\s*:\s*([\dm.]+)s\s*@\s*([\d.]+)\s*MB/s", line)
                    if found and found.group(1) in rates:
                        rates[found.group(1)].append(float(found.group(3)))
        cells = [f"{statistics.median(rate):.0f}" if rate else "—" for rate in rates.values()]
        print(f"| {name} | " + " | ".join(cells) + " |")


def vtebench():
    """vtebench's median time of each scenario's samples, in milliseconds,
    those that it took."""
    medians = {}
    for key, _ in TERMINALS:
        with open(os.path.join(ROOT, f"throughput/{key}/vtebench.dat")) as file:
            rows = [line.split() for line in file if line.strip()]
        columns = {scenario: [] for scenario in rows[0]}
        for row in rows[1:]:
            for scenario, value in zip(rows[0], row):
                if value != "_":
                    columns[scenario].append(float(value))
        medians[key] = {scenario: statistics.median(values)
                        for scenario, values in columns.items() if values}
    print("| Scenario | " + " | ".join(name for _, name in TERMINALS) + " |\n|---"
          + "|---" * len(TERMINALS) + "|")
    for scenario in medians["lokyr"]:
        print(f"| `{scenario}` | "
              + " | ".join(f"{medians[key][scenario]:g}" for key, _ in TERMINALS) + " |")


def throughput_and_energy():
    print("| Terminal | cat of 1 GiB, median | Its runs | The CPU's energy a GiB | At "
          "|\n|---|---|---|---|---|")
    for key, name in TERMINALS:
        runs = [run["mib_s"] for run in lines(f"throughput/{key}/cat.jsonl")]
        energy = load(f"energy/energycat-{key}.json")
        rate = lines(f"energy/energycat-{key}.jsonl")[0]["mib_s"]
        print(f"| {name} | {statistics.median(runs):.1f} MiB/s | {min(runs):.1f} to "
              f"{max(runs):.1f} | {energy['joules']:.1f} J | {rate:.1f} MiB/s |")


def unicode():
    summary = load("unicode/ucs-summary.json")
    print("| Terminal | " + " | ".join(UNICODE) + " | Languages, all right | Their clusters "
          "|\n|---" + "|---" * (len(UNICODE) + 2) + "|")
    for key, name in TERMINALS:
        results = summary[key]
        cells = []
        for kind in UNICODE:
            right, total = results.get(kind, (0, 0))
            cells.append(f"{100 * right / total:.1f}%" if total else "—")
        whole, languages = results["languages"]
        right, clusters = results["language_graphemes"]
        print(f"| {name} | " + " | ".join(cells)
              + f" | {whole} of {languages} | {100 * right / clusters:.1f}% of {clusters} |")


def main():
    for title, table in [("Startup", startup), ("At rest", idle),
                         ("kitty's benchmark", lambda: kitten("kitten")),
                         ("kitty's benchmark, rendering", lambda: kitten("kitten-render")),
                         ("vtebench", vtebench), ("cat and energy", throughput_and_energy),
                         ("Unicode", unicode)]:
        print(f"\n## {title}\n")
        table()


if __name__ == "__main__":
    main()
