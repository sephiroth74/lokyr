# 1.3.0 against 1.2.0

Measured on 18 November 2026, in fourteen steps of ten minutes at most between 10:18 and 15:53, the Mac in use between them: the app of 1.2.0's DMG, notarized and stapled, against 1.3.0's bundle, of commit cc317751, signed and not notarized, whose app 1.3.0 releases. The reports are in [ab-F5a-17](ab-F5a-17/), the window's motion's in [`motion`](ab-F5a-17/motion/), with the session's script, [`run.sh`](ab-F5a-17/run.sh), which runs one step at a time, its analysis, [`analyze.py`](ab-F5a-17/analyze.py), and the tables, [`analysis.txt`](ab-F5a-17/analysis.txt).

## Method

As in [1.2.0's A/B](ab-F3b-29.md#method), of [the releases' config](ab-F3b-20/config.toml), in steps: each step runs its own rounds in balanced orders, AB BA BA AB, with the same pause before each run, so that it compares the two copies in its own conditions.

- **Startup**: warm, 10 runs after 2, to the first frame on the display, each round of the minimal AppKit app of F0 as its floor. Four rounds after each copy started 30 times, then eight rounds, in a step of its own, after 30 more starts each.
- **Latency**: 200 keys, eight rounds, after a warm-up of 20 keys of each.
- **Idle and memory**: idle over 60 s and memory over 10 s of a copy started with `open -n`, 20 s after its start, in four rounds over two steps; then idle over 240 s, two rounds in two steps of their own. The memory at rest, 20 s after a start, over 10 s, in four rounds, with `vmmap`'s summary of each copy.
- **Throughput**: the four workloads of [onscreen-F2-56](onscreen-F2-56.md) on screen, two runs in each of four rounds over two steps.
- **The window's motion**: as [1.2.0's](ab-F3b-29.md#the-windows-motion), of [motion-F3-152](motion-F3-152.md)'s config, Menlo at 20 points, 88 × 27 cells, opaque and translucent at 0.95 with a blur of 10: each window dragged by its titlebar, then resized by its corner, 10 s each along the same path, while Instruments' Animation Hitches records every process. Each step records one look in one round, the mouse left alone, then takes each recording's figures and removes the recording.

The motion's first four steps all recorded the blurred look, as the script's function that wrote the two configs also set the look of the round. Two of those rounds, which no later step wrote over, are kept as the blur's first and third rounds. The opaque look's two rounds and the blur's second were recorded again once the script was fixed. 1.2.0's opaque resize of the first round counted the frames of 1.2 s of its 10 and is left out, so its opaque resize is of the second round alone.

## Machine

- Apple M5 Pro, macOS 27.2.1, on AC power, a Dell U3417W of 3440 × 1440 at 60 Hz and a scale of 1, the one display. 1.2.0's session was of a Dell P2721Q at a scale of 2, which is why the GPU's memory here is half of its 30.6 MB for both versions, and why a live resize misses fewer refreshes: 1.2.0's 20 and 15 here, opaque and of a blur, against 33 and 31 in its own session.
- The load average 1 to 7 through the steps, of no other work but the system's, Jamf's and Microsoft Defender's, with the session's folders excluded from Defender's scans.

## Results

| Measure | 1.2.0 | 1.3.0 | Budget |
|---|---|---|---|
| Startup, warm, first frame, the mean of eight rounds, of copies started 60 times | 240.0 ms | 238.6 ms | |
| The same, the first step's four rounds, of copies started 30 times | 237.8, 256.1, 270.1 and 269.5 ms | 239.3, 270.6, 263.2 and 284.4 ms | |
| Its gap to the minimal app's first window, of the eight rounds | 28.6 ms | 27.2 ms | ≤ 35 ms |
| Latency at 60 Hz, p50, the mean of 8 rounds | 17.26 ms | 17.28 ms | ≤ 2 refreshes, 33.3 ms |
| Latency at 60 Hz, p99, the mean of 8 rounds | 32.36 ms | 31.49 ms | ≤ 4.5 refreshes, 75 ms |
| Its samples in the fourth refresh or later | 0 of 1592 | 0 of 1592 | |
| Idle, package idle wakeups, four rounds of 60 s | 0, 0, 0 and 0/s | 0.017, 0.017, 0 and 0/s | 0/s |
| The same, two rounds of 240 s | 0 and 0/s | 0 and 0/s | 0/s |
| Idle, interrupt wakeups, four rounds of 60 s | 0.38, 0.35, 0.35 and 0.35/s | 0.35, 0.35, 0.38 and 0.35/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest, four rounds | 27.3, 27.4, 27.4 and 27.6 MB | 27.5, 27.4, 27.7 and 31.5 MB | ≤ 35 MB |
| GPU memory | 14.2 MB, 11.5 in two later rounds | 14.2 MB, 11.5 in one later round | |
| `cat` of 1 GiB of ASCII on screen, median of 8 | 298.6 MiB/s | 308.6 MiB/s | ≥ 295 MiB/s |
| `seq 1 10000000` | 220.2 MiB/s | 218.3 MiB/s | |
| Colored text, SGR | 279.0 MiB/s | 285.2 MiB/s | |
| CJK and emoji | 217.2 MiB/s | 213.2 MiB/s | |
| A frame on the CPU, ASCII, p50 and p99, the means of four rounds | 0.281 and 0.429 ms | 0.277 and 0.418 ms | p99 ≤ 2 ms |
| A frame on the CPU, CJK and emoji, p50, the mean of four rounds | 0.732 ms | 0.727 ms | |

- **Every budget holds, and no measure is worse beyond its spread.** The latency's p50 is 1.2.0's, and its p99 0.9 ms shorter, within the rounds' deviation of 2.1 ms, of no sample in the fourth refresh.
- **The startup is 1.5 ms shorter**, of copies started as often, 238.6 against 240.0 ms: the eight rounds' differences run from −8.6 to +6.1 ms, of a deviation of 5.2 ms. The startup's gap to the minimal app is 27.2 against 28.6 ms, within the budget for both, of a minimal app that started in 207 to 218 ms, as in the sessions before 1.1.0's. The first step's four rounds, of a load that rose to 5.4 in their first, found gaps of 47 to 73 ms for both in their last three rounds, and 1.3.0 6.0 ms longer, which the second step's eight rounds take the place of.
- **No package idle wakeup at rest.** In the rounds of 60 s, two of 1.3.0's four woke the package once each, at 10 and at 55 s of the round, and none of 1.2.0's, while both versions' interrupt wakeups were the same in number and spacing, 21 to 23 in 60 s. A package idle wakeup is an interrupt wakeup that finds the whole chip idle, which the rest of the system decides: of two such events between two versions of the same wakeups, a split of two to none comes one time in two. Over 240 s, in two rounds of each, neither version woke the package once, of 0.31 to 0.34 interrupt wakeups a second.
- **The memory at rest is 0.2 MB larger**, the median of four rounds 27.6 against 27.4 MB, within the tolerance of 2 MB. One start of 1.3.0's in four, 31.5 MB, held 31,000 allocations more in the default zone than its others, 89,247 against 58,337, 3.6 MB more of the heap; the idle's four rounds, 27.6 to 27.7 MB, did not repeat it.
- **The throughputs are 1.2.0's within their runs' spread**, `cat` 3 % faster and SGR 2 %. CJK and emoji are 1.8 % slower over every run: 3 % in the first step's two rounds, 211.2 against 217.6 MiB/s, where no run of 1.3.0 passed any of 1.2.0's; 0.8 % in the second step's, 215.4 against 217.1, of runs that overlap; within the tolerance of 5 %, and of a frame's CPU that is the same. `cat` moved with the session's load: 1.2.0 ran at 287.4 MiB/s in the first step's rounds, below the budget, and 1.3.0 at 296.9, then both at 314 and 316 in the second step's.

## The window's motion

The means of the rounds, two of the opaque look and three of the blur, the windows on the Dell at 60 Hz, which Instruments names the built-in display:

| Measure | 1.2.0, opaque | 1.3.0, opaque | 1.2.0, a blur | 1.3.0, a blur |
|---|---|---|---|---|
| A drag, frames a second shown | 58.1 | 58.3 | 58.6 | 58.5 |
| Its refreshes missed of 600 | 12.0 | 7.5 | 6.0 | 9.0 |
| A live resize, frames a second shown | 57.4 | 57.4 | 57.6 | 57.6 |
| Its refreshes missed of 600 | 20.0 | 15.0 | 15.3 | 13.7 |
| A live resize, the window server's CPU and the app's | 36 % and 14 % | 35 % and 15 % | 37 % and 15 % | 38 % and 15 % |
| A live resize, the compositor's delay at the 95th percentile | 13.2 ms | 21.0 ms | 18.3 ms | 18.4 ms |

- **The window's motion is 1.2.0's**: as many frames a second of a drag and of a resize, within 0.2, the refreshes missed from 5 to 20 a run for both, and the same CPU.
- **The compositor's delay at the 95th percentile** is again of two values, 13 ms in some runs and 28, a refresh more, in others, for both versions: of 1.3.0's opaque resizes, the first round's 28.8 ms and the second's 13.3, against 1.2.0's 13.2 of the second round alone; of the blur's three rounds, the first's 28 ms for both versions and the others' 13. The median delay is 12 to 13 ms for both.

## The binary's size

| | 1.2.0 | 1.3.0 |
|---|---|---|
| The bundle | 43.4 MB | 44.3 MB |
| The app's binary | 18.3 MB | 19.0 MB |
| Sleip's binary | 16.5 MB | 16.7 MB |

The app grows by 0.7 MB, of the programs' status and its notifications, the click and the selection in the command line, the tabs used last, pinned, grouped and reopened, the hints, the quick terminal of a double-tapped modifier, Use Transparency, the settings' hints editor, and the text fields' keys and context menu.
