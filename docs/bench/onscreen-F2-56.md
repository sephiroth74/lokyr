# CPU per frame and throughput on screen (F2-56)

Measured on 2026-10-31, on the bundle of the commit after acf9679, against the [budgets](../PERFORMANCE.md#budgets): a frame's time on the CPU, p99 ≤ 2 ms at 4K, and the throughput within 10% of the faster of Ghostty and Alacritty, but for [ADR 0015](../adr/0015-vt-core-throughput-exception.md)'s exception. The raw results, one line a run, and each terminal's config are in [onscreen-F2-56/](onscreen-F2-56/).

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

## After F2-145

Measured on 2026-10-31, on the bundle of the commit after ce85fd6, as above, of Lokyr, Ghostty and Alacritty alone, which the criterion names: the raw results are in [after-F2-145.jsonl](onscreen-F2-56/after-F2-145.jsonl).

| Workload | Lokyr | Lokyr before | Ghostty | Alacritty | Against the faster |
|---|---|---|---|---|---|
| `ascii.txt`, 1 GiB | **293.1** | 117.8 | 301.7 | 167.3 | −2.9% |
| `seq.txt` | **183.0** | 117.3 | 176.7 | 83.4 | +3.6% |
| `sgr.txt` | **245.4** | 138.4 | 249.8 | 149.8 | −1.8% |
| `cjk.txt` | **143.6** | 112.7 | 174.8 | 152.7 | −17.8% |

- **The cause**: one thread read the PTY and parsed what it read, in turn. A macOS terminal queues 1 KiB of output, which every read takes, 98.5% of them 1024 bytes exactly: the program waited, each KiB, for the parse, and the reader for the program's refill, two wakeups a KiB. A reader of `cat`'s output that does nothing else takes 154 MiB/s as Lokyr read, a `poll` then a `read`; 174 reading first, and polling once the queue is empty; 182 with a blocking read; 313 with reads that never wait. With 1 to 3 µs of work a KiB between its reads, as a parse takes, each of the first three takes 134 to 176 MiB/s. The reader's thread at the user-interactive QoS changed nothing, 2.16 s for 256 MiB either way. Ghostty, as `sample` shows, reads on a thread and parses on another.
- **The fix**: a thread of the PTY's reads into a ring of four buffers of 64 KiB, ahead of the I/O thread's parse, and bridges the refills of a stream that fills the queue, as [ARCHITECTURE.md](../ARCHITECTURE.md#threads-and-data-flow) tells: ASCII, `seq` and SGR come within 3% of Ghostty, `seq` ahead.
- **CJK is the parse's now**: the I/O thread parses in 98% of the samples, and the reader waits for a free buffer in 54%. The VT core alone, as `lokyr-throughput` feeds it, parses `cjk.txt` at 156.7 MiB/s, where `alacritty_terminal` parses it at 280, 0.56 of it, though vtebench's `unicode` scenario, without emoji of joiners, skin tones and variation selectors, keeps 1.04: the clusters take `print_chars`, the segmenter, `write_stretch` and `join_previous`, and an allocation each, `malloc` and `free` about a seventh of the core's samples.
- **What it costs**: a thread more for each pane; four buffers where there was one, of which only the pages written are resident, all 256 KiB once a pane has had a flood; on a stream that fills the queue, up to 16 reads that do not wait for each refill, about 8 µs of CPU, where an idle or interactive pane never spins. A key's echo, which goes through both threads, takes as long: at 60 Hz, the median of 200 synthetic keys of `lokyr --bench latency`, 21.1 and 21.6 ms, against 22.6 and 20.8 at ce85fd6, in turn.
- **A frame's time**, at 3840 × 2224 pixels, of the three runs: 548 frames of ASCII, p50 0.28 ms, p99 1.52 at most; 75 of `seq`, 0.27 and 2.30; 91 of SGR, 0.55 and 5.68; 142 of CJK, 0.57 and 17.41, whose first frames of new glyphs [F2-146](../tasks/F2-macos-single-pane.md#f2-146--a-frames-new-glyphs) takes.
- **The headless throughput of F1** holds: the VT core did not change.

## After F2-147

On screen, on the bundle of bc7c256, as above, of Lokyr, Ghostty and Alacritty; the raw results are in [after-F2-147.jsonl](onscreen-F2-56/after-F2-147.jsonl). The median of three runs, in MiB/s:

| Workload | Lokyr | After F2-145 | Ghostty | Alacritty | Against the faster |
|---|---|---|---|---|---|
| `ascii.txt`, 1 GiB | 290.5 | 293.1 | 295.4 | 167.2 | −1.7% |
| `seq.txt` | 180.0 | 183.0 | 171.8 | 82.8 | +4.8% |
| `sgr.txt` | 240.5 | 245.4 | 249.9 | 150.9 | −3.8% |
| `cjk.txt` | **185.5** | 143.6 | 168.4 | 152.6 | +10.2% |

- **CJK is ahead of Ghostty** by a tenth, where it was 17.8% behind; the other workloads stay within 4% of it, as after F2-145, whose path this does not take.
- **A frame's time**, at 3840 × 2224 pixels, of the three runs: 564 frames of ASCII, p50 0.28 ms, p99 1.50 at most; 74 of `seq`, 0.29 and 2.05; 89 of SGR, 0.52 and 6.62; 117 of CJK, 0.48 and 21.12, whose first frames of new glyphs [F2-146](../tasks/F2-macos-single-pane.md#f2-146--a-frames-new-glyphs) takes.
- **A first round**, with the display asleep and the screen locked, drew two frames a run, and was left out: a window that does not show draws nothing, and its terminal reads faster.

Measured on 2026-10-31, on the commit after ab12125, against it, of the VT core alone: `lokyr-throughput`, and a harness that feeds a file to the terminal in reads of 64 KiB at 120 × 40, its line feeds after carriage returns as a terminal's ONLCR puts them, the median of five runs.

| Input | Before | After | |
|---|---|---|---|
| `cjk.txt` | 163.5 MiB/s | **214.3** | +31% |
| Its CJK alone | 289 | 322 | +11% |
| Its emoji of one codepoint | 302 | 335 | +11% |
| Its ZWJ sequences and skin tones | 119 | 215.5 | +81% |
| ❤️ and ✔️, VS16 after a narrow pictographic | 89 | 112 | +25% |
| Its flags | 111 | 144 | +29% |
| Letters of combining accents | 57 | 77 | +34% |

- **The cause**: the codepoints that join a cluster, a ZWJ, a skin tone, a variation selector, the second of a flag's indicators, took 38% of the time of `cjk.txt`, which takes 0.761 s with them and 0.473 s without, as many lines: each ended the stretch of characters that the terminal writes at once, looked its properties, its width and its segmentation up again, and found the cluster's row in its page five to seven times. The allocations that a first sample of `lokyr-throughput` showed were `alacritty_terminal`'s, which runs in the same process.
- **The fix**: a codepoint that joins a cluster of the stretch and leaves it as wide stays in the stretch, and goes into the page's grapheme table once the stretch is written; the stretch's buffers are the terminal's own, where each run filled 512 bytes of them anew; a join that changes a cluster's width finds its row once, and a widening too.
- **Against `alacritty_terminal`**, as `lokyr-throughput` feeds both: `cjk.txt` from 0.58 of its rate to 0.75, 203.0 MiB/s against 271.2. vtebench's scenarios, twice each, keep their rates: `unicode` 408.6 and 406.1 MiB/s against 410.3 and 410.3 before, 1.03 and 1.02 of `alacritty_terminal`'s; the bottom region 0.71 and 0.68, above ADR 0015's 0.6.
- **The same states**: the terminal, fed whole and a byte at a time, holds the same cells, clusters, styles and cursor after each of 200 000 generated inputs, of emoji, joiners, selectors, flags, margins, insert mode and wraps, and each of the 83 004 of the fuzzers' corpora; and the same as before the change, after each of 182 851 of them. The fuzzers ran ten minutes each, `terminal` and `differential`, without a failure.
