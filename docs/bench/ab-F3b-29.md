# 1.2.0 against 1.1.0

Measured on 2026-11-16, unattended, in a session between 08:17 and 08:32, a second one of the startup and the memory between 08:38 and 08:50, and one of the window's motion between 09:00 and 09:39: the app of 1.1.0's DMG, notarized and stapled, against 1.2.0's bundle, of commit 74173b2b, signed and not notarized, whose code 1.2.0 releases. The reports are in [ab-F3b-29](ab-F3b-29/), the first session's at its root and the second's in [`again`](ab-F3b-29/again/), the motion's in [`motion`](ab-F3b-29/motion/), with the sessions' scripts, [`run.sh`](ab-F3b-29/run.sh), [`again.sh`](ab-F3b-29/again.sh) and [`motion/run.sh`](ab-F3b-29/motion/run.sh), the first's analysis, [`analyze.py`](ab-F3b-29/analyze.py), and the tables, [`analysis.txt`](ab-F3b-29/analysis.txt) and [`again/analysis.txt`](ab-F3b-29/again/analysis.txt).

## Method

As in [1.1.0's A/B](ab-F3b-20.md#method), of balanced orders and of [its config](ab-F3b-29/config.toml), the releases' from 1.1.0 on: each copy started 30 times before the measures; the startup, warm, 10 runs after 2, to the first frame on the display, four rounds, AB BA BA AB, each of the minimal AppKit app of F0 as its floor; the latency of 200 keys, eight rounds, after a warm-up of 20 keys of each; idle over 60 s and memory over 10 s of a copy started with `open -n`, 20 s after its start, two rounds; the throughput of the four workloads of [onscreen-F2-56](onscreen-F2-56.md) on screen, two runs in each of two rounds; the same pause of 2 s before each run of the startup and of the latency.

The second session measured again the two measures that the first left in doubt: the startup, of copies started 30 times more each, in eight rounds, and the memory at rest, 20 s after a start, over 10 s, in four rounds, with `vmmap`'s summary of each copy.

The window's motion, as [0.9.0's](ab-F4-42.md#the-windows-motion), of [motion-F3-152](motion-F3-152.md)'s config, Menlo at 20 points, 88 × 27 cells, opaque and translucent at 0.95 with a blur of 10: each window dragged by its titlebar, then resized by its corner, 10 s each along the same path, while Instruments' Animation Hitches records every process, two rounds, the second in the reverse order. Each recording's figures are taken as it ends and the recording removed, which takes a few minutes a recording, Instruments saving and exporting some gigabytes of every process's frames.

## Machine

- Apple M5 Pro, macOS 27.2.1, on AC power, a Dell P2721Q at 60 Hz, a scale of 2, as the releases' sessions before 1.1.0's; 1.1.0's was of the built-in display at 120 Hz.
- The load average 4 to 8 through the first session's startup, 2 to 3 after it and through the second, of no other work but the system's, Jamf's and Microsoft Defender's.

## Results

| Measure | 1.1.0 | 1.2.0 | Budget |
|---|---|---|---|
| Startup, warm, first frame, median of the four rounds | 251.8, 243.0, 239.8 and 237.3 ms | 254.1, 262.8, 237.3 and 252.3 ms | |
| The same, of copies started 60 times, the mean of eight rounds | 248.9 ms | 254.3 ms | |
| Its gap to the minimal app's first window, of the eight rounds | 53.2 ms | 58.5 ms | ≤ 35 ms |
| Latency at 60 Hz, p50, the mean of 8 rounds | 17.68 ms | 16.61 ms | ≤ 2 refreshes, 33.3 ms |
| Latency at 60 Hz, p99, the mean of 8 rounds | 31.15 ms | 31.47 ms | ≤ 4.5 refreshes, 75 ms |
| Its samples in the fourth refresh or later | 0 of 1591 | 0 of 1591 | |
| Idle, package idle wakeups | 0 and 0/s | 0 and 0/s | 0/s |
| Idle, interrupt wakeups | 0.35 and 0.35/s | 0.35 and 0.33/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest, the four rounds of the second session | 27.8, 28.0, 28.0 and 28.0 MB | 28.5, 28.3, 28.4 and 28.4 MB | ≤ 35 MB |
| GPU memory | 30.6 MB | 30.6 MB | |
| `cat` of 1 GiB of ASCII on screen, median of 4 | 315.6 MiB/s | 314.9 MiB/s | ≥ 295 MiB/s |
| `seq 1 10000000` | 213.6 MiB/s | 217.9 MiB/s | |
| Colored text, SGR | 286.4 MiB/s | 286.3 MiB/s | |
| CJK and emoji | 214.6 MiB/s | 214.9 MiB/s | |
| A frame on the CPU, ASCII, p50 and p99 of each round | 0.295 and 0.450, 0.294 and 0.428 ms | 0.296 and 0.416, 0.300 and 0.429 ms | p99 ≤ 2 ms |
| A frame on the CPU, CJK and emoji, p50 of each round | 0.784 and 0.732 ms | 0.795 and 0.771 ms | |

- **Every budget holds but the startup's gap, which holds for neither version, and no measure is worse beyond its spread.** The latency's p50 is 1.1 ms shorter, in seven rounds of eight, and its p99 1.1.0's, within the rounds' deviations of 0.9 and 1.3 ms, of no sample in the fourth refresh. The throughputs and the frames are 1.1.0's within their runs' spread; idle is 1.1.0's.
- **The memory at rest is 0.5 MB larger**, 28.4 against 27.9 MB, in each of four rounds, within the tolerance of 2: `vmmap` finds it in the app's heap, 11.2 against 10.5 MB allocated, of 2,200 allocations more, the rest of the regions alike. The first session's two rounds of each, of 27.8 to 36.1 MB, varied as much for 1.1.0 as for 1.2.0, measured after 80 s of the idle's measure and in a busier session, which the second's four rounds of each, within 0.2 MB of one another, take the place of.
- **The startup is 5.3 ms longer**, of copies started as often, 254.3 against 248.9 ms, 2 %: the eight rounds' differences from −8.7 to +16.5 ms, their deviation 8.4 ms, within the rounds' spread; the first session's 8.6 ms of copies seen less, as a binary that the system has seen little of starts slower. The startup's gap to the minimal app, 53 and 58 ms, holds for neither version: the minimal app started in 179 to 210 ms in these sessions, against 192 to 198 in 1.1.0's, of a load that moved it by 30 ms from one round to the next.

## The window's motion

The means of the two rounds, the windows on the built-in display at 60 Hz:

| Measure | 1.1.0, opaque | 1.2.0, opaque | 1.1.0, a blur | 1.2.0, a blur |
|---|---|---|---|---|
| A drag, frames a second shown | 57.7 | 57.1 | 58.1 | 58.0 |
| Its refreshes missed of 600 | 16.0 | 23.0 | 15.0 | 16.0 |
| A live resize, frames a second shown | 56.2 | 55.7 | 55.8 | 56.3 |
| Its refreshes missed of 600 | 31.5 | 33.0 | 36.5 | 31.0 |
| A live resize, the window server's CPU and the app's | 35 % and 16 % | 34 % and 16 % | 31 % and 15 % | 34 % and 15 % |
| A live resize, the compositor's delay at the 95th percentile | 20.6 ms | 28.3 ms | 20.8 ms | 19.8 ms |

- **The window's motion is 1.1.0's** within the runs' spread: as many frames a second of a drag and of a resize, as many refreshes missed, the runs' own from 10 to 24 of a drag and 23 to 40 of a resize for both, and the same CPU.
- **The compositor's delay at the 95th percentile** is of two values, 13 to 14 ms in some runs and 26 to 28, a refresh more, in others, for both versions: in the first round, 1.1.0's four runs were of the shorter and 1.2.0's four of the longer; in the second, of the reverse order, three of 1.1.0's and two of 1.2.0's of the longer, as the load rose from 2.7 to 6.5 through the session, of Microsoft Defender's scans of the recordings as they were written. The median delay is 13 ms for both, and the frames shown and missed tell no difference: a session of more rounds, at rest, would tell whether the first round's was 1.2.0's.

## The binary's size

| | 1.1.0 | 1.2.0 |
|---|---|---|
| The bundle | 42.6 MB | 43.4 MB |
| The app's binary | 17.6 MB | 18.3 MB |
| Sleip's binary | 16.4 MB | 16.5 MB |

The app grows by 0.7 MB, of the Theme Editor and its previews, the themes made, duplicated, converted and imported in the app, the rules of the interface's colors, faint text, the cursor guide and the HDR cursor, and the settings' Keys page.
