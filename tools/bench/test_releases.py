"""Tests of the releases' summary and charts: `python3 -m unittest
discover -s tools/bench`, as `just test-bench-tools` runs them."""
import unittest

import charts
import releases

LOWER = {"better": "lower", "budget": 35, "tolerance": 5, "digits": 1, "unit": "ms"}
HIGHER = {"better": "higher", "tolerance_pct": 4, "digits": 1, "unit": "MiB/s"}


class Verdict(unittest.TestCase):
    def test_a_change_within_its_tolerance_is_none(self):
        self.assertEqual(releases.verdict(LOWER, 20, 24), "✅ As before")
        self.assertEqual(releases.verdict(HIGHER, 330, 318.6), "✅ As before")

    def test_a_change_past_its_tolerance_is_better_or_worse_by_the_measure_s_direction(self):
        self.assertEqual(releases.verdict(LOWER, 20, 13), "✅ Better")
        self.assertEqual(releases.verdict(LOWER, 13, 26), "⚠️ Worse, within its budget")
        self.assertEqual(releases.verdict(HIGHER, 300, 280), "⚠️ Worse")
        self.assertEqual(releases.verdict(HIGHER, 280, 300), "✅ Better")

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
        self.assertEqual(ringed.count('class="surface accent-line"'), 2)

    def test_two_series_take_two_colors_and_a_legend(self):
        columns = [("0.5.0", False), ("0.5.1", False)]
        panel = {"title": "A drag", "unit": "/s", "digits": 1, "budget": None,
                 "series": [[57.3, 58.0], [58.0, 58.0]]}
        svg = charts.trends("Motion", "Frames", columns, [panel], ["opaque", "a blur"])
        self.assertIn('class="second ring"', svg)
        self.assertIn(">a blur</text>", svg)


class Files(unittest.TestCase):
    def test_the_summary_and_the_charts_are_those_of_the_data(self):
        # `python3 tools/bench/releases.py` writes them again.
        self.assertEqual(releases.main(["--check"]), 0)


if __name__ == "__main__":
    unittest.main()
