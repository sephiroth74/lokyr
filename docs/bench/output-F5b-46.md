# `cat` in a large window, 1.4.0 against 1.5.0

Measured on 25 November 2026, on the built-in Retina display at a scale of 2. [1.5.0's measures](ab-F5b-20.md#the-measures-of-lokyr-bench) found `cat` of 1 GiB on screen 2.4 % slower in a window of 1920 × 1080 points, three runs of four, where ttybench's window of 120 × 40 cells told both copies alike. This report measures it again in a large window, of both tools, of 1.4.0's and 1.5.0's tags built into bundles signed alike. The sessions are in [output-F5b-46](output-F5b-46/): ttybench's [report](output-F5b-46/ttybench/report.md), of the session [`4-output-large`](../../tools/bench/sessions/ab/4-output-large.toml), and `lokyr-bench`'s runs, of [`run.sh`](output-F5b-46/run.sh).

## Results

| Tool | Window | 1.4.0 | 1.5.0 | Change |
|---|---|---|---|---|
| ttybench, plain text to the quiet, the median of four cycles | 216 × 64 cells of Menlo at 13 points | 294.8 MiB/s | 294.5 MiB/s | −0.1 %, −11.9 … +20.4, within the noise |
| `lokyr-bench`, `--bench output` of ASCII, the median of eight runs | 1720 × 1080 points, 3440 × 2020 pixels | 291.6 MiB/s | 295.9 MiB/s | +3.1 MiB/s the rounds' mean, sd 3.9, 1.5.0 slower in 1 round of 4 |

- **1.5.0 prints as fast as 1.4.0 in a large window**, of either tool. The 2.4 % of 1.5.0's session doesn't come back: of three runs of four, in a session whose load ran from 3 to 7 and whose antivirus scanned at times, it was within that day's noise.
- **A frame takes the same time on the CPU**: 0.31 ms at the median and 0.40 at the p99 for both, of 3440 × 2020 pixels.
- The window is of 1720 × 1080 points rather than 1920 × 1080, as large as the built-in display holds: some nine tenths of the pixels of 1.5.0's session's window at a scale of 2.
