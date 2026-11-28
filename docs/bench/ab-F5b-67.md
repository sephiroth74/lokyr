# 1.6.0 against 1.5.0

Measured on 2026-11-28 in sessions of [ttybench](https://github.com/sephiroth74/ttybench), the toolkit that measures terminal emulators on macOS: 1.5.0's tag built into a bundle, 1.5.0 (202611250509), against the bundle of 1.6's code at commit 445c7de4, 1.5.0 (202611280854), whose version 1.6.0 raises; both signed alike, neither notarized, so that the system checks both alike at each start. The sessions' files are [`tools/bench/sessions`](../../tools/bench/sessions), and each session's report, records and log are in [ab-F5b-67](ab-F5b-67/).

## Method

- **The A/B in six steps**, as [1.5.0's](ab-F5b-20.md#method), each a session of its own balanced orders, AB BA BA AB, and the same pause of 2 s before each run: the startup, each copy and ttybench's floor app launched 30 times first, the floor app beside each round, and the memory; a window at rest, 30 s of each copy 20 s after its launch; a key's latency, 100 keys of each copy in each of eight cycles; 1 GiB of plain text, of colored text and of wide characters printed on screen, and of plain text in a window of 216 × 64 cells; 10 s of a trackpad's scroll, by rows, then of smooth scrolling; the window dragged and resized along a sine, 10 s each, and each frame of a smooth scroll of Instruments' trace.
- **One config for both**, the releases' of the least work: Menlo at 13 points, 120 × 40 cells, no ligatures, a cursor that doesn't blink, an opaque window, no quick tip, no windows restored; smooth scrolling off, and on in a config of its own for the runs of that feature.
- **The intervals resample each copy's runs first**, then each run's samples, so that a run that moves as a whole of the machine's state is not told as a change.
- **What 1.6 brings to every run**: its copy runs the automation's plugin, which hears what scripts and shortcuts ask, and loads the libraries of the Shortcuts actions at each launch; 1.5.0 has neither. The window at rest tells the automation's cost when nothing scripts Lokyr, and the startup tells the libraries'.
- **A hidden session's memory**, of each copy, a second session's window hidden and a program at rest in its shell's place, as [1.5.0's scenario](ab-F5b-20.md#the-scenarios-of-15s-features) had it: 1.6 changes what a hidden window holds.
- **Checks**: a step whose interval told a change of four runs or fewer of each copy ran again of more cycles: the output in the large window of eight cycles, and the drag of six.

## Machine

- Apple M5 Pro, macOS 27.2.1, on AC power, its lid closed; the display a 4K one of 28 inches at 60 Hz and a scale of 2, of 3008 × 1692 points.
- The load average 1 to 10 through the steps, and up to 23 in the drag's check: the system's work, an antivirus's among it, which took up to half a core in the drag's check, an editor and a browser open, as each session's runs' table tells.

## Results

| Measure | 1.5.0 | 1.6.0 | Change, its 95 % interval | Budget |
|---|---|---|---|---|
| Startup, the launch to the window, median of 32 | 354.6 ms | 357.5 ms | +2.9 ms, −12.9 … +10.3 | |
| Startup, the launch to the shell's command | 792.6 ms | 796.5 ms | +3.9 ms, −66.7 … +68.9 | |
| Startup, its window after the floor app's | 92.5 ms | 86.0 ms | −6.5 ms, −15.8 … +10.3 | ≤ 70 ms ✘ both |
| Memory, the processes' footprint with the GPU's | 55.01 MiB | 54.55 MiB | −0.46, −0.67 … −0.30 | |
| Idle, interrupt wakeups, their mean | 0.33/s | 0.32/s | −0.01, −0.13 … +0.12 | ≤ 0.5/s ✔ |
| Idle, package idle wakeups | 0/s | 0/s | | ≤ 0/s ✔ |
| Idle, the CPU | 0.051 % | 0.052 % | +0.001, −0.001 … +0.004 | |
| Latency, a key to its echo, p50 | 11.41 ms | 11.42 ms | +0.01 ms, −0.18 … +0.18 | ≤ 33.3 ms ✔ |
| Latency, p99 | 26.33 ms | 24.54 ms | | ≤ 75 ms ✔ |
| 1 GiB of plain text on screen, to the quiet | 315.4 MiB/s | 316.2 MiB/s | +0.8, −4.1 … +4.5 | ≥ 275 MiB/s ✔ |
| Colored text | 278.5 MiB/s | 283.2 MiB/s | +4.7, −10.3 … +13.3 | |
| Wide characters | 328.0 MiB/s | 324.9 MiB/s | −3.1, −22.3 … +7.3 | |
| Plain text in a window of 216 × 64 cells, eight cycles | 322.8 MiB/s | 318.7 MiB/s | −4.1, −8.8 … +3.4 | |
| A scroll by rows, frames a second that moved the text | 15.87 | 15.87 | ±0 | |
| A smooth scroll, frames a second that moved the text | 60.79 | 60.84 | +0.05, +0.00 … +0.25 | |
| A smooth scroll, refreshes of a new frame | 98.92 % | 99.00 % | +0.08, −0.50 … +0.66 | |
| A smooth scroll, a frame's GPU time | 0.273 ms | 0.273 ms | ±0 | |
| A drag, frames a second, six cycles | 58.08 | 57.97 | −0.11, −0.96 … +0.74 | |
| A drag, the compositor's delay, six cycles | 13.25 ms | 12.98 ms | −0.27, −0.71 … +0.04 | |
| A drag, Lokyr's CPU, six cycles | 3.54 % | 4.01 % | +0.47, −0.18 … +1.03 | |
| A drag, the window server's CPU, six cycles | 31.90 % | 34.59 % | +2.69, −1.74 … +6.16 | |
| A live resize, frames a second | 56.64 | 57.01 | +0.37, −0.53 … +1.28 | |
| A live resize, the compositor's delay, p95 | 12.88 ms | 26.71 ms | | |

- **Every budget of ttybench's measures holds but the startup's gap, past it for both copies, and no measure is worse beyond its runs' spread.**
- **The startup is 1.5.0's**: the launch to the window 2.9 ms later, within the noise, the libraries of the Shortcuts actions with it; the gap to the floor app's window 6.5 ms shorter, within the noise. Both copies were past the gap's budget, 92.5 and 86.0 ms, against 70.4 ms of 1.5.0's notarized app in [the startup beside a floor app](startup-TB-23.md#results): bundles not notarized start up to 12.5 ms later than the same code notarized, as [the startup and the output at rest](rest-F5a-123.md) found, and an editor open beside them took a core at times.
- **The automation wakes nothing at rest**: the wakeups, the CPU and the power of 1.6.0's window at rest are 1.5.0's.
- **The latency is 1.5.0's**, each run's median between 11.1 and 11.6 ms for both, eight runs of each. It is 5 ms shorter than in 1.5.0's session for both versions, of this session's display.
- **Output on screen is 1.5.0's.** In the window of 216 × 64 cells, the step of four cycles told 1.6.0 2.6 % slower, 314.4 against 322.8 MiB/s; eight cycles of the same session told it within the noise, 318.7 against 322.8, each run of both between 312 and 330.
- **The drag's frames and the compositor's delay are 1.5.0's.** The step of two cycles told Lokyr's CPU 0.7 points of a core higher in the drag and the window server's 6.5, of two runs each; six cycles told both within the noise, 0.47 and 2.69 points higher, each run of 1.5.0 from 2.9 to 4.3 % and of 1.6.0 from 3.3 to 4.3 %, while the antivirus took up to half a core. The live resize took less of both in the step, 0.4 and 0.7 points.
- **The live resize's compositor's delay** at the p95 was 26.71 ms of 1.6.0, one run of its two a refresh longer, 28.51 ms against 12.80, as the window server holds the compositor a refresh longer of some state of its own, as [1.5.0's session](ab-F5b-20.md#results) found of its drag; the median is 1.5.0's, 12.05 against 11.90 ms.

## A hidden session's memory

| Measure | 1.5.0 | 1.6.0 | Change, its 95 % interval |
|---|---|---|---|
| One session, the footprint with the GPU's | 54.96 MiB | 49.53 MiB | |
| With a hidden session | 93.82 MiB | 84.20 MiB | −9.62, −11.78 … +4.52 |
| With a hidden session, the peak footprint | 251.1 MiB | 257.6 MiB | +6.6, +2.1 … +26.1 |

- **A hidden session takes some 9.6 MiB less**: three runs of 1.5.0's four took 93.8 to 94.1 MiB, and every run of 1.6.0 82.0 to 84.6, as [a hidden session's memory](hidden-session-F5b-47.md) measured, the hidden window letting its drawables go; a fourth run of 1.5.0, 79.7 MiB, widens the interval past 0.
- **The peak rises for a moment**, 6.6 MiB, as that report found: the window that shows draws a new drawable of its size while the hidden one's frame of a pixel is still in the GPU's hands.
- **One session's footprint** came at 49.5 or 55.0 MiB from a run to the next, of both copies, which the step of the startup's memory, at 54.6 and 55.0, does not tell apart.

## The measures of `lokyr-bench`

The releases' measures that ttybench does not take came of `lokyr-bench`, of the same two copies and config, in the steps of [run.sh](ab-F5b-67/run.sh), each of its balanced orders; their reports are in [ab-F5b-67/lokyr-bench](ab-F5b-67/lokyr-bench/), and [analyze.py](ab-F5b-67/analyze.py)'s tables in [analysis.txt](ab-F5b-67/analysis.txt).

| Measure | 1.5.0 | 1.6.0 | Budget |
|---|---|---|---|
| Memory at rest without the GPU's, the median of four rounds | 28.5 MB | 28.4 MB | ≤ 35 MB ✔ |
| The GPU's memory | 30.6 MB | 30.6 MB | |
| A frame on the CPU, ASCII, p50 and p99 of each round | 0.298 and 0.440, 0.298 and 0.419 ms | 0.298 and 0.477, 0.299 and 0.441 ms | p99 ≤ 2 ms ✔ |
| A frame on the CPU, CJK and emoji, p50 of each round | 0.769 and 0.745 ms | 0.797 and 0.741 ms | |
| `cat` of 1 GiB of ASCII on screen, a window of 1920 × 1080, the median of four | 308.0 MiB/s | 311.4 MiB/s | |
| `seq 1 10000000` on screen | 221.1 MiB/s | 227.4 MiB/s | |
| Colored text, SGR | 282.5 MiB/s | 252.9 MiB/s | |
| CJK and emoji | 214.6 MiB/s | 214.8 MiB/s | |

- **The memory at rest and a frame's time on the CPU are 1.5.0's.**
- **`cat` and `seq` on screen are 1.5.0's.** The colored text's runs of 1.6.0 spread from 216 to 283 MiB/s, two of them below every run of 1.5.0, 261 to 295, the other two within them; ttybench's colored text, of each copy's four runs, told 1.6.0 1.7 % faster, within the noise.
- **The startup is ttybench's alone from 1.6.0**, its gap to the floor app's window, which launches alike and holds along a session, where `lokyr-bench`'s gap to the minimal app's first window moved by tens of milliseconds between rounds, as [the startup beside a floor app](startup-TB-23.md) found.

## The sessions

Each session's report, of ttybench, in the order that they ran:

- [ab-1-start-2026-11-28-1018](ab-F5b-67/ab-1-start-2026-11-28-1018/report.md): the startup, each copy launched 30 times first, beside the floor app, and the memory.
- [ab-2-idle-2026-11-28-1028](ab-F5b-67/ab-2-idle-2026-11-28-1028/report.md): a window at rest, 20 s after the launch.
- [ab-3-latency-2026-11-28-1035](ab-F5b-67/ab-3-latency-2026-11-28-1035/report.md): the latency, eight cycles of 100 keys.
- [ab-4-output-2026-11-28-1042](ab-F5b-67/ab-4-output-2026-11-28-1042/report.md): the output on screen.
- [ab-4-output-large-2026-11-28-1045](ab-F5b-67/ab-4-output-large-2026-11-28-1045/report.md): the output in a window of 216 × 64 cells, four cycles.
- [ab-5-scroll-2026-11-28-1046](ab-F5b-67/ab-5-scroll-2026-11-28-1046/report.md): the scroll by rows and smooth.
- [ab-6-motion-2026-11-28-1051](ab-F5b-67/ab-6-motion-2026-11-28-1051/report.md): the window's motion and a smooth scroll's frames.
- [scenario-hidden-session-memory-2026-11-28-1109](ab-F5b-67/scenario-hidden-session-memory-2026-11-28-1109/report.md): a hidden session's memory.
- [output-large-check-2026-11-28-1127](ab-F5b-67/output-large-check-2026-11-28-1127/report.md): the output in the large window again, eight cycles, of [its session's file](ab-F5b-67/output-large-check.toml).
- [drag-check-2026-11-28-1129](ab-F5b-67/drag-check-2026-11-28-1129/report.md): the drag alone, six cycles, of [its session's file](ab-F5b-67/drag-check.toml).
