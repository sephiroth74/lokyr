# Lokyr against the other terminals (F2-134)

Measured on 2026-10-31, in five sessions between 14:54 and 22:30, on the bundle of commit e2f9e85, with this task's change of `lokyr --bench output`, which takes the config's columns and rows without `--window` and through which `compare.py` runs each measure's program in Lokyr, against Ghostty, Alacritty, kitty, WezTerm, iTerm2 and Terminal.app, under the same conditions. The raw results, the configs and each terminal's reports are in [compare-F2-134/](compare-F2-134/); `python3 tools/bench/compare.py` takes the measures again, and `python3 tools/bench/report_charts.py compare` draws the charts from the raw results. The user's own part came on 2026-11-01, on the bundle of commit e070548, after F2-152: the latency with Typometer, `powermetrics`, the smoothness by eye and vttest's screens.

## In short

| Measure | Lokyr | Its place of 7 | The best of the others |
|---|---|---|---|
| Startup to the first window, median | 256 ms | 1st | Ghostty, 331 ms |
| Memory at rest, without the GPU's | 25.0 MB | 1st | Alacritty, 31.3 MB |
| Interrupt wakeups at rest | 0.38 a second | 2nd, with Terminal.app | Alacritty, 0.37 |
| `cat` of 1 GiB on screen | 296 MiB/s | 2nd | Ghostty, 306 MiB/s |
| The CPU's energy to show that GiB | 17.3 J | 1st | kitty, 41.1 J |
| kitty's benchmark, short CSI sequences | 58 MB/s | 4th | Ghostty, 221 MB/s |
| A key to its echo at 120 Hz, median, of 5 | 21.2 ms | 1st | iTerm2, 21.3 ms |
| A key to its echo at 120 Hz, p99, of 5 | 32.2 ms | 3rd | Alacritty, 25.8 ms |
| A key to its echo at 60 Hz, median, of 5 | 22.8 ms | 2nd | Alacritty, 21.2 ms |

![Startup to the first window, median of 10 runs: Lokyr 256 ms, Ghostty 331, Terminal.app 340, Alacritty 349, WezTerm 394, kitty 672, iTerm2 794](compare-F2-134/charts/startup.svg)

