"""Tests of the charts' layout, which `just test-bench-tools` runs."""
import re
import unittest

import charts


class Layout(unittest.TestCase):
    def test_a_long_subtitle_wraps_within_the_chart_and_moves_its_content_down(self):
        short = "Less is better"
        long = ("From the process's start to its first window, median of 10 runs; less is "
                "better; Lokyr of the build after F3-72, the others of F2-134")
        self.assertEqual(charts.subtitle_lines(short), [short])
        lines = charts.subtitle_lines(long)
        self.assertEqual(len(lines), 2)
        self.assertEqual(" ".join(lines), long)
        for line in lines:
            self.assertLessEqual(charts.text_width(line, charts.SUBTITLE_SIZE), charts.WIDTH - 40)
        self.assertEqual(charts.top_of(long), charts.top_of(short) + charts.SUBTITLE_LINE)
        svg = charts.bars("Startup", long, [("Lokyr", 236, True), ("Other", 331, False)], "ms")
        self.assertEqual(len(re.findall(r'<text class="secondary" x="16"', svg)), 2)

    def test_charts_side_by_side_take_one_height_their_rows_spread(self):
        rows = [("A", 1.0, 2.0, True), ("B", 1.5, 3.0, False)]
        svg = charts.ranges("Latency", "Less is better", rows, "ms", height=322)
        self.assertIn('height="322"', svg)
        bars = charts.bars("Memory", "Less is better", [("A", 1, True)], "MB", height=322)
        self.assertIn('height="322"', bars)
        # Rows past the height are refused, not squeezed.
        many = [(str(index), index + 1, False) for index in range(20)]
        with self.assertRaises(ValueError):
            charts.bars("Memory", "Less is better", many, "MB", height=322)


if __name__ == "__main__":
    unittest.main()
