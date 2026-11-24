# 1.5.0 against 1.4.0

Measured on 2026-11-24 in sessions of [ttybench](https://github.com/sephiroth74/ttybench), the toolkit that measures terminal emulators on macOS: 1.4.0's tag built into a bundle, 1.4.0 (202611220721), against the bundle of 1.5's code at commit bb12f907, 1.4.0 (202611241345), whose version 1.5.0 raises; both signed alike, neither notarized, so that the system checks both alike at each start. The sessions' files are [`tools/bench/sessions`](../../tools/bench/sessions), and each session's report, records and log are in [ab-F5b-20](ab-F5b-20/).

## Method

- **The A/B in six steps** of a few minutes each, each a session of its own balanced orders, AB BA BA AB, and the same pause of 2 s before each run, so that each step compares its two copies of its own conditions: the startup, each copy launched 30 times first, and the memory; a window at rest, 30 s of each copy 20 s after its launch; a key's latency, 100 keys of each copy in each of eight cycles; 1 GiB of plain text, of colored text and of wide characters printed on screen; 10 s of a trackpad's scroll, by rows, then of smooth scrolling; the window dragged and resized along a sine, 10 s each, and each frame of a smooth scroll of Instruments' trace.
- **One config for both**, the releases' of the least work: Menlo at 13 points, 120 × 40 cells, no ligatures, a cursor that doesn't blink, an opaque window, no quick tip, no windows restored; smooth scrolling off, and on in a config of its own for the runs of that feature.
- **The intervals resample each copy's runs first**, then each run's samples: a run moves as a whole of the machine's state, as another program's work raises the processor's clocks, which a resample of the samples alone would tell as a change. Each session's report tells each run's load average and its busiest processes beside it.
- **The scenarios of 1.5's features**, which 1.4.0 has not, of 1.5's copy against itself: a config whose plugin builds the scenario as Lokyr starts, of the same balanced orders. The broadcast: the measured pane above, of the window's width, and seven panes below of a program that takes the keys and draws nothing, the typed input broadcast to the tab. The hidden session: a second session's window hidden, a program at rest in its shell's place. The floating pane: a popup of a program at rest over the pane.
- **The scrollback** at the quit and at the next start, of the app's own measure, `scrollback_costs`, as [the scrollback's report](scrollback-F5b-04.md) measured it, of 1.5's code.

## Machine

- Apple M5 Pro, macOS 27.2.1, on AC power; the main display an external one at 60 Hz, of 3440 × 1440 for the startup and the rest, and a 5K one at a scale of 2 from the latency of eight cycles on.
- The load average 3 to 8 through the sessions: the system's work, an antivirus's among it, which took a processor's core and more for a minute at times, as each session's runs' table tells; the folders that the sessions write and run excluded of its scans.

## Results

| Measure | 1.4.0 | 1.5.0 | Change, its 95 % interval | Budget |
|---|---|---|---|---|
| Startup, the launch to the window, median of 32 | 365.8 ms | 372.1 ms | +6.2 ms, −0.6 … +16.5 | |
| Startup, the launch to the shell's command | 713.8 ms | 718.6 ms | +4.8 ms, −4.2 … +11.2 | |
| Memory, the processes' footprint with the GPU's | 45.26 MiB | 45.29 MiB | +0.03, −0.13 … +0.13 | |
| Idle, interrupt wakeups, their mean | 0.35/s | 0.33/s | | ≤ 0.5/s ✔ |
| Idle, package idle wakeups | 0/s | 0/s | | ≤ 0/s ✔ |
| Idle, the CPU | 0.059 % | 0.055 % | | |
| Latency, a key to its echo, p50 | 16.81 ms | 16.34 ms | −0.47 ms, −1.57 … +0.70 | ≤ 33.3 ms ✔ |
| Latency, p99 | 31.16 ms | 27.97 ms | | ≤ 75 ms ✔ |
| 1 GiB of plain text on screen, to the quiet | 313.0 MiB/s | 313.9 MiB/s | +0.9, −3.8 … +6.3 | ≥ 275 MiB/s ✔ |
| Colored text | 282.5 MiB/s | 282.0 MiB/s | −0.5, −8.9 … +3.6 | |
| Wide characters | 330.3 MiB/s | 330.3 MiB/s | −0.1, −1.8 … +6.0 | |
| A scroll by rows, frames a second that moved the text | 15.87 | 15.87 | ±0 | |
| A smooth scroll, frames a second that moved the text | 60.74 | 60.74 | ±0, −0.15 … +0.15 | |
| A smooth scroll, refreshes of a new frame | 98.59 % | 99.17 % | +0.58, +0.17 … +0.99 | |
| A smooth scroll, a frame's GPU time | 0.273 ms | 0.273 ms | ±0 | |
| A drag, frames a second, four cycles | 57.87 | 58.13 | +0.26, −1.17 … +1.38 | |
| A drag, the compositor's delay, four cycles | 12.72 ms | 12.45 ms | −0.26, −0.45 … +0.08 | |
| A live resize, frames a second | 58.82 | 58.82 | ±0, −1.39 … +1.38 | |
| A live resize, the compositor's delay, p95 | 26.48 ms | 27.92 ms | | |

