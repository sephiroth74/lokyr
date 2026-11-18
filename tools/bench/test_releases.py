"""Tests of the releases' summary and charts: `python3 -m unittest
discover -s tools/bench`, as `just test-bench-tools` runs them."""
import unittest

import charts
import releases

LOWER = {"better": "lower", "budget": 35, "tolerance": 5, "digits": 1, "unit": "ms"}
HIGHER = {"better": "higher", "tolerance_pct": 4, "digits": 1, "unit": "MiB/s"}


class Verdict(unittest.TestCase):
    def test_a_change_within_its_tolerance_is_none(self):
        self.assertEqual(releases.verdict(LOWER, 20, 24), "✔ As before")
        self.assertEqual(releases.verdict(HIGHER, 330, 318.6), "✔ As before")

    def test_a_change_past_its_tolerance_is_better_or_worse_by_the_measure_s_direction(self):
        self.assertEqual(releases.verdict(LOWER, 20, 13), "✔ Better")
        self.assertEqual(releases.verdict(LOWER, 13, 26), "⚠️ Worse, within its budget")
        self.assertEqual(releases.verdict(HIGHER, 300, 280), "⚠️ Worse")
        self.assertEqual(releases.verdict(HIGHER, 280, 300), "✔ Better")

    def test_a_value_past_its_budget_is_over_it_whatever_its_change(self):
        self.assertEqual(releases.verdict(LOWER, 36, 36), "❌ Over its budget")

    def test_the_change_and_the_budget_read_in_the_measure_s_unit(self):
        self.assertEqual(releases.change_text(LOWER, 13.4, 26.6), "+13.2 ms, +99 %")
        self.assertEqual(releases.change_text(HIGHER, 330.0, 318.6), "−11.4 MiB/s, −3 %")
        self.assertEqual(releases.budget_text(LOWER, 26.6), "≤ 35 ms, 76 % of it")
        self.assertEqual(releases.budget_text(HIGHER, 300), "—")


class Trends(unittest.TestCase):
    def test_a_panel_draws_a_dot_of_each_value_a_ring_of_a_build_and_its_budget(self):
        columns = [("0.1.0", False), ("F3-49", True), ("0.5.1", False)]
        panel = {"title": "Startup", "unit": "ms", "digits": 1, "budget": 35,
                 "series": [[9.4, None, 26.6]]}
        svg = charts.trends("Starting", "The gap", columns, [panel])
        # Two dots of values, the release without one left out, and the
        # legend's ring of a build.
        self.assertEqual(svg.count("<circle"), 3)
        self.assertIn("stroke-dasharray", svg)
        self.assertIn("budget 35", svg)
        self.assertIn(">26.6</text>", svg)
        build = [(label, True) for label, _ in columns]
        ringed = charts.trends("Starting", "The gap", build, [panel])
        self.assertEqual(ringed.count('class="surface accent-line s0"'), 2)

    def test_two_series_take_two_colors_and_a_legend(self):
        columns = [("0.5.0", False), ("0.5.1", False)]
        panel = {"title": "A drag", "unit": "/s", "digits": 1, "budget": None,
                 "series": [[57.3, 58.0], [58.0, 58.0]]}
        svg = charts.trends("Motion", "Frames", columns, [panel], ["opaque", "a blur"])
        self.assertIn('class="second ring s1"', svg)
        self.assertIn(">a blur</text>", svg)

    def test_a_chart_carries_its_panels_releases_and_values_for_the_site(self):
        columns = [("1.2.0", False, "ab-F3b-29.md"), ("1.3.0", False, "ab-F5a-17.md")]
        panel = {"title": "Latency p50, ms", "unit": "ms", "digits": 1, "budget": 33.3,
                 "better": "lower", "names": ["opaque", "a blur"], "series": [[16.6, 17.3], [16.9, None]]}
        svg = charts.trends("Starting", "The gap", columns, [panel], ["opaque", "a blur"])
        self.assertIn('class="chart" data-chart="trends"', svg)
        self.assertIn('<g class="panel" data-title="Latency p50, ms" data-unit="ms" data-digits="1" '
                      'data-budget="33.3" data-better="lower" data-names="opaque|a blur">', svg)
        # A column of each release, of its report, under its dots.
        self.assertIn('data-release="1.3.0" data-index="1" data-report="ab-F5a-17.md"/>', svg)
        self.assertLess(svg.index('class="column"'), svg.index("<circle class=\"accent ring"))
        self.assertIn('data-release="1.3.0" data-series="0" data-value="17.3">', svg)
        self.assertEqual(svg.count('data-series="1" data-value='), 1)
        # Each series' marks of its class, which a legend's item turns off.
        self.assertIn('<g class="legend-item" data-series="1">', svg)
        self.assertIn(".chart.hide-1 .s1", svg)
        # The dark colors as the reader asks for them, or as the page names them.
        self.assertIn("@media (prefers-color-scheme: dark)", svg)
        self.assertIn(".chart.dark .surface { fill: #1a1a19; }", svg)


