# The startup's gap, beside a floor app

Measured on 25 November 2026, from 14:01 to 14:10, from a terminal with VS Code closed. The releases' startup gap compared Lokyr's first frame, which Lokyr tells itself, with a minimal app's window, which a poll finds. [An earlier session](startup-F5b-45.md#why-the-gap-moves) found that gap moving by 23 ms within one session, of no change to the copies, as the two figures are not taken alike. [ttybench](https://github.com/sephiroth74/ttybench) now launches a floor app beside each round: the least app of AppKit that shows a window, launched as the terminal is and timed alike. This session measures 1.5.0 with both tools, one after the other, so that the releases' gap moves to ttybench's figure. Its script, [`run.sh`](startup-TB-23/run.sh), its analysis, [`analyze.py`](startup-TB-23/analyze.py), the results and [the tables](startup-TB-23/analysis.txt) are in [startup-TB-23](startup-TB-23/).

## Method

The copy was 1.5.0's app of its release's DMG, notarized and stapled, the copy of the earlier session, started many times before. It used the sessions' [base config](../../tools/bench/sessions/configs/base/lokyr/config.toml): Menlo at 13 points, 120 × 40 cells, smooth scrolling and Blocks off.

- **ttybench**, [its session's report](startup-TB-23/ttybench-2026-11-25-1401/report.md): 32 rounds in two runs of 16, after the copy and the floor app were each launched 10 times more. Each round launches Lokyr by `open` of a script that runs ttybench's payload, and the floor app by `open` alike. The floor goes first in odd rounds and second in even ones, with the same pause of 2 s before each launch. Both windows are looked for every 5 ms in the window server's list of windows on screen. Each round's gap is Lokyr's window less the floor's.
- **lokyr-bench**, as the releases' gap was measured until 1.5.0: six rounds, each of ten warm runs after two. Lokyr's executable is started directly and timed to its first frame shown on the display; the minimal AppKit app is launched by `open` and timed to its window, in the orders AB BA BA AB AB BA.

The Mac was an Apple M5 Pro on AC power, the display a Dell U3417W at 60 Hz. The load average was 1.9 at the start and 3.1 to 5.3 while lokyr-bench ran, so the Mac was not at rest; both tools ran in the same conditions.

## Results

| Tool | Lokyr | The floor | The gap |
|---|---|---|---|
| ttybench, the median of 32 rounds | 330.6 ms | 259.8 ms | **70.4 ms**, 67.1 to 75.4 |
| lokyr-bench, the mean of six rounds' medians | 243.3 ± 12.3 ms | 198.5 ± 23.3 ms | 44.8 ± 15.9 ms |

The interval of ttybench's gap is the 95 % interval of its median.

- **ttybench's gap held along the session**: 74.5, 71.6, 67.1 and 70.0 ms in each quarter of its rounds. The order of the launches did not move it: 70.2 ms with the floor first, 71.1 ms with it second.
- **lokyr-bench's gap moved from 70.6 to 24.2 ms between its rounds.** The minimal app's window went from 175 to 228 ms as the load rose, while Lokyr's first frame moved by 30 ms, in whole refreshes.
- **The two gaps don't compare.** ttybench times both apps from the same launch through Launch Services to the same event, the window on screen, so what slows every launch falls on both. lokyr-bench times Lokyr from its process's start, without `open`, to its first frame on the display. That is why Lokyr's figure is 87 ms longer in ttybench.

## The releases' budget

From 1.6.0 on, the releases' startup gap is ttybench's `window_gap_ms`, the median of the A/B's first step, [`ab/1-start`](../../tools/bench/sessions/ab/1-start.toml), which now launches the floor beside each round. Its budget is 70 ms. 1.5.0's 70.4 ms here is 0.4 ms past it, a figure for later releases to bring down. The gap to the minimal app's first window stays in [the releases' table](RELEASES.md#every-measure) up to 1.5.0, as its history.
