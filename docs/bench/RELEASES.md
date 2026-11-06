# The releases' benchmarks side by side

Every release's measures in one table, and the build under way, each column of the A/B that measured it against the release before, at F2-134's conditions: Menlo at 13 points, 120 × 40 cells, an opaque window, the main display at 60 Hz. Each release's notes tell the features that it brought and what they cost, as its session found. Each release's session adds its column and its notes here, the window's motion among its measures, as the user asked on 6 November 2026.

Measures of different sessions compare within a few percent alone: a session's load and display move them, as each report tells; the A/B of a release against the one before, in one session of balanced orders, is what tells a cost. The window's motion, a drag by the titlebar and a live resize along a sine of 200 points, is measured as [F3-152](motion-F3-152.md) measures it, of its config, Menlo at 20 points, 88 × 27 cells.

| Measure | F2, 0.1.0 | 0.2.0 | 0.3.0 | 0.4.0 | 0.5.0 | After F3-49 | Budget |
|---|---|---|---|---|---|---|---|
| Report | [ab-F3-13](ab-F3-13.md) | [ab-F3-13](ab-F3-13.md) | [ab-F3-83](ab-F3-83.md) | [ab-F3-29](ab-F3-29.md) | [ab-F3-40](ab-F3-40.md) | [ab-F3-49](ab-F3-49.md) | |
| Startup, warm, first frame, median | 236.2, 234.8 ms | 220.1, 234.8 ms | 239.9, 237.1 ms | 230.3, 236.4 ms | 237.7, 236.9, 237.6, 238.1 ms | 236.7, 234.2, 239.8, 233.1 ms | |
| Its gap to the minimal app's first window | 8.6, 10.1 ms | −7.5, 10.1 ms | 13.5, 13.7 ms | 19.0, 22.6 ms | 18.7 ms | 27.2 ms | ≤ 35 ms |
| Latency, p50 | 19.7 ms | 19.3 ms | 20.4 ms | 19.6 ms | 19.9 ms | 21.5 ms | ≤ 2 frames at 120 Hz |
| Latency, p99 | 43.6 ms | 51.2 ms | 48.2 ms | 48.3 ms | 50.6 ms | 35.6 ms | ≤ 4.5 frames at 120 Hz |
| Its samples in the fourth refresh or later | | | | 10 of 1592 | 25 of 3184 | 0 of 1592 | |
| Idle, package idle wakeups | 0/s | 0/s | 0, 0.02/s | 0/s | 0.017, 0/s | 0/s | 0/s |
| Idle, interrupt wakeups | 0.35/s | 0.35/s | 0.35/s | 0.35/s | 0.32, 0.35/s | 0.33, 0.35/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest | 24.7 MB | 24.4 MB | 24.8 MB | 25.1 MB | 25.6 MB | 26.4 MB | ≤ 35 MB |
| GPU memory | 30.0 MB | 30.0 MB | 30.0 MB | 14.1 MB, of a scale of 1 | 30.5 MB | 30.5 MB | |
| `cat` of 1 GiB of ASCII on screen | 321.0 MiB/s | 318.8 MiB/s | 310.1 MiB/s | 309.7 MiB/s | 318.7 MiB/s | 318.4 MiB/s | |
| `seq 1 10000000` | 207.6 MiB/s | 219.2 MiB/s | 216.3 MiB/s | 216.5 MiB/s | 217.2 MiB/s | 219.8 MiB/s | |
| Colored text, SGR | 285.7 MiB/s | 285.5 MiB/s | 277.7 MiB/s | 274.7 MiB/s | 284.5 MiB/s | 287.2 MiB/s | |
| CJK and emoji | 218.1 MiB/s | 220.0 MiB/s | 221.7 MiB/s | 217.1 MiB/s | 215.7 MiB/s | 216.1 MiB/s | |
| A frame on the CPU, ASCII, p50 | 0.260 ms | 0.261 ms | 0.272 ms | 0.255 ms, of a scale of 1 | 0.272 ms | 0.270 ms | p99 ≤ 2 ms |
| A frame on the CPU, CJK and emoji, p50 | | | | | 0.45 ms | 0.71 ms | |
| A drag of the window, frames a second shown, opaque and translucent with a blur | | | | | 57.3, 58.0 | 58.8, 58.0 | |
| Its refreshes missed of 600 | | | | | 15.0, 16.5 | 9.0, 16.0 | |
| A live resize, frames a second shown, opaque and translucent with a blur | | | | | 55.7, 55.6 | 55.3, 56.0 | |
| Its refreshes missed of 600 | | | | | 37.0, 38.5 | 42.5, 34.5 | |
| A live resize, the window server's CPU and the app's | | | | | 33 % and 18 %, 36 % and 18 % | 34 % and 19 %, 36 % and 17 % | |

The window's motion was first measured of 0.5.0 and the build after F3-49, in the [motion session after F3-49](ab-F3-49.md#the-windows-motion), of the means of two rounds; F3-152 measured it of the builds before 0.5.0, against Ghostty's. The latency's p50 and p99 are the means of the session's rounds: F2's and 0.2.0's of the first two rounds of F3-13, 0.3.0's of F3-83's two, 0.4.0's of F3-29's eight, 0.5.0's of F3-40's sixteen, the build after F3-49's of its eight. In the session after F3-49, 0.5.0 measured 19.0 ms at the p50 and 47.8 ms at the p99, 10 samples in the fourth refresh or later, and a startup's gap of 21.9 ms, against which the build after F3-49 is read below.

## The releases' features and their costs

- **F2, 0.1.0**: the macOS app of a single pane, of the headless VT core of F1: the baseline.
- **0.2.0**, F3's architecture: the controller in `app-core`, the kernel's workspace, the plugins' first plumbing. No measure moved beyond its spread.
- **0.3.0**, the distribution: Sparkle's updater, the DMG, the notarization. The startup took a refresh more, the work before the first frame a few milliseconds longer; Sparkle at rest no wakeup and under 0.5 MB.
- **0.4.0**, `ui-core`: the view tree, its layout and paint, the widgets, AccessKit's tree for VoiceOver. The startup took a refresh less; the latency's tail a few samples longer, 10 of 1592 in the fourth refresh against 4; the footprint 0.4 MB larger, of a binary 0.42 MB larger.
- **0.5.0**, the plumbing: the workspace in the HostApi, the slots and the panels in the windows, the plugins on threads of their own, their config and state. The latency's p99 3.6 ms longer, within its budget, a few samples; the startup a refresh more now and then, of the slots in the windows; the footprint 0.4 MB larger, of a binary 0.98 MB larger.
- **After F3-49**, the plugins' block under way for 0.6.0: the keymap's plugin, the tab bar, its drags out to windows and into panes, its marks, its spinner and a single tab's title in the titlebar; the splits' commands, dividers, padding, fade, headers and the keyboard that follows the pointer; the window server's progresses. Against 0.5.0 of the same session: the latency's p50 2.4 ms longer and its tail shorter, of no sample in the fourth refresh; the startup 5 ms longer, its gap 27 ms; the footprint 0.9 MB larger; a frame of CJK and emoji 0.26 ms longer on the CPU, of the same throughput; the window's drag and live resize as 0.5.0's. **The single tab's title** that the tabs plugin draws costs a refresh at the start, 233 ms against 216 without the plugin, and 2.7 ms of the latency's median, and a frame at each title's change, 102 of 100 changes against one ([F3-187](../tasks/F3-macos-mvp.md#f3-187--the-single-tabs-titles-cost-at-the-start-and-in-the-latency) finds where); a program's progress draws no frame of its own, the window server sliding its fills.
