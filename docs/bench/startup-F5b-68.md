# 1.6.0's startup of its notarized DMG

Measured on 28 November 2026, from 13:48 to 14:03, from a terminal with VS Code closed. As for the releases before, both copies here are of notarized disk images, because a copy that is not notarized starts later than the same code notarized. The 1.6.0 copy is the release's candidate, of the same code as the release. The session's script, [`run.sh`](startup-F5b-68/run.sh), its analysis, [`analyze.py`](startup-F5b-68/analyze.py), the reports and [the tables](startup-F5b-68/analysis.txt) are in [startup-F5b-68](startup-F5b-68/).

## Method

The copies were 1.5.0's app of its release's DMG, build 202611242301, and 1.6.0's app of its candidate's DMG, build 202611281135, both notarized, their tickets stapled, both accepted by Gatekeeper. Both used the sessions' [base config](../../tools/bench/sessions/configs/base/lokyr/config.toml): Menlo at 13 points, 120 × 40 cells, smooth scrolling and Blocks off.

- **ttybench**, [its session's report](startup-F5b-68/ttybench-2026-11-28-1348/report.md): each copy and the floor app launched 30 times first, then four cycles of eight rounds of each copy, in the orders AB BA BA AB, the same pause of 2 s before each run. Each round launches Lokyr and ttybench's floor app, the least AppKit app that shows a window, both by `open`, and times both windows on screen. The gap is Lokyr's window less the floor app's, the releases' startup gap from 1.6.0 on.
- **lokyr-bench**: eight rounds of each copy's first frame on the display, ten warm starts after two, and of the minimal AppKit app's window, in turns.

The Mac was an Apple M5 Pro on AC power, up for 3 days and 5 hours. The display was a 4K one of 28 inches at 60 Hz and a scale of 2, and the load average was 1.8 to 6.5.

## Results

| Measure | 1.5.0 | 1.6.0 | Change, its 95 % interval | Budget |
|---|---|---|---|---|
| ttybench, the launch to the window, the median of 32 | 335.9 ms | 327.7 ms | −8.2 ms, −13.2 … −2.4 | |
| ttybench, its window after the floor app's | 80.9 ms | 76.6 ms | −4.4 ms, −14.6 … +4.6 | ≤ 70 ms ✘ both |
| The floor app's window | 255.4 ms | 251.8 ms | −3.6 ms, −14.6 … +8.5 | |
| lokyr-bench, the first frame, the mean of eight rounds' medians | 247.0 ms | 237.7 ms | −9.3 ms, sd 6.6 | |
| lokyr-bench, its gap to the minimal app, the mean of the rounds | 22.1 ms | 12.8 ms | | |
| The minimal app | 224.9 ms, from 221.0 to 232.2 | | | |

- **1.6.0 starts quicker than 1.5.0**: its window comes 8.2 ms sooner in ttybench, beyond the noise, and its first frame 9.3 ms sooner in lokyr-bench, sooner in 7 rounds of 8. 1.6's first window no longer waits for the menu bar, the global keys and the updates' checks. The libraries of the Shortcuts actions, which 1.6.0 loads at each launch, cost less than that.
- **The startup's gap is past its budget for both copies**, 80.9 and 76.6 ms of 70. 1.5.0's notarized app had 70.4 ms in [the startup beside a floor app](startup-TB-23.md#results), on 25 November; in this session the same app's gap was 10.5 ms longer, of the Mac's state, and 1.6.0's 4.4 ms shorter than it, within the noise.
- **lokyr-bench's gap is within its old budget of 35 ms for both**, 22.1 and 12.8 ms, as the minimal app's launch held from 221 to 232 ms through the session.
- **Shortcuts indexed 1.6.0's actions** in one of its runs: the system's `BackgroundShortcutRunner` and `siriactionsd` took half a core each, which a new copy of the app brings once.
