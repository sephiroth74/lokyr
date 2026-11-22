# 1.4.0 against 1.3.0

Measured on 21 November 2026, in fifteen steps of ten minutes at most between 17:25 and 19:47, the Mac free, then the two steps of the scroll again between 22:20 and 22:44: the app of 1.3.0's DMG, notarized and stapled, against 1.4.0's bundle, signed and not notarized, of commit 216113d7 and, for the scroll's two steps, of commit 313f5a5a, which brought the scroll's frames at every refresh. The two differ in scrolling by pixels alone, which the other steps keep off. The reports are in [ab-F5a-78](ab-F5a-78/), the window's motion's in [`motion`](ab-F5a-78/motion/) and the scroll's in [`scroll`](ab-F5a-78/scroll/), with the session's script, [`run.sh`](ab-F5a-78/run.sh), which runs one step at a time, its analysis, [`analyze.py`](ab-F5a-78/analyze.py), and the tables, [`analysis.txt`](ab-F5a-78/analysis.txt).

## Method

As in [1.3.0's A/B](ab-F5a-17.md#method), of [the releases' config](ab-F3b-20/config.toml), in steps: each step runs its own rounds in balanced orders, AB BA BA AB, with the same pause before each run, so that it compares the two copies in its own conditions. Smooth scrolling and Blocks are off in every step, their defaults, as the config turns neither on, but for the scroll's run by pixels.