- **Every budget of ttybench's measures holds, and no measure is worse beyond its runs' spread.** The latency's p50 is 0.47 ms shorter, its p99 3.2 ms shorter; each run's median lies between 13.9 and 18.4 ms for both, eight runs of each.
- **The wakeups at rest are 1.4.0's**, 0.35 a second, its own release's figure, of a mean of the intervals of a second: a wakeup comes a few times a minute, and most intervals hold none. Lokyr wakes some 0.8 times a second in its first ten seconds, so that the step measures from 20 s after the launch, as the releases' measures did.
- **The drag's compositor's delay** was 26.01 ms of 1.5 in the step's two cycles, one run a refresh longer, 28.24 ms against 13.21 of the other, as the window server holds the compositor a refresh longer of some state of its own: four cycles of the drag alone measured 12.72 against 12.45, each run 12.25 to 12.81.
- **Output on screen** measured 236 MiB/s for both in a first session, each cycle of both alike, 208 to 222 then 302, as the antivirus's watcher of the files took half a core while each run wrote its file anew: ttybench writes each file once since, and the session again measured 309 to 319 MiB/s each cycle.

## The scenarios of 1.5's features

| Scenario | Measure | 1.5.0 | 1.5.0 of the scenario | Change, its 95 % interval | Budget |
|---|---|---|---|---|---|
| A key broadcast to eight panes | latency, p50 | 16.95 ms | 17.33 ms | +0.37 ms, −0.78 … +1.40 | ≤ 33.3 ms ✔ |
| | latency, p99 | 30.38 ms | 28.22 ms | | ≤ 75 ms ✔ |
| A hidden session | interrupt wakeups, their mean | 0.330/s | 0.330/s | +0.000, −0.124 … +0.116 | ≤ +0.1/s ✔ |
| | the CPU | 0.058 % | 0.057 % | −0.0005, −0.0057 … +0.0043 | |
| | memory | 55.4 MiB | 94.1 MiB | +38.7 MiB | |
| A floating pane at rest | interrupt wakeups, their mean | 0.339/s | 0.331/s | −0.008, −0.140 … +0.116 | ≤ +0.1/s ✔ |
| | frames at rest | 0 | 0 | | |
| | memory | 55.3 MiB | 72.8 MiB | +17.6 MiB | |

- **A key broadcast to eight panes costs nothing measurable**: its latency is within the noise of one pane's, no key missed. The measured pane is 20 rows tall in the scenario, 40 alone, which the report's tables tell apart: its A/B comes of a copy of the session of no cells' count, [broadcast-of-no-size](ab-F5b-20/broadcast-of-no-size/report.md).
- **A hidden session wakes nothing**: the wakeups, the CPU and the power are a single session's. Its window keeps the surface of its last frame, some 39 MiB. A first measure of the session's login shell in the hidden window found 1.0 more wakeup a second, of the shell's prompt, which wakes each second of its own, Lokyr's own wakeups 0.40 a second: the scenario puts a program at rest in its place since.
- **A floating pane at rest draws and wakes nothing**; its surface takes some 18 MiB.

