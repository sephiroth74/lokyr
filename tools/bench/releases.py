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
        f"[{last['report'][:-3]}]({last['report']}), but a measure that names another: each measure's "
        "change, the share of its budget, "
        "the best of the other terminals, of [the comparison of 31 October 2026](compare-F2-134.md) and [the window's motion](motion-F3-152.md), "
        "with Lokyr's own value of that session where its conditions differ, and whether it is as before, "
        "better, or worse beyond its rounds' spread.",
        "",
        f"| Measure | {against['title']} | {last['title']} | Change | Budget | The best of the others | |",
        "|---|---|---|---|---|---|---|",
    ]
    for measure in data["measures"]:
        after = measure["values"].get(last["id"])
        before = measure.get("baseline", {}).get(last["id"])
        if after is None or before is None:
            continue
        reference = reference_of(measure)
        best = reference["label"] if reference else "—"
        name = measure["name"]
        session = measure.get("sessions", {}).get(last["id"])
        if session:
            name += f", of [{session['title']}]({session['report']})'s session"
        lines.append(f"| {name} | {value_text(before, measure)} | {value_text(after, measure)} | "
                     f"{change_text(measure, before, after)} | {budget_text(measure, after)} | {best} | "
                     f"{verdict(measure, before, after)} |")
    lines += ["", "The latency's budgets, 2 refreshes at the p50 and 4.5 at the p99, set of a display at 120 Hz, "
              "are of these sessions' 60 Hz here: 33.3 and 75 ms. The throughput's of `cat` is the faster of "
              "Ghostty and Alacritty less 10 %, of [the comparison of 31 October 2026](compare-F2-134.md): Ghostty's 306 MiB/s.",
              "", "## Over the releases", "",
              "Each measure over the releases, a dot of each release and a ring of a build between "
              "two, its budget a dashed line, and the best of the other terminals a dotted one, of "
              "[the comparison of 31 October 2026](compare-F2-134.md), and of [the window's motion](motion-F3-152.md) for the window's motion, "
              "measured once of their versions then, Lokyr's own value of that session beside it where "
              "its conditions differ from the releases' sessions, the latency's by Typometer, of the "
              "screen; of the columns of [the table](#every-measure): different sessions, which compare "
              "within a few percent alone. A panel without a dashed line has no budget, as `seq`'s, SGR's, "
              "CJK's and the window's motion; without a dotted one, no other terminal was measured of it: "
              "that comparison timed no minimal app beside the others' first windows, which their startup's gap needs, "
              "nor the four workloads but `cat`, nor a frame's CPU.", ""]
    for chart in data["charts"]:
        subtitle = chart["subtitle"][0].lower() + chart["subtitle"][1:]
        lines.append(f"![{chart['title']}: {subtitle}]({chart['file']})")
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def reference_of(measure):
    """The line of the best of the other terminals of `measure`, if it has
    one: its value, and its label, of Lokyr's own value of the same session
    where the releases' sessions measure otherwise."""
    reference = measure.get("reference")
    if not reference:
        return None
    label = f"{reference['name']} {reference['value']:g}"
    if "lokyr" in reference:
        label += f", Lokyr {reference['lokyr']:g} then"
    return {"value": reference["value"], "label": label}


def reference_pair(series):
    """The label of a panel's references of two series, of one terminal:
    its name and both values, as `Ghostty 57 · 57.7`."""
    references = [m.get("reference") for m in series]
    if len(references) < 2 or not all(references):
        return None
    return f"{references[0]['name']} " + " · ".join(f"{r['value']:g}" for r in references)


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
        held = [i for i, r in enumerate(releases)
                if any(r["id"] in measures[name]["values"] for name in names)]
        first, last = min(held), max(held)
        shown = releases[first:last + 1]
        panels = []
        for panel in chart["panels"]:
            series = [measures[name] for name in panel["series"]]
            panels.append({
                "title": panel["title"],
                "unit": series[0].get("unit", ""),
                "digits": series[0].get("digits", 1),
                "budget": series[0].get("budget"),
                "budget_label": series[0].get("budget_label"),
                "names": chart.get("legend", [None, None]),
                "series": [[m["values"].get(r["id"]) for r in shown] for m in series],
                "references": [reference_of(m) for m in series],
                "reference_label": reference_pair(series),
            })
        files[chart["file"]] = charts.trends(chart["title"], chart["subtitle"], columns[first:last + 1],
                                             panels, chart.get("legend")) + "\n"
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