class Table(unittest.TestCase):
    DATA = {
        "releases": [{"id": "1.2.0", "label": "1.2.0", "report": "ab-F3b-29.md"},
                     {"id": "1.3.0", "label": "1.3.0", "report": "ab-F5a-17.md"}],
        "measures": [
            {"id": "p50", "better": "lower", "tolerance": 1, "budget": 33.3,
             "values": {"1.2.0": 16.6, "1.3.0": 19.0}, "baseline": {"1.2.0": 17.7, "1.3.0": 16.6}},
        ],
        "table": {"groups": [{"title": "Latency", "rows": [
            {"name": "A key to its echo, p50, ms", "budget": ["≤ 33.3", "2 refreshes at 60 Hz"], "measure": "p50",
             "limit": {"value": 33.3, "better": "lower"},
             "cells": {"1.2.0": ["16.6", "at 60 Hz"], "1.3.0": "19.0"}},
            {"name": "The gap, ms", "limit": {"value": 35, "better": "lower"},
             "cells": {"1.2.0": ["58.5", "at 60 Hz"], "1.3.0": "11 of 1592"}},
            {"name": "A drag, frames a second", "cells": {"1.3.0": "58.3"}},
        ]}]},
    }

    def test_the_table_of_every_measure_is_the_data_s_of_its_marks_and_notes(self):
        text = releases.table(self.DATA)
        # The releases' columns of their reports, right-aligned, a group's row.
        self.assertIn("| Measure | Budget | [1.2.0](ab-F3b-29.md) | [1.3.0](ab-F5a-17.md) |\n|---|---|---:|---:|", text)
        self.assertIn("| **Latency** | | | |", text)
        # Better and worse of the measure's verdicts against the release
        # before, past the limit of the value shown, none of no measure.
        self.assertIn("| A key to its echo, p50, ms | ≤\u00a033.3\u00a0¹ | 16.6\u00a0▲\u00a0² | 19.0\u00a0▼ |", text)
        self.assertIn("| The gap, ms |  | 58.5\u00a0✖\u00a0² | 11\u00a0of\u00a01592 |", text)
        # A cell of no value a dash.
        self.assertIn("| A drag, frames a second |  | — | 58.3 |", text)
        # The notes numbered in the cells' order, one of several rows of a
        # release told once, of the release alone.
        self.assertIn("1. The budget · A key to its echo, p50, ms: 2 refreshes at 60 Hz.", text)
        self.assertIn("2. 1.2.0: at 60 Hz.", text)
        self.assertNotIn("3. ", text)


class Files(unittest.TestCase):
    def test_the_summary_and_the_charts_are_those_of_the_data(self):
        # `python3 tools/bench/releases.py` writes them again.
        self.assertEqual(releases.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
