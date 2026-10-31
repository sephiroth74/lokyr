# CPU per frame and throughput on screen (F2-56)

Measured on 2026-10-31, on the bundle of the commit after 8a5b5f4, against the [budgets](../PERFORMANCE.md#budgets): a frame's time on the CPU, p99 ≤ 2 ms at 4K, and the throughput within 10% of the faster of Ghostty and Alacritty, but for [ADR 0015](../adr/0015-vt-core-throughput-exception.md)'s exception. The raw results, one line a run, and each terminal's config are in [onscreen-F2-56/](onscreen-F2-56/).

## Method

- **The workloads**, which `tools/bench/workloads.py` writes, the same bytes each time: `ascii.txt`, 1 GiB of lines of ASCII text; `seq.txt`, `seq 1 10000000`'s output; `sgr.txt`, 128 MiB of lines whose every character changes its colors, of the 256's and of 24-bit ones, and bold; `cjk.txt`, 128 MiB of lines of CJK and of emoji, with their variation selectors, joiners and skin tones.
- **The throughput**: `tools/bench/onscreen.py`, run in each terminal, times `cat` of a workload and a query of the cursor's position after it, whose answer the terminal gives once it has parsed the whole output, as kitty's benchmark ends its tests, where `time cat` ends as the PTY takes the last bytes. `tools/bench/onscreen_compare.py` runs each workload in each terminal, in a window of its own, three times, the terminals' order turning from one workload to the next.
- **A frame's time**: `lokyr --bench output --window 1920x1080 -- <program>` runs the program in a window of 1920 × 1080 points, 3840 × 2224 pixels with its titlebar, and times each frame on the CPU by its phases, the screen's copy, the cells' layout, the rest of the display list, and the encoding and commit, but the wait for a drawable; it prints them as the program ends.
- **The same conditions**, as [PERFORMANCE.md](../PERFORMANCE.md#comparing-with-other-terminals) asks: Menlo at 13 points, ligatures off, no blur nor translucency, a window of about 1920 × 1080 points, 245 × 67 cells where a terminal takes cells, release builds, every terminal's defaults otherwise, with its own vsync and delays.

## Machine and versions

- Apple M5 Pro, macOS 27.2, on AC power, with Microsoft Defender scanning; a Samsung U28E590 at 60 Hz, 6016 × 3384 pixels as 3008 × 1692 points.
- Lokyr at this commit; Ghostty 1.3.2-main+62fa23bfa; Alacritty 0.18.0-dev (bbb2cbf), built from source, as Homebrew's cask is disabled; kitty 0.49.2, from Homebrew; iTerm2 3.7.6.

## Throughput

The median of three runs, in MiB/s:

| Workload | Lokyr | Ghostty | Alacritty | kitty | iTerm2 |
|---|---|---|---|---|---|
| `ascii.txt`, 1 GiB | 117.8 | **301.4** | 168.1 | 107.5 | 105.8 |
| `seq.txt` | 117.3 | **171.5** | 84.4 | 18.7 | 86.2 |
| `sgr.txt` | 138.4 | **252.7** | 150.5 | none | 26.7 |
| `cjk.txt` | 112.7 | **175.6** | 150.6 | 113.6 | 8.1 |

- **The runs agree** within 5% each, but Ghostty's second of `seq`, 10% slower, and iTerm2's first of `sgr`, 11.3.
- **Lokyr is behind Ghostty** by 61% in ASCII, 32% in `seq`, 45% in SGR and 36% in CJK, past the 10%; it is ahead of kitty and iTerm2 but in CJK, where it ties kitty.
- **kitty gave no result of `sgr.txt`** in 300 s, three times: its answer to the query did not come. **iTerm2** gave two runs of ASCII and of `seq` of three, its window opened by AppleScript.
- **A first round**, which `onscreen.py` ran with the terminal raw, its output too, so that each line feed went down without going back to the line's start, was left out: the lines stepped across the screen, as no `cat` shows them.

## A frame's time on the CPU

Lokyr's frames as each workload's output draws, in ms, at 3840 × 2224 pixels: the frames of the three runs, the runs' median of each percentile, and the largest p99 and longest frame:

| Workload | Frames | p50 | p90 | p99 | Longest | The p99's largest phase |
|---|---|---|---|---|---|---|
| `ascii.txt` | 1317 | 0.24 | 0.27 | 0.38 | 2.74 | the layout, 0.26 |
| `seq.txt` | 116 | 0.24 | 0.28 | 1.95 | 1.95 | the layout, 1.50 |
| `sgr.txt` | 157 | 0.44 | 0.57 | 5.47 | 5.47 | the layout, 4.80 |
| `cjk.txt` | 193 | 0.29 | 0.48 | 18.75 | 18.75 | the layout, 18.26 |

- **A frame of the steady output** takes a quarter of a millisecond to half a millisecond, a tenth of the budget, at 4K.
- **The first frames of glyphs that the atlas lacks** take 2 to 19 ms, all in the layout, which shapes and rasterizes them: hundreds of CJK characters and emoji, or bold faces. With a few dozen frames a run, the p99 is these frames, past the budget in SGR and CJK.

## Where the throughput goes

`sample` of Lokyr as `cat` writes `ascii.txt`, over 3 s: the session's reader waits in `poll` 72% of the time, reads 18%, and parses 9%. The parser waits for the bytes, as each piece of the output goes through the PTY's small buffer, a `poll`, a `read` and a wake of `cat` each: the core is not what holds the throughput, which Ghostty reaches at 330 MiB/s through the same PTY.

## The gaps

- **The throughput**: [F2-145](../tasks/F2-macos-single-pane.md#f2-145--the-outputs-way-from-the-pty) takes the reader's loop, its reads' size and count, the `poll` before each, its thread's priority, and the lock's handovers.
- **A frame's new glyphs**: [F2-146](../tasks/F2-macos-single-pane.md#f2-146--a-frames-new-glyphs) takes the first frames of glyphs that the atlas lacks.
- **The instrumentation**: the phases are timed with the app's own clock in `--bench output`; `tracing` spans and the signposts that Instruments shows wait for the overlay's and IPC's measures, as no subscriber reads them yet.