- **Startup**: warm, 10 runs after 2, to the first frame on the display, each round of the minimal AppKit app of F0 as its floor. Four rounds after each copy started 30 times, then eight rounds, in a step of its own, after 30 more starts each.
- **Latency**: 200 keys, eight rounds, after a warm-up of 20 keys of each.
- **Idle and memory**: idle over 60 s and memory over 10 s of a copy started with `open -n`, 20 s after its start, in four rounds over two steps; then idle over 240 s, two rounds in two steps of their own. The memory at rest, 20 s after a start, over 10 s, in four rounds, with `vmmap`'s summary of each copy.
- **Throughput**: the four workloads of [onscreen-F2-56](onscreen-F2-56.md) on screen, two runs in each of four rounds over two steps.
- **The window's motion**: as [1.3.0's](ab-F5a-17.md#the-windows-motion), of [motion-F3-152](motion-F3-152.md)'s config, Menlo at 20 points, 88 × 27 cells, opaque and translucent at 0.95 with a blur of 10: each window dragged by its titlebar, then resized by its corner, 10 s each along the same path, while Instruments' Animation Hitches records every process. Each step records one look in one round, the mouse left alone, then takes each recording's figures and removes the recording.
- **The scroll**: new, of `lokyr-bench motion --kind scroll`, the trackpad's precise scroll events that macOS sends, posted at the window's middle, 120 a second, of its phases: a swipe up of 600 points over half a second, its momentum over the next half, slowing to a pixel an event, then the same down, for 10 s, while Instruments records; then idle over 30 s of the copy where the scroll left its view. Of a window of Menlo at 20 points, 88 × 27 cells, no scroll bar, its shell printing 50 000 rows first: 1.3.0 by rows, 1.4.0 by rows and 1.4.0 by pixels, `[window] smooth-scroll`, a round each in two steps of reversed orders. Each run's figures count the frames that the app presented, of Metal's command buffers of a present, against the display's refreshes, as a display shows frames only as they change.

Instruments' recording hung as it saved, now and then, the session's first steps of the window's motion waiting an hour on one: the script now ends a recording that has not saved within five minutes and makes it again, once; one of the first step's opaque recordings of 1.3.0 hung twice, and the step was run again.

## Machine

- Apple M5 Pro, macOS 27.2.1, on AC power, a Dell P2721Q at 60 Hz and a scale of 2, the one display, as 1.2.0's session: the GPU's memory is 30.6 MB for both versions, twice 1.3.0's session's of a scale of 1, and a live resize misses as many refreshes as in 1.2.0's session.
- The load average 1.8 to 7.3 through the steps, of no other work but the system's, Jamf's and Microsoft Defender's, with the session's folders excluded from Defender's scans.

## Results

| Measure | 1.3.0 | 1.4.0 | Budget |
|---|---|---|---|
| Startup, warm, first frame, the mean of eight rounds, of copies started 60 times | 233.0 ms | 240.5 ms | |
| The same, the first step's four rounds, of copies started 30 times | 235.7, 244.5, 236.1 and 236.3 ms | 249.2, 235.1, 236.0 and 235.8 ms | |
| Its gap to the minimal app's first window, of the eight rounds | 53.4 ms | 60.9 ms | ≤ 35 ms |
| Latency at 60 Hz, p50, the mean of 8 rounds | 14.50 ms | 13.59 ms | ≤ 2 refreshes, 33.3 ms |
| Latency at 60 Hz, p99, the mean of 8 rounds | 29.76 ms | 27.45 ms | ≤ 4.5 refreshes, 75 ms |
| Its samples in the fourth refresh or later | 0 of 1592 | 0 of 1592 | |
| Idle, package idle wakeups, four rounds of 60 s | 0, 0, 0 and 0/s | 0, 0, 0 and 0/s | 0/s |
| The same, two rounds of 240 s | 0 and 0/s | 0 and 0/s | 0/s |
| Idle, interrupt wakeups, four rounds of 60 s | 0.37, 0.35, 0.35 and 0.32/s | 0.35, 0.35, 0.35 and 0.35/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest, four rounds | 28.6, 29.0, 28.7 and 28.7 MB | 28.7, 28.8, 28.7 and 28.5 MB | ≤ 35 MB |
| GPU memory | 30.6 MB | 30.6 MB | |
| `cat` of 1 GiB of ASCII on screen, median of 8 | 265.6 MiB/s | 269.5 MiB/s | ≥ 275 MiB/s, Ghostty's of [the comparison of 21 and 22 November](compare-F5a-78.md) less 10 %; 294.9 before |
| `seq 1 10000000` | 159.4 MiB/s | 185.9 MiB/s | |
| Colored text, SGR | 195.1 MiB/s | 191.9 MiB/s | |
| CJK and emoji | 203.3 MiB/s | 204.6 MiB/s | |
| A frame on the CPU, ASCII, p50 and p99, the means of four rounds | 0.319 and 0.425 ms | 0.323 and 0.445 ms | p99 ≤ 2 ms |
| A frame on the CPU, CJK and emoji, p50, the mean of four rounds | 0.685 ms | 0.679 ms | |

- **The latency, idle, the memory and the frames hold their budgets, and are 1.3.0's within their spread.** The latency's p50 is 0.9 ms shorter and its p99 2.3 ms, within the rounds' deviations of 0.5 and 2.2 ms, of no sample in the fourth refresh; neither version woke the package once at rest, over four rounds of 60 s nor two of 240 s, of 0.32 to 0.37 interrupt wakeups a second; the memory at rest is 28.7 MB for both, the median of four rounds.
- **The startup's gap to the minimal app and `cat`'s throughput hold for neither version in this session**, which their measures at rest, in a session of their own, take again. The minimal app, the same binary since October, started in 170 to 195 ms, 15 to 40 ms quicker than in the sessions before, while Lokyr's startup is that of 1.3.0's own session, 233.0 ms for 1.3.0 against its 238.6 there: the gap is 53 to 61 ms for both. `cat` ran from 199 to 285 MiB/s, of both versions alike, against 315 for both on this display in 1.2.0's session, and `seq` and SGR some 20 % slower than there too.
- **1.4.0's startup is 0.9 ms longer in the first step's four rounds and 7.5 ms in the second step's eight**, of copies started as often: the eight rounds' differences run from −4.9 to +20.8 ms, of a deviation of 8.6 ms, past the tolerance of 5 ms. The startup at rest takes it again, and its cause, if it holds.
- **The throughputs are 1.3.0's within their runs' spread**, CJK and emoji 0.6 % faster and SGR 1.6 % slower; `seq` 17 % faster, of its second step's two rounds, 222.2 against 191.6 MiB/s, its first's 147.9 against 154.7. The frames on the CPU are 1.3.0's, the ASCII p99 0.02 ms longer, within the rounds' spread.
- **The commands' folds, which the VT core keeps of no command that folds them, cost nothing that counts**: the throughputs and the frames, of the rows that every output pushes into the scrollback, are 1.3.0's.

## The window's motion

The means of the rounds, two of each look, the windows on the Dell at 60 Hz, which Instruments names the built-in display:

| Measure | 1.3.0, opaque | 1.4.0, opaque | 1.3.0, a blur | 1.4.0, a blur |
|---|---|---|---|---|
| A drag, frames a second shown | 57.5 | 58.4 | 57.2 | 58.7 |
| Its refreshes missed of 600 | 18.0 | 11.5 | 22.5 | 9.5 |
| A live resize, frames a second shown | 56.4 | 55.5 | 56.2 | 56.8 |
| Its refreshes missed of 600 | 26.0 | 36.0 | 21.0 | 23.5 |
| A live resize, the window server's CPU and the app's | 33 % and 17 % | 35 % and 17 % | 32 % and 17 % | 37 % and 16 % |
| A live resize, the compositor's delay at the 95th percentile | 21.0 ms | 13.4 ms | 13.4 ms | 28.4 ms |

- **The window's motion is 1.3.0's**: as many frames a second of a drag and of a resize, within 0.9, and the same CPU. An opaque resize missed 36 refreshes of 600 against 1.3.0's 26, 38 and 34 in 1.4.0's two rounds, 30 and 22 in 1.3.0's, within the tolerance of 10 and as many as 1.2.0's 33 on this display.
- **The compositor's delay at the 95th percentile** is again of two values, 13 ms in some runs and 28, a refresh more, in others: 1.4.0's both blurred resizes 28 ms and 1.3.0's 13, and of the opaque ones 1.3.0's first 28.7 ms and the rest 13. The median delay is 12 to 13 ms for both.
- 1.3.0's blurred resize of the first round counted the frames of 5 s of its 10, of no other change.

## Scrolling by pixels

The synthetic swipes and their momentum at 60 Hz, a round of each in two steps, of 1.4.0's bundle of 313f5a5a:

| Measure | 1.3.0, by rows | 1.4.0, by rows | 1.4.0, by pixels |
|---|---|---|---|
| The app's frames a second | 32.2 | 32.1 | 60.8 |
| The display's refreshes with a frame of the app's | 51 % | 51 % | 99 % |
| The gaps between the app's frames of one refresh, of all | 50 % | 50 % | 100 % |
| The app's CPU | 4.7 % | 4.8 % | 6.9 % |
| The window server's CPU | 13 % | 13 % | 20 % |
| Idle after the scroll, package and interrupt wakeups a second | 0 and 0.33 | 0 and 0.32 | 0 and 0.32 |

- **By pixels, the view moves at every refresh**: 1142 frames of the two rounds, each a refresh after the last, 60.8 a second as the frames are paced a quarter of a millisecond quicker than the refresh, the later of two shown where they meet within one. By rows, a frame comes only as the view crosses a row, 32 a second, of 1.3.0 and 1.4.0 alike.
- **The cost is a frame a refresh while the view moves**: the app's CPU 6.9 % against 4.8 % by rows, and the window server's 20 % against 13 %, of twice the frames. **None at rest**: over 30 s after the scroll, the view a share of a row up, neither copy woke the package, of the same interrupt wakeups as by rows.
- Of 1.4.0's bundle before it, of commit 216113d7, the same swipes by pixels, of a momentum that slowed to none, gave 40 frames a second on the display, as each frame waited for the display link's next tick after a trackpad's event, which macOS paces by the events while they come, its ticks 8 to 33 ms apart, so that a refresh that no event reached drew nothing. A trackpad's scroll by hand, measured of the app's own log of its frames, went from 93 % of the refreshes with a frame of a fast scroll, 83 % of a medium one and 58 % of a slow one, to 100 %, 100 % and 91 %, the rest where the view rests between a slow momentum's events.
- **At 120 Hz the scroll is not measured yet**, as the session had the Dell alone: a step of the built-in display at 120 Hz takes it before the release.

## The binary's size

| | 1.3.0 | 1.4.0 |
|---|---|---|
| The bundle | 44.3 MB | 45.0 MB |
| The app's binary | 19.0 MB | 19.5 MB |
| Sleip's binary | 16.7 MB | 16.9 MB |

The app grows by 0.5 MB, of the commands' blocks, smooth scrolling, the status bar's sides, a plugin that replaces another, the controls of macOS 27's sizes, switches, checkboxes, radio buttons, pop-up buttons, search fields, group boxes, key caps, callouts and tags, and the plugin API's new calls.
