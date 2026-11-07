"""The releases' benchmarks at a glance: from `docs/bench/releases.json`,
the summary at the top of `docs/bench/RELEASES.md`, the last release
against the one before as its own session measured it, and the charts of
each group of measures over the releases, in `docs/bench/releases/`.

    python3 tools/bench/releases.py            # writes them
    python3 tools/bench/releases.py --check    # fails where they differ
"""
import json
import os
import sys

import charts

BENCH = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "docs", "bench")
DATA = os.path.join(BENCH, "releases.json")
PAGE = os.path.join(BENCH, "RELEASES.md")
START = "<!-- The summary and the charts, which tools/bench/releases.py writes from releases.json: edit those, not this. -->"
END = "<!-- The end of what tools/bench/releases.py writes. -->"


def value_text(value, measure):
    unit = measure.get("unit", "")
    text = charts.number(value, measure.get("digits", 1))
    return f"{text} {unit}".strip() if unit and not unit.startswith("/") else f"{text}{unit}"


def verdict(measure, before, after):
    """The change from `before` to `after` of `measure`, its share of its
    budget, and its verdict: over its budget, worse, better, or as before,
    a change within its tolerance being none."""
    lower = measure["better"] == "lower"
    change = after - before
    if "tolerance_pct" in measure:
        tolerance = abs(before) * measure["tolerance_pct"] / 100
    else:
        tolerance = measure.get("tolerance", 0)
    budget = measure.get("budget")
    over = budget is not None and (after > budget if lower else after < budget)
    worse = (change > tolerance) if lower else (change < -tolerance)
    better = (change < -tolerance) if lower else (change > tolerance)
    if over:
        return "❌ Over its budget"
    if worse:
        return "⚠️ Worse, within its budget" if budget is not None else "⚠️ Worse"
    if better:
        return "✅ Better"
    return "✅ As before"


def change_text(measure, before, after):
    change = after - before
    digits = measure.get("digits", 1)
    sign = "+" if change > 0 else "−" if change < 0 else "±"
    amount = charts.number(abs(change), digits)
    unit = measure.get("unit", "")
    amount = f"{amount} {unit}".strip() if unit and not unit.startswith("/") else f"{amount}{unit}"
    if before:
        return f"{sign}{amount}, {sign}{abs(change) / abs(before) * 100:.0f} %"
    return f"{sign}{amount}"


def budget_text(measure, after):
    budget = measure.get("budget")
    if budget is None:
        return "—"
    sign = "≤" if measure["better"] == "lower" else "≥"
    unit = measure.get("unit", "")
    text = f"{sign} {budget:g}" + (unit if unit.startswith("/") else f" {unit}")
    if budget and measure["better"] == "lower":
        text += f", {after / budget * 100:.0f} % of it"
    return text


def summary(data):
    """The Markdown of the summary and of the charts' images."""
    releases = data["releases"]
    last = next(r for r in reversed(releases) if not r.get("build"))
    against = next(r for r in releases if r["id"] == last["against"])
    lines = [
        "## At a glance",
        "",
        f"{last['title']} against {against['title']}, both measured in {last['title']}'s session, "
        f"[{last['report'][:-3]}]({last['report']}): each measure's change, the share of its budget, "
        "and whether it is as before, better, or worse beyond its rounds' spread.",
        "",
        f"| Measure | {against['title']} | {last['title']} | Change | Budget | |",
        "|---|---|---|---|---|---|",
    ]
    for measure in data["measures"]:
        after = measure["values"].get(last["id"])
        before = measure.get("baseline", {}).get(last["id"])
        if after is None or before is None:
            continue
        lines.append(f"| {measure['name']} | {value_text(before, measure)} | {value_text(after, measure)} | "
                     f"{change_text(measure, before, after)} | {budget_text(measure, after)} | "
                     f"{verdict(measure, before, after)} |")
    lines += ["", "The latency's budgets, 2 refreshes at the p50 and 4.5 at the p99, are of a display at 120 Hz, "
              "which these sessions at 60 Hz do not measure.",
              "", "## Over the releases", "",
              "Each measure over the releases, a dot of each release and a ring of a build between "
              "two, its budget a dashed line, of the columns of [the table](#every-measure): different "
              "sessions, which compare within a few percent alone.", ""]
    for chart in data["charts"]:
        subtitle = chart["subtitle"][0].lower() + chart["subtitle"][1:]
        lines.append(f"![{chart['title']}: {subtitle}]({chart['file']})")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def chart_files(data):
    """Each chart's path and its SVG."""
    releases = data["releases"]
    columns = [(r["label"], bool(r.get("build"))) for r in releases]
    measures = {m["id"]: m for m in data["measures"]}
    files = {}
    for chart in data["charts"]:
        # From the first release that a measure of the chart has, as the
        # window's motion was first measured of 0.5.0.
        names = [name for panel in chart["panels"] for name in panel["series"]]
        first = min(i for i, r in enumerate(releases)
                    if any(r["id"] in measures[name]["values"] for name in names))
        shown = releases[first:]
        panels = []
        for panel in chart["panels"]:
            series = [measures[name] for name in panel["series"]]
            panels.append({
                "title": panel["title"],
                "unit": series[0].get("unit", ""),
                "digits": series[0].get("digits", 1),
                "budget": series[0].get("budget") or None,
                "names": chart.get("legend", [None, None]),
                "series": [[m["values"].get(r["id"]) for r in shown] for m in series],
            })
        files[chart["file"]] = charts.trends(chart["title"], chart["subtitle"], columns[first:], panels,
                                             chart.get("legend")) + "\n"
    return files


def page(text, block):
    """`text`, RELEASES.md, with `block` between its markers."""
    start, end = text.index(START) + len(START), text.index(END)
    return text[:start] + "\n\n" + block + "\n" + text[end:]


def main(argv):
    check = "--check" in argv
    with open(DATA) as file:
        data = json.load(file)
    outputs = {os.path.join(BENCH, path): svg for path, svg in chart_files(data).items()}
    with open(PAGE) as file:
        outputs[PAGE] = page(file.read(), summary(data))
    stale = []
    for path, content in outputs.items():
        current = open(path).read() if os.path.exists(path) else None
        if current == content:
            continue
        stale.append(os.path.relpath(path, os.getcwd()))
        if not check:
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w") as file:
                file.write(content)
    for path in stale:
        print(f"{'stale' if check else 'wrote'}: {path}")
    return 1 if check and stale else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