## The scrollback

Of [scrollback.txt](ab-F5b-20/scrollback.txt), one pass, as [the scrollback's report](scrollback-F5b-04.md):

| Panes | Fill | Quit, off | Quit, on | Start, off | Start, on | Rows shown | Files |
|---|---|---|---|---|---|---|---|
| 1 | full | 1.0 ms | 12.9 ms | 10.6 ms | 10.1 ms | 16.7 ms | 1.4 MB |
| 10 | half | 1.2 ms | 61.6 ms | 28.8 ms | 38.6 ms | 41.4 ms | 7.3 MB |
| 10 | full | 2.6 ms | 127.4 ms | 35.4 ms | 44.2 ms | 47.1 ms | 13.7 MB |
| 50 | half | 1.4 ms | 123.3 ms | 170.0 ms | 197.0 ms | 199.1 ms | 12.8 MB |
| 50 | full | 2.3 ms | 109.3 ms | 154.8 ms | 205.4 ms | 207.4 ms | 11.4 MB |

- **The quit stays within its 300 ms**, at 127 ms at most.
- **Ten full panes start 8.8 ms later** before the first frame, against 7.1 ms in the first measure, of one pass each and a load average of 4.2 at its start; the files are the first measure's, byte for byte.

## The measures of `lokyr-bench`

The releases' measures that ttybench does not take came of `lokyr-bench`, of the same two copies and config, in the steps of [run.sh](ab-F5b-20/run.sh), each of its balanced orders; their reports are in [ab-F5b-20/lokyr-bench](ab-F5b-20/lokyr-bench/), and [analyze.py](ab-F5b-20/analyze.py)'s tables in [analysis.txt](ab-F5b-20/analysis.txt).

| Measure | 1.4.0 | 1.5.0 | Budget |
|---|---|---|---|
| Startup, warm, first frame, the median of eight rounds | 250.2 ms | 253.1 ms | |
| Its gap to the minimal app's first window, the mean of eight rounds | 47.1 ms | 47.8 ms | ≤ 35 ms ✘ both |
| The same at rest, from a terminal of no other app open, eight rounds | 34.9 ms | 34.6 ms | ≤ 35 ms ✔ |
| Memory at rest without the GPU's, the median of four rounds | 28.9 MB | 29.0 MB | ≤ 35 MB ✔ |
| A frame on the CPU, ASCII, p50 and p99 of each round | 0.363 and 0.440, 0.358 and 0.461 ms | 0.358 and 0.443, 0.361 and 0.463 ms | p99 ≤ 2 ms ✔ |
| A frame on the CPU, CJK and emoji, p50 of each round | 0.795 and 0.791 ms | 0.738 and 0.815 ms | |
| `cat` of 1 GiB of ASCII on screen, a window of 1920 × 1080, the median of four | 277.1 MiB/s | 270.4 MiB/s | |
| `seq 1 10000000` on screen | 185.8 MiB/s | 208.9 MiB/s | |

- **The startup is 1.4.0's**: 1.5.0 started 0.7 ms later in the mean of eight rounds, each round's change from −18.1 to +23.3 ms, and 7.1 ms later of four rounds before, of a first round 20 ms apart.
- **The startup's gap was past its budget for both copies in the session**, 47.1 and 47.8 ms, as the minimal app's first window came in 182 to 226 ms from a round to the next, which moved each round's gap by as much, as [the startup and the output at rest](rest-F5a-123.md) found of 1.4's A/B. **At rest**, eight rounds more from a terminal of no other app open, of [their reports](ab-F5b-20/lokyr-bench/rest/) and [tables](ab-F5b-20/analysis-rest.txt), the minimal app came in 218 to 224 ms, and the gap is within its budget for both, 34.9 ms of 1.4.0 and 34.6 of 1.5.0, 1.5.0 0.3 ms quicker in the mean of the rounds; of bundles not notarized, which start up to 12.5 ms later than the same code notarized, as that report found.
- **The memory at rest and a frame's time on the CPU are 1.4.0's.**
- **`cat` on screen in a window of 1920 × 1080 came 2.4 % slower** of 1.5.0, three runs of four, 270.4 against 277.1 MiB/s, where ttybench's of a window of 120 × 40 cells told both copies alike, 313.9 against 313.0; `seq`'s and the colored text's runs spread too widely to tell a change.

## The sessions

Each session's report, of ttybench, in the order that they ran:

- [ab-1-start-2026-11-24-1518](ab-F5b-20/ab-1-start-2026-11-24-1518/report.md): the startup, each copy launched 30 times first, and the memory.
- [ab-2-idle-2026-11-24-1538](ab-F5b-20/ab-2-idle-2026-11-24-1538/report.md): a window at rest, 3 s after the launch, which Lokyr's first seconds' wakeups moved.
- [ab-2-idle-2026-11-24-1554](ab-F5b-20/ab-2-idle-2026-11-24-1554/report.md): a window at rest, 20 s after the launch.
- [ab-3-latency-2026-11-24-1616](ab-F5b-20/ab-3-latency-2026-11-24-1616/report.md): the latency, four cycles of 200 keys, beside the machine's other work.
- [ab-3-latency-2026-11-24-1631](ab-F5b-20/ab-3-latency-2026-11-24-1631/report.md): the latency again, the Mac left alone first.
- [ab-3-latency-2026-11-24-1644](ab-F5b-20/ab-3-latency-2026-11-24-1644/report.md): the latency again, beside the antivirus's scans.
- [ab-3-latency-2026-11-24-1827](ab-F5b-20/ab-3-latency-2026-11-24-1827/report.md): the latency, eight cycles of 100 keys.
- [ab-4-output-2026-11-24-1841](ab-F5b-20/ab-4-output-2026-11-24-1841/report.md): the output on screen, each run's file written anew.
- [ab-4-output-2026-11-24-1923](ab-F5b-20/ab-4-output-2026-11-24-1923/report.md): the output on screen, each file written once.
- [ab-5-scroll-2026-11-24-1859](ab-F5b-20/ab-5-scroll-2026-11-24-1859/report.md): the scroll by rows and smooth.
- [ab-6-motion-2026-11-24-1904](ab-F5b-20/ab-6-motion-2026-11-24-1904/report.md): the window's motion and a smooth scroll's frames.
- [drag-check-2026-11-24-1926](ab-F5b-20/drag-check-2026-11-24-1926/report.md): the drag alone, four cycles.
- [scenario-broadcast-2026-11-24-2008](ab-F5b-20/scenario-broadcast-2026-11-24-2008/report.md): a key broadcast to eight panes.
- [broadcast-of-no-size](ab-F5b-20/broadcast-of-no-size/report.md): the broadcast's session of no cells' count, of its A/B.
- [scenario-hidden-session-2026-11-24-2048](ab-F5b-20/scenario-hidden-session-2026-11-24-2048/report.md): a hidden session, its login shell running.
- [scenario-hidden-session-2026-11-24-2058](ab-F5b-20/scenario-hidden-session-2026-11-24-2058/report.md): a hidden session, a program at rest in its shell's place.
- [scenario-floating-2026-11-24-2106](ab-F5b-20/scenario-floating-2026-11-24-2106/report.md): a floating pane at rest.

## Of ttybench's measures and the releases' before

The releases' measures before 1.5.0 came of `lokyr-bench` and the scripts of `tools/bench`. ttybench measures any terminal alike, and some figures differ of its method: the latency of its capture of the window, and of its keys at every phase of the refresh; the throughput to the quiet of the terminal's processes; the memory of the processes' footprint with the GPU's. [RELEASES.md](RELEASES.md) takes 1.5.0's values and 1.4.0's of this session, which its summary compares, and tells which measures came of which tool.
