"""Tests of the measuring tools, which `just test-bench-tools` runs:

    python3 -m unittest discover -s tools/bench
"""
import datetime
import os
import subprocess
import tempfile
import unittest

import compare
import powermetrics_extract


class Executables(unittest.TestCase):
    def test_every_process_that_ps_lists_has_its_executable(self):
        child = subprocess.Popen(["/bin/sleep", "30"])
        try:
            found = compare.executables()
            listed = subprocess.run(["ps", "-A", "-o", "pid="], capture_output=True, text=True,
                                    check=True).stdout.split()
        finally:
            child.kill()
            child.wait()
        self.assertEqual(found.get(child.pid), "/bin/sleep")
        # But a process that ended between the two lists, and the few whose
        # path the kernel keeps to itself, as its own, pid 0.
        missing = {int(pid) for pid in listed} - found.keys()
        self.assertLess(len(missing), len(listed) / 20, sorted(missing))


class Restore(unittest.TestCase):
    def test_a_setting_goes_back_once_its_app_quit_and_keeps_its_copy_until(self):
        with tempfile.TemporaryDirectory() as directory:
            saved = os.path.join(directory, "com.apple.Terminal.plist")
            put = []
            back = lambda path: put.append(path) or True
            # The app still runs: nothing goes back, the copy stays.
            open(saved, "w").close()
            self.assertFalse(compare.restore_one(saved, lambda: False, back))
            self.assertEqual(put, [])
            self.assertTrue(os.path.exists(saved))
            # It could not be put back: the copy stays.
            self.assertFalse(compare.restore_one(saved, lambda: True, lambda path: False))
            self.assertTrue(os.path.exists(saved))
            # The app quit, and the setting went back: the copy goes.
            self.assertTrue(compare.restore_one(saved, lambda: True, back))
            self.assertEqual(put, [saved])
            self.assertFalse(os.path.exists(saved))
            # Nothing saved: nothing to do.
            self.assertTrue(compare.restore_one(saved, lambda: False, back))


HEADER = "*** Sampled system activity ({}) ({}ms elapsed) ***\n"


class Samples(unittest.TestCase):
    def test_a_sample_is_stamped_at_its_end(self):
        # Three samples of 1.3 s from 10:00:01.3: they end at 02.6, 03.9 and
        # 05.2, which their headers tell to the second.
        text = "".join(HEADER.format(f"Sun Nov  1 10:00:{second:02} 2026 +0100", "1300.00")
                       for second in (2, 3, 5))
        with tempfile.TemporaryDirectory() as directory:
            path = os.path.join(directory, "powermetrics.txt")
            with open(path, "w") as file:
                file.write(text)
            samples = powermetrics_extract.samples_of(path)
        zone = datetime.timezone(datetime.timedelta(hours=1))
        first = datetime.datetime(2026, 11, 1, 10, 0, 1, 300_000, zone).timestamp()
        # The headers bound the first start from 01.1 to 01.4.
        self.assertLess(abs(samples[0]["start"] - first), 0.2)
        self.assertAlmostEqual(samples[2]["start"] - samples[0]["start"], 2.6, places=3)


if __name__ == "__main__":
    unittest.main()