![Memory at rest without the GPU's: Lokyr 25.0 MB, Alacritty 31.3, Terminal.app 34.8, WezTerm 37.2, Ghostty 40.0, kitty 97.3, iTerm2 136.5](compare-F2-134/charts/memory.svg)

![Interrupt wakeups a second at rest: Alacritty 0.37, Lokyr 0.38, Terminal.app 0.38, WezTerm 1.35, iTerm2 8.18, Ghostty 16.57, kitty 20.08](compare-F2-134/charts/wakeups.svg)

![Throughput of cat of 1 GiB on screen, median of 5 runs: Ghostty 306 MiB/s, Lokyr 296, Alacritty 170, kitty 147, iTerm2 51, WezTerm 45, Terminal.app 40](compare-F2-134/charts/throughput.svg)

![The CPU's energy to show cat of 1 GiB: Lokyr 17.3 J, kitty 41.1, Alacritty 41.9, Ghostty 43.1, iTerm2 107.9, Terminal.app 124.3, WezTerm 196.3](compare-F2-134/charts/energy.svg)

![Latency at 120 Hz, a key to its echo on screen with Typometer, median and 99th percentile: Alacritty 21.6 and 25.8 ms, kitty 21.8 and 29.6, Lokyr 21.2 and 32.2, iTerm2 21.3 and 45.0, Ghostty 24.8 and 47.7](compare-F2-134/charts/latency-120hz.svg)

![Latency at 60 Hz, a key to its echo on screen with Typometer, median and 99th percentile: Alacritty 21.2 and 47.9 ms, Lokyr 22.8 and 50.3, kitty 32.0 and 50.5, iTerm2 35.8 and 50.6, Ghostty 33.1 and 64.7](compare-F2-134/charts/latency-60hz.svg)

## Conditions

- **The machine**: an Apple M5 Pro (Mac17,8), macOS 27.2 (26B123), on AC power, Low Power Mode off; the main display a 6016 × 3384 panel at 60 Hz, as 3008 × 1692 points, on which every window opened. The user's apps stayed open, among them VS Code, Android Studio and Chrome, and Microsoft Defender scanned: a session's figures moved with them, as below, and each measure of a terminal against the others comes of one session.
- **Each terminal**: Menlo at 13 points, no ligatures, a grid of 120 by 40 cells, which `compare.py check` saw through `stty size` in each; a steady cursor, an opaque window without blur, about 10 000 rows of scrollback, no window restored, no shell integration; the configs are in [configs/](compare-F2-134/configs/), and Terminal.app's profile, which `compare.py setup` writes, has the same.
- **The shell**: zsh with `ZDOTDIR` an empty directory, of its plain prompt, in each.
- **The defaults that the measures depend on**, left as they are: Ghostty's `window-vsync = true`; kitty's `input_delay 3`, `repaint_delay 10` and `sync_to_monitor yes`; WezTerm's `front_end = "WebGpu"` and `max_fps = 60`.
- **The versions**:

| Terminal | Version |
|---|---|
| Lokyr | e2f9e85 |
| Ghostty | 1.3.2-main+62fa23bfa, a build of its tip |
| Alacritty | 0.18.0-dev, bbb2cbf of 26 October 2026, built here |
| kitty | 0.49.2 |
| WezTerm | 20240203-110809-5046fc22 |
| iTerm2 | 3.7.6 |
| Terminal.app | 2.15 (488.9) |

## Startup

`lokyr-bench startup`, which launches each terminal through LaunchServices and times its first window on screen from the process's start, a run to warm up and five, twice, the other terminals' order turned between the rounds, while Lokyr came first in both, as the review after F2-148 found; and Lokyr's own first frame, as its presented handler tells it. The spread is the range from the 10th to the 90th percentile, as a fraction of the median, by `lokyr-bench`'s ranks, the 2nd and the 9th of ten runs; `python3 tools/bench/compare_tables.py` makes this table and the others of their raw results.

| Terminal | Median | p90 | Spread | Fastest | Slowest |
|---|---|---|---|---|---|
| **Lokyr** | **256.2 ms** | 258.2 | 0.10 | 211.9 | 278.3 |
| Ghostty | 330.6 ms | 342.3 | 0.12 | 283.4 | 361.9 |
| Terminal.app | 339.6 ms | 375.0 | 0.15 | 319.8 | 397.5 |
| Alacritty | 349.0 ms | 368.5 | 0.13 | 310.6 | 369.7 |
| WezTerm | 394.3 ms | 415.9 | 0.16 | 341.7 | 422.1 |
| kitty | 671.7 ms | 681.6 | 0.03 | 659.2 | 696.1 |
| iTerm2 | 794.2 ms | 1 203.0 | 0.64 | 695.0 | 1 250.7 |

Lokyr's first frame, as its presented handler tells it, in runs of its own, came at 235.1 ms at the median, p90 253.8, a spread of 0.14.

## At rest

Each terminal alone, 10 s after its start, then 60 s of `lokyr-bench idle` and of its energy, then its memory, over the processes of its bundle, without the shell.

| Terminal | Processes | Package idle wakeups | Interrupt wakeups | Footprint without the GPU's | The GPU's | Energy |
|---|---|---|---|---|---|---|
| **Lokyr** | lokyr | 0 a second | 0.38 | **25.0 MB** | 30.5 MB | 0.07 mW |
| Ghostty | ghostty | 0.05 | 16.57 | 40.0 MB | 130.7 MB | 0.52 mW |
| Alacritty | alacritty | 0 | 0.37 | 31.3 MB | 52.1 MB | 0.07 mW |
| kitty | kitty, two kittens | 0 | 20.08 | 97.3 MB | 41.9 MB | 0.15 mW |
| WezTerm | wezterm-gui | 0 | 1.35 | 37.2 MB | 56.9 MB | 0.09 mW |
| iTerm2 | iTerm2 and pidinfo; and the session's server, in its footprint alone | 0.02 | 8.18 | 136.5 MB | 184.4 MB | 5.45 mW |
| Terminal.app | Terminal | 0 | 0.38 | 34.8 MB | 15.2 MB | 0.07 mW |

- Lokyr is within its [budgets](../PERFORMANCE.md#budgets): no package idle wakeup, 0.38 interrupt wakeups a second, of ≤ 0.5, and 25.0 MB, of ≤ 35.
- iTerm2's footprint has the server of its session, 2.2 MB, which [idle/iterm2-server.json](compare-F2-134/idle/iterm2-server.json) measured apart, as iTerm2's servers live outside its bundle; its wakeups and its energy leave the server out.

## Throughput

In each terminal's window, one after the other: kitty's `kitten __benchmark__`, three times without its `--render` and three times with it; vtebench, at commit ead8003, five seconds a scenario; and `cat` of 1 GiB of lines of ASCII, ended with a query of the cursor, five times. F2-56's [measures of Lokyr's own](onscreen-F2-56.md), of other workloads, come with these.

### kitty's benchmark

Without `--render`, MB/s, the median of 3 runs; more is better:

| Terminal | ASCII | Unicode | Unique clusters of several codepoints | Short CSI sequences | Long escape sequences | Images |
|---|---|---|---|---|---|---|
| **Lokyr** | 314 | 298 | 75 | **58** | 317 | 317 |
| Ghostty | 316 | 320 | 104 | 221 | 320 | 320 |
| Alacritty | 117 | 138 | 81 | 94 | 232 | 240 |
| kitty | 173 | 166 | 57 | 114 | 273 | 248 |
| WezTerm | 39 | 75 | 95 | 22 | 286 | 257 |
| Terminal.app | 30 | 41 | 22 | 32 | 59 | 49 |

- With `--render`, the figures are the same within 5%, but Alacritty's, which are faster, 196 MB/s of ASCII and 126 of short CSI sequences, and WezTerm's clusters of several codepoints, 9 MB/s.
- The benchmark waits for an answer that iTerm2 never gives, and was left out of iTerm2's measures.
- Lokyr is within 10% of Ghostty in four tests of six. **Short CSI sequences**, at 0.26 of Ghostty's rate, and **unique clusters of several codepoints**, at 0.72, are past the budget of throughput, within 10% of the faster of Ghostty and Alacritty.
- [F2-150](../tasks/F2-macos-single-pane.md#f2-150--short-csi-sequences-faster) brought short CSI sequences to 233 MB/s, 1.05 of Ghostty's 222 in the same session, whose figures are in [throughput/after-F2-150/](compare-F2-134/throughput/after-F2-150/): a fifth of the test's pieces end with REP, which printed its 100 copies one at a time.
- [F2-151](../tasks/F2-macos-single-pane.md#f2-151--unique-clusters-of-several-codepoints-faster) brought unique clusters of several codepoints to 111.6 MB/s, 1.07 of Ghostty's 104.0 in the same session, whose figures are in [throughput/after-F2-151/](compare-F2-134/throughput/after-F2-151/): each cluster's three combining marks joined it one by one, each looking the cluster up again.

### vtebench

The median time of a scenario's samples, in milliseconds; less is better. vtebench's time ends as the PTY takes the bytes, not as the terminal shows them, in whole milliseconds:

| Scenario | Lokyr | Ghostty | Alacritty | kitty | WezTerm | iTerm2 | Terminal.app |
|---|---|---|---|---|---|---|---|
| `dense_cells` | **4** | 5 | 5 | 8 | 8 | 124 | 25 |
| `medium_cells` | **4** | 4 | 6 | 7 | 33 | 381 | 43 |
| `scrolling` | **8** | 12 | 19 | 64 | 67 | 28 | 146 |
| `scrolling_bottom_region` | **9** | 13 | 10 | 37 | 63 | 548 | 124 |
| `scrolling_bottom_small_region` | **9** | 13 | 10 | 37 | 63 | 546 | 124 |
| `scrolling_fullscreen` | **9** | 16 | 36 | 156 | 78 | 37 | 148 |
| `scrolling_top_region` | **14** | 12 | 29 | 26 | 81 | 2 710 | 123 |
| `scrolling_top_small_region` | **9** | 13 | 10 | 36 | 62 | 546 | 124 |
| `sync_medium_cells` | **4** | 4 | 9 | 14 | 46 | 361 | 44 |
| `unicode` | **3** | 3 | 6 | 7 | 20 | 34 | 59 |

- Lokyr is the fastest, or as fast as the fastest, in every scenario but **`scrolling_top_region`**, 14 ms against Ghostty's 12, a sixth more, past the budget. Its setup reads the rows of terminfo, 24, as its `tput lines` writes to a pipe, so that its region is rows 2 to 24 of 40; [F2-152](../tasks/F2-macos-single-pane.md#f2-152--vtebenchs-top-region) brought it to 12.0 ms, Ghostty's.
- iTerm2 took only two samples of some scenarios in five seconds.

### `cat` of 1 GiB

| Terminal | Median | Its 5 runs |
|---|---|---|
| Ghostty | 305.6 MiB/s | 300.3 to 307.8 |
| **Lokyr** | **295.5 MiB/s** | 290.1 to 296.4 |
| Alacritty | 169.6 MiB/s | 169.0 to 170.8 |
| kitty | 146.7 MiB/s | 145.7 to 148.3 |
| iTerm2 | 51.0 MiB/s | 50.1 to 51.1 |
| WezTerm | 44.6 MiB/s | 44.2 to 44.7 |
| Terminal.app | 39.9 MiB/s | 39.8 to 40.1 |

Lokyr is at 0.97 of Ghostty's rate, within the budget.

### A session that slowed Lokyr

The first session of throughput, at 15:28, ran Lokyr first. Its first run of kitty's benchmark matched the figures above; from the second on, every test fell by a third or more, ASCII to 211 MB/s, short CSI sequences to 26, vtebench's scenarios took three to five times as long, and `cat` ran at 62 MiB/s. Lokyr used the same CPU throughout, about 1.1 to 1.3 cores, as `powermetrics` saw. No state of the terminal explained it: neither vtebench, the alternate screen, each of the benchmark's tests in turn, two whole runs of it, nor the app hidden or in the background slowed it again. A fourth session measured Lokyr again, with Ghostty as the control, whose figures matched the first session's within 2%; the tables above are that session's, and the first one's are in [throughput/lokyr-disturbed/](compare-F2-134/throughput/lokyr-disturbed/).

## Latency

Typometer, built from its maintained fork, typed 200 keys into each terminal, which ran `cat > /dev/null`, so that the terminal's echo of each key was the line discipline's, and timed each from its key to the change of the window's pixels that it reads from the screen: alone, and with a second window of the same terminal beside it, flooded by `yes`. At 60 Hz on the main display, and at 120 Hz on the MacBook's own, through ProMotion; WezTerm and Terminal.app were not measured. The raw results are in [latency/](compare-F2-134/latency/). The median, the 90th and the 99th percentiles, by the nearest rank, in milliseconds; less is better:

| Terminal | 60 Hz, median | p90 | p99 | 120 Hz, median | p90 | p99 |
|---|---|---|---|---|---|---|
| **Lokyr** | 22.8 ms | 49.1 | 50.3 | **21.2 ms** | 25.2 | 32.2 |
| Ghostty | 33.1 ms | 50.0 | 64.7 | 24.8 ms | 41.9 | 47.7 |
| Alacritty | 21.2 ms | 29.4 | 47.9 | 21.6 ms | 23.8 | 25.8 |
| kitty | 32.0 ms | 48.3 | 50.5 | 21.8 ms | 24.1 | 29.6 |
| iTerm2 | 35.8 ms | 47.9 | 50.6 | 21.3 ms | 40.6 | 45.0 |

| Terminal, next to a flood | 60 Hz, median | p90 | p99 | 120 Hz, median | p90 | p99 |
|---|---|---|---|---|---|---|
| **Lokyr** | 19.9 ms | 34.0 | 43.2 | 16.7 ms | 20.1 | 24.3 |
| Ghostty | 32.3 ms | 40.8 | 43.8 | 21.0 ms | 28.8 | 34.1 |
| Alacritty | 16.1 ms | 26.5 | 38.5 | 16.2 ms | 18.5 | 20.8 |
| kitty | 16.5 ms | 28.6 | 37.5 | 16.3 ms | 19.0 | 21.9 |
| iTerm2 | 16.5 ms | 29.3 | 40.6 | 18.2 ms | 35.6 | 40.4 |

- **At 120 Hz**, Lokyr has the lowest median of the five, and its p99, 32.2 ms, comes third, after Alacritty's and kitty's: 24 of its 200 keys took more than three frames, 25 ms, where 5 of Alacritty's and 11 of kitty's did, 41 of iTerm2's and 94 of Ghostty's.
- **At 60 Hz, alone**, a third of Lokyr's keys took a frame more than Alacritty's: 69 of 200 past two frames, 33.3 ms, where 14 of Alacritty's did, and 98 of kitty's and of Ghostty's, 148 of iTerm2's; hence its p90 of 49.1 ms, against Alacritty's 29.4.
- **Next to a flood**, every terminal's keys came sooner, at both rates, likely as the window server, which composes the screen at each refresh for the flooded window, has no rest to wake from.
- **Lokyr's own measure** of [F2-54](latency-F2-54.md#after-f2-144), from the key's event to the frame's presentation, gave 21.2 ms at the median and 41.5 at p99 at 60 Hz, 14.5 and 34.2 at 120 Hz, on commit 7796b52. Typometer's medians of Lokyr are 1.6 ms longer at 60 Hz and 6.7 ms at 120 Hz: its keys go through the system's queue of events, and its end is its capture of the screen, a path that bounds every terminal alike, as four of the five medians at 120 Hz fall between 21.2 and 21.8 ms. The budget is that measure's, p50 ≤ 2 frames and p99 ≤ 4.5 at 120 Hz, which Lokyr keeps; by Typometer too, its p99 at 120 Hz is 3.9 frames.
- **Is It Snappy**, of a camera at 240 frames a second, needs an iPhone, which the user has not: not measured.

## Energy

`lokyr-bench energy`, which reads the energy that the kernel bills each process, `ri_energy_nj` of `proc_pid_rusage`, and needs no administrator, over `cat` of 1 GiB in each terminal, all of them in one session: the same bytes in each, whatever its rate. The kernel bills the CPU's energy alone: four seconds of a Metal loop that keeps the GPU busy add 0.02 J to a process's count, where a second of the CPU's work adds about 6 J, as the review after F2-148 measured. The figures leave out each terminal's own drawing on the GPU.

| Terminal | A GiB | At |
|---|---|---|
| **Lokyr** | **17.3 J** | 298.5 MiB/s |
| kitty | 41.1 J | 145.3 MiB/s |
| Alacritty | 41.9 J | 168.8 MiB/s |
| Ghostty | 43.1 J | 300.2 MiB/s |
| iTerm2 | 107.9 J | 102.8 MiB/s |
| Terminal.app | 124.3 J | 38.7 MiB/s |
| WezTerm | 196.3 J | 44.0 MiB/s |

- Lokyr's CPU spends 0.4 of Ghostty's energy at the same rate.
- The GPU's work and the window server's composition are each terminal's cost too, and these figures leave them out: `powermetrics`, which reads them, needs an administrator, and is the user's part.
- iTerm2's rate moved between sessions, from 10 to 103 MiB/s in a new window, and 51 in the window of the throughput's session.
- Ten seconds of `yes`, in the first session, are not comparable across terminals, whose rates differ: they are in [idle/](compare-F2-134/idle/).

### `powermetrics`

The user recorded `powermetrics`, which needs the administrator, in samples of about a second, of the tasks and of the CPU's and the GPU's power, while a script opened each terminal in turn, in the order of the tables, each running 15 s at rest, then `cat` of 256 MiB of the same lines of ASCII, then 3 s, before it closed and the next one opened 5 s later. [powermetrics/](compare-F2-134/powermetrics/) holds the figures that the report reads, which `python3 tools/bench/powermetrics_extract.py` takes out of the recording, and the times of the script's phases; `compare_tables.py` makes the tables.

At rest, the median of the samples from 5 s after each terminal's shell started to its `cat`, about ten. `powermetrics` stamps a sample at its end, which the figures take, as of the fix of the review at F2's end; they had taken the stamps for the samples' starts, a second off:

| Terminal, at rest | Its CPU | Its interrupt wakeups | Its energy impact | The window server's CPU | The GPU's power |
|---|---|---|---|---|---|
| **Lokyr** | 0.5 ms/s | 2.4 a second | 0.10 | 276.2 ms/s, not Lokyr's, below | 16 mW |
| Ghostty | 4.6 ms/s | 16.0 a second | 0.57 | 19.7 ms/s | 5 mW |
| Alacritty | 0.7 ms/s | 0.5 a second | 0.08 | 7.1 ms/s | 0 mW |
| kitty | 0.5 ms/s | 0.9 a second | 0.07 | 9.5 ms/s | 0 mW |
| WezTerm | 0.2 ms/s | 1.9 a second | 0.04 | 307.9 ms/s | 406 mW |
| iTerm2 | 16.1 ms/s | 7.5 a second | 3.64 | 22.9 ms/s | 4 mW |
| Terminal.app | 0.7 ms/s | 0.9 a second | 0.08 | 15.3 ms/s | 1 mW |

Under `cat` of 256 MiB, the sums over the whole samples that overlap its start to 2 s past its end, as a sample's rates are its whole length's, whatever part of it the `cat` took; the window server's and the GPU's past their rest's median, over those samples' length:

| Terminal, `cat` of 256 MiB | Its time | Its CPU | Its energy impact, over the seconds | The window server's CPU, past its rest | The GPU's energy, past its rest |
|---|---|---|---|---|---|
| **Lokyr** | 0.89 s | **1 006 ms** | **4 333** | — | ≤ 98 mJ |
| Ghostty | 0.88 s | 1 816 ms | 10 296 | 269 ms | 101 mJ |
| Alacritty | 1.68 s | 2 010 ms | 11 376 | 365 ms | 162 mJ |
| kitty | 1.77 s | 2 098 ms | 10 138 | 264 ms | 210 mJ |
| WezTerm | 5.51 s | 6 529 ms | 49 787 | 220 ms | 1 920 mJ |
| iTerm2 | 2.54 s | 5 199 ms | 27 486 | 218 ms | 136 mJ |
| Terminal.app | 7.09 s | 7 410 ms | 32 615 | 4 ms | 2 mJ |

- **Under output**, Lokyr's process took 0.55 of Ghostty's CPU and 0.42 of its energy impact, at the same time, as `ri_energy_nj` found above. Its GPU's energy, all of the samples' with its rest's, 98 mJ at most, is below every other terminal's past its rest, Ghostty's 101 mJ just above it, but Terminal.app's, which took 2 mJ past its rest.
- **At rest**, Lokyr's process is light, as every terminal's but Ghostty's and iTerm2's: of CPU, second with kitty, at 0.5 ms/s, after WezTerm's 0.2; of energy impact, fifth, at 0.10, against 0.04 to 0.08 of WezTerm, kitty, Alacritty and Terminal.app: medians of ten samples, of a few milliseconds of CPU each. The interrupt wakeups of these ten seconds, with the user at the Mac, differ from those of the 60 s of `lokyr-bench idle` above, of nobody at the Mac and of another counter, both ways: Lokyr's 2.4 a second here, 0.38 there, and kitty's 0.9 here, 20.08 there. `lokyr-bench`'s are the budget's.
- **The window server's 276 ms/s, while Lokyr was in front, are not Lokyr's at rest.** In the 40 s before Lokyr opened and in each gap between the terminals, the window server ran at about 200 to 300 ms/s, with peaks to 470, as the user's own Lokyr window, which ran the script, had the focus, its cursor fading and its background blurred, which costs that much, as below. While the test's Lokyr was in front, it stayed so, which the recording does not explain; Calendar, WindowManager and MenuBarAgent ran in its first samples too. The same bundle, measured again with `top`, left the window server as it is without any Lokyr window, below. Under `cat`, the window server's figure is left out, and the GPU's whole energy bounds Lokyr's.
- **WezTerm** keeps the window server composing at rest, at 308 ms/s, and the GPU at 406 mW, which this report did not look into.

### The window server and the cursor's fades

The window server's CPU, in a core's percent, by `top` every 2 s, as a Lokyr window of commit e070548 was in front, at rest at its prompt, 4 to 16 s after its start, the mean of the samples while the cursor blinked; the configs and every sample are in [window-server/top.json](compare-F2-134/window-server/top.json). `top`'s percent counts otherwise than `powermetrics`' milliseconds a second: the two do not compare.

| The window in front | Opaque | `background-opacity = 0.9`, `background-blur = 10` |
|---|---|---|
| None of Lokyr's | 9.2% | |
| Lokyr, a steady cursor | 9.1% | 8.7% |
| Lokyr, a steady cursor, in `lokyr --bench output`, as in the session | 9.9% | |
| Lokyr, a cursor switched on and off, the default | 10.0% | 10.0% |
| Lokyr, a cursor that fades, of the user's config: 600 ms of fades in a cycle of 1.2 s | 28.8% | 30.8% |

- **A cursor that fades costs the window server about a fifth of a core** while it blinks, 20 points over the rest, as the window server composes the window at each refresh of a fade; a cursor switched on and off, the default, costs nothing that `top` tells apart. The cost follows the share of the cycle that fades: 29.7%, 17.8% and 14.6% for 600, 200 and 40 ms of fades in 1.2 s.
- It lasts as long as the blinking: 15 s after the last key or output by default, `blink-timeout`, and in the window that has the focus alone. A second Lokyr in front, started by a script of the first one's shell, as in the session, took the window server back to 5 to 10% within about 5 s, and the first window's fades took it to 31% again once it had the focus back.
- [CONFIGURATION.md](../CONFIGURATION.md#smooth-blink) tells this cost.

## Unicode

`ucs-detect` 2.3.8, of Unicode 18, in each terminal: the share of the sequences of each kind that the terminal draws as wide as `wcwidth` 0.9.4 measures them.

| Terminal | Wide | Emoji ZWJ | VS16 | VS15 | Narrow | Flags | A lone regional indicator | Languages, all right | Their clusters |
|---|---|---|---|---|---|---|---|---|---|
| **Lokyr** | 94.9% | 100% | 100% | 100% | 100% | 100% | **0%** | 57 of 85 | **43.9%** of 6 589 |
| Ghostty | 99.8% | 100% | 100% | 100% | 99.9% | 100% | 100% | 83 of 85 | 99.5% of 6 589 |
| Alacritty | 94.9% | 0.7% | 50.0% | 0% | 99.6% | 100% | 0% | 63 of 85 | 84.4% of 6 589 |
| kitty | 100% | 100% | 97.9% | 100% | 99.6% | 100% | 100% | 60 of 85 | 18.6% of 3 788 |
| WezTerm | 54.9% | 100% | 50.0% | 0% | 99.7% | 100% | 0% | 61 of 85 | 53.7% of 2 175 |
| iTerm2 | 99.8% | 100% | 100% | 0% | 100% | 100% | 100% | 74 of 85 | 76.6% of 474 |
| Terminal.app | 94.9% | 0% | 50.0% | 0% | 98.3% | 98.9% | 0% | 62 of 85 | 81.0% of 6 589 |

- **Lokyr's emoji are all right**, as Ghostty's.
- **A lone regional indicator**, as 🇦 without a second one to make a flag, takes one cell in Lokyr, as Markus Kuhn's `wcwidth`, Alacritty, WezTerm and Terminal.app give it, and two in Ghostty, kitty, iTerm2 and `wcwidth` 0.9.4; the 26 of them are all of Lokyr's errors of wide characters too.
- **The languages**: a consonant with a vowel sign that spaces, as Bengali's ক with া, which `wcwidth` 0.9.4 and Ghostty count as two cells, as a classic `wcwidth` gives one to each of its two codepoints, takes the width of its base in Lokyr, one cell. A shell or an editor that counts with the system's `wcwidth` puts its cursor a cell further than Lokyr shows it, in Hindi, Bengali, Tamil, Malayalam and the other scripts of South and Southeast Asia.
- [F2-153](../tasks/F2-macos-single-pane.md#f2-153--widths-as-the-programs-count-them) gave the clusters that a vowel sign that spaces or a virama widens two cells, a lone regional indicator two, and an Arabic number sign with its digit two: Lokyr's languages are now 83 of 85 all right, and 99.5% of their clusters, 6 557 of 6 589, Ghostty's figures, its wide characters 100% and its lone regional indicators 100%; [unicode/after-F2-153/](compare-F2-134/unicode/after-F2-153/) has its report. The 32 clusters left, of Javanese and of Grantha, are a consonant and a virama that spaces, which `wcwidth` 0.9.4 counts as one cell, and Ghostty and Lokyr as two, as a `wcwidth` that adds its codepoints up does.
- ucs-detect stops each kind after 10 s: kitty, WezTerm and iTerm2 measured fewer clusters of the languages. In iTerm2, which leaves one of its queries of the terminal's features unanswered, it ran without them.

## vttest and esctest

The user went through vttest's menus 1, of the cursor's movements, 2, of the screen's features, and 8, of the VT102's insertion and deletion, in each terminal, at 120 by 40 cells, and rated each from 1 to 5:

| Terminal | 1, cursor movements | 2, screen features | 8, insertion and deletion |
|---|---|---|---|
| **Lokyr** | 4 | **5** | 5 |
| Ghostty | 4 | 4 | 5 |
| Alacritty | 4 | 4 | 5 |
| kitty | 3 | 4 | 5 |
| iTerm2 | 3 | 3 | 5 |

- **Menu 1's screens** of Lokyr fall out of place for the size, not for a fault: vttest draws its frames for 80 columns and then for 132, switched by DECCOLM, and counts on DECCOLM to erase the screen before its test of the autowrap. Lokyr ignores DECCOLM, as most terminals do by default, so that at 120 columns the frame of 80 stops short of the right edge and its bottom line runs past it, the one of 132 loses its last 12 columns, and the autowrap's letters draw over the frame before. Its tests of control characters and of leading zeros in sequences came right.
- **esctest**, at the commit that `just esctest` pins, in Lokyr's headless terminal: 496 of 533 applicable tests pass, 93.1%, as after [F1-46](esctest-F1-46.md), with 34 known bugs of xterm left out.

## Frame pacing

The HUD's log, `MTL_HUD_ENABLED` and `MTL_HUD_LOG_ENABLED`, over 10 s of `yes`, gave nothing comparable: only Lokyr and iTerm2 logged, their fields private but three numbers, which Apple does not document; and iTerm2 crashed in the HUD's own code, `-[HUDMTLLayerTracking safeAreaInsets]`, twice. The frames' pacing needs another tool: the counter of a window's frames through ScreenCaptureKit that the task's notes propose, or Instruments.

By eye, the user saw no difference between the terminals as each scrolled lines that came at a steady 60 a second.

## What this leaves

- **The gaps past the budget of throughput**, each a task, as the user chose on 31 October 2026: kitty's short CSI sequences, at 0.26 of Ghostty's rate, [F2-150](../tasks/F2-macos-single-pane.md#f2-150--short-csi-sequences-faster); its unique clusters of several codepoints, at 0.72, [F2-151](../tasks/F2-macos-single-pane.md#f2-151--unique-clusters-of-several-codepoints-faster); vtebench's `scrolling_top_region`, a sixth slower than Ghostty, [F2-152](../tasks/F2-macos-single-pane.md#f2-152--vtebenchs-top-region).
- **The widths**: a lone regional indicator, and a cluster with a vowel sign that spaces, as `wcwidth` 0.9.4 and Ghostty count them, as the user chose: [F2-153](../tasks/F2-macos-single-pane.md#f2-153--widths-as-the-programs-count-them).
- **The latency at 60 Hz**: a third of Lokyr's keys a frame later than Alacritty's, alone, within the budget, which counts frames, not the others' figures: a known issue of F2.
- **A cursor that fades** costs the window server about a fifth of a core while it blinks, which the guide now tells; the default, which switches it, costs nothing measurable.
- **Is It Snappy**, which needs an iPhone, and WezTerm's and Terminal.app's latency were not measured.
