# 1.1.0 against 1.0.0

Measured on 2026-11-14 in two sessions, between 21:33 and 21:48 and between 22:23 and 22:39, unattended: the app of 1.0.0's DMG, of commit 9b3b378b, notarized and stapled, against 1.1.0's bundle, of commit cbc576eb, signed and not notarized, whose code 1.1.0 releases; then the startup again, of copies the system had seen as often. The reports are in [ab-F3b-20](ab-F3b-20/), the second session's at its root and the first's in [`session1`](ab-F3b-20/session1/), with the sessions' script, [`run.sh`](ab-F3b-20/run.sh), its analysis, [`analyze.py`](ab-F3b-20/analyze.py), the tables, [`analysis.txt`](ab-F3b-20/analysis.txt), and the startup measured again, [`startup-seasoned.txt`](ab-F3b-20/startup-seasoned.txt).

## Method

As in [1.0.0's A/B](ab-F3-85.md#method), of balanced orders: the startup, warm, 10 runs after 2, to the first frame on the display, four rounds, AB BA BA AB, each of the minimal AppKit app of F0 as its floor; the latency of 200 keys, eight rounds, after a warm-up of 20 keys of each bundle; idle over 60 s and memory over 10 s of a bundle started with `open -n`, 20 s after its start, two rounds; the throughput of the four workloads of [onscreen-F2-56](onscreen-F2-56.md) on screen, two runs in each of two rounds; the same pause of 2 s before each run of the startup and of the latency.

Its config, [`config.toml`](ab-F3b-20/config.toml), is new, and the one of the releases' sessions from 1.1.0 on: the least work beside the terminal's own, of keys that both versions know. Menlo at 13 points, 120 × 40 cells and about 10 000 rows of scrollback, as before; no ligatures, an opaque window of no blur, no blinking of the cursor nor of the text, the cursor moving at once, no bell's flash nor bounce, no quick tip at the start and no windows restored. No installed plugin runs: the benches load none, and the session's config folder holds no `plugins.lock`.

The sessions started each copy once before their measures, as the A/Bs before. A binary that the system has seen little of starts slower, which the startup measured again found: [the startup](#the-startup) tells it, and the script now starts each copy 30 times first. The window's motion did not run.

## Machine

- Apple M5 Pro, macOS 27.2.1, on AC power, the built-in Liquid Retina XDR display at 120 Hz, a scale of 2: the releases' sessions before were of a Dell P2721Q at 60 Hz.
- The load average 3 to 6 through the sessions, of no other work but the system's, Jamf's and Microsoft Defender's.

## Results

Of the second session, the first's in brackets where it differs beyond a percent:

| Measure | 1.0.0 | 1.1.0 | Budget |
|---|---|---|---|
| Startup, warm, first frame, median of the four rounds | 239.7, 239.1, 235.2 and 235.6 ms | 245.5, 243.8, 252.2 and 246.8 ms | |
| The same, of copies started as often, the mean of eight rounds | 242.9 ms | 245.6 ms | |
| Latency at 120 Hz, p50, the mean of 8 rounds | 14.44 ms (14.66) | 13.91 ms (14.08) | ≤ 2 refreshes, 16.7 ms |
| Latency at 120 Hz, p99, the mean of 8 rounds | 32.25 ms | 31.97 ms (32.52) | ≤ 4.5 refreshes, 37.5 ms |
| Its samples in the fourth refresh or later | 54 of 1592 (58) | 46 of 1592 (69) | |
| Idle, package idle wakeups | 0 and 0/s | 0 and 0/s | 0/s |
| Idle, interrupt wakeups | 0.35 and 0.35/s | 0.35 and 0.35/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest | 27.9 and 27.8 MB | 28.3 and 28.2 MB (27.8 and 28.0) | ≤ 35 MB |
| GPU memory | 30.6 MB | 30.6 MB | |
| `cat` of 1 GiB of ASCII on screen, median of 4 | 268.4 MiB/s (274.5) | 271.1 MiB/s (276.9) | ≥ 294.9 MiB/s |
| `seq 1 10000000` | 193.6 MiB/s (198.1) | 192.6 MiB/s (198.4) | |
| Colored text, SGR | 245.6 MiB/s (232.2) | 234.7 MiB/s (255.1) | |
| CJK and emoji | 209.4 MiB/s (205.5) | 207.8 MiB/s (206.7) | |
| A frame on the CPU, ASCII, p50 and p99 of each round | 0.368 and 0.443, 0.366 and 0.444 ms | 0.386 and 0.467, 0.384 and 0.458 ms | p99 ≤ 2 ms |
| A frame on the CPU, CJK and emoji, p50 of each round | 0.787 and 0.757 ms | 0.801 and 0.774 ms | |

- **No measure is worse beyond its spread, but a frame of ASCII on the CPU.** The latency's p50 is 0.5 ms shorter in both sessions and its p99 1.1.0's, within the rounds' deviations of 0.6 and 4.5 ms. The throughputs are within their runs' spread, SGR's the slower in one session and the faster in the other. The memory at rest is 0.2 to 0.4 MB larger, within the tolerance of 2, of a binary 1 MB larger; idle is 1.0.0's.
- **A frame of ASCII takes 0.018 ms more on the CPU**, 0.385 against 0.367 ms at its p50, in both sessions alike, 5 %, the CJK frame 2 %: far within the budget's 2 ms at its p99, of no throughput lost.
- **Two budgets hold for neither version in these sessions**: the startup's gap to the minimal app, 42.5 and 52.2 ms against 35, as the minimal app started in 192 to 198 ms, some 15 ms faster than in the sessions before, while Lokyr's own startup stayed near 1.0.0's of its session, 236 ms; and `cat`'s throughput, 268 and 271 MiB/s against 295, of the display at 120 Hz, which draws twice the frames of the 60 Hz one of the sessions before, where 1.0.0 drew 311 MiB/s.

## The startup

The sessions found 1.1.0's startup 6.6 ms and then 9.7 ms longer than 1.0.0's, in each of the four rounds. It is not 1.1.0's code, but most of it a binary that the system had seen little of:

- 1.0.0 rebuilt of its own tag, its code the release's, started 16.3 ms later than the release while new, and 2.1 ms sooner after some 30 launches.
- 1.0.0's and 1.1.0's code rebuilt alike, each launched as often, differ by 1.8 ms; 1.0.0's release and 1.1.0's bundle by 2.7 ms in eight rounds, their rounds from −8.0 to +8.6 ms.
- The CPU's time of a startup, of a time profile of ten startups of each, is the same, 107.7 and 107.5 ms on the main thread; a signature of no notarization starts as the notarized one does, 1.0.0's release signed anew 0.7 ms from it.
- Of traces of the startup's steps, 1.1.0's own work is about a millisecond: the config's read 0.3 ms longer, as it reads the config's schema for the texts' bounds, and the commands' and the menus' 0.8 ms, of the catalogs of two plugins more read for their commands' titles, and a menu of three items more.
- A startup ends at the display's refresh after its first frame: at 120 Hz, a refresh of 8.3 ms, its times fall in steps of it, so that a millisecond more moves a session's mean by a part of a refresh.

1.1.0's startup is so some 2.7 ms longer, 1 %, within the rounds' spread.

## The binary's size

| | 1.0.0 | 1.1.0 |
|---|---|---|
| The bundle | 41.4 MB | 42.6 MB |
| The app's binary | 16.6 MB | 17.6 MB |
| Sleip's binary | 16.1 MB | 16.4 MB |

The app grows by 1 MB, of the settings window, its pages of the config's keys, the bindings, the themes and the plugins, and the plugin manager.
