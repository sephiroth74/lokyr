# A frame's new glyphs

Measured on 2026-10-31, on the commits before and after the change, c56eeda and ba9fabc, against the [budget](../PERFORMANCE.md#budgets) of a frame's time on the CPU, p99 ≤ 2 ms at 4K; and, once the change was done, every earlier measure of F2 again, on the bundle of ba9fabc and on that of 105dc42, before the output's way from the PTY, the core's print of clusters and this change. The raw results of that session are in [glyphs-F2-146/](glyphs-F2-146/).

## Method

- **A frame's layout**, outside the app: the font of the benches' config, Menlo at 13 points at a scale of 2, lays out a screen of 245 × 67 cells as a frame does, with the CoreText backend's calls timed: the faces' lookups, the shaping and the rasterization.
- **On screen**: `lokyr --bench output --window 1920x1080` of a program that writes a file a screen at a time, 0.1 s apart: `cjk-all.txt`, the 32 164 CJK ideographs and Hangul syllables, 120 a line; `emoji-all.txt`, 1 178 emoji, 100 a line; `page.txt`, 60 lines of 110 characters of 600 ideographs, as a page of Chinese. The four workloads of [onscreen-F2-56](onscreen-F2-56.md), with `onscreen.py`, three runs each.
- **A screen's fill**: from the first frame that leaves glyphs for the frames after to the first that leaves none, as a build of the app that logs each frame told it.

## Where a frame of new glyphs went

Before, in the layout outside the app:

| A first frame of | Its time | The faces' lookups | Shaping | Rasterization |
|---|---|---|---|---|
| `cjk.txt` | 14 to 20 ms | 37, 240 to 360 µs each | 6, 100 to 220 µs each | 37 glyphs, 3.9 to 5.3 ms |
| `sgr.txt` | 4.3 ms | | | 188 glyphs, regular and bold, 20 µs each |
| A screen of `cjk-all.txt` | 300 to 325 ms | 8 040, 2.3 µs each | | 8 040 glyphs, 37 µs each |
| `emoji-all.txt` | 194 to 198 ms | 1 178, 113 µs each | | 1 061 glyphs, 60 µs each |

On screen, a frame of a screen of new CJK took 160 to 197 ms, and of the emoji 276 ms.

- **The system's fallback**: a process's first lookups take 1.55 ms each, as CoreText loads its cascade of fonts and their faces; 1.5 µs after; an emoji 113 µs each, always; a pictographic of text presentation, as 🌡, 544 µs.
- **The rasterization is CoreText's**: about 20 µs a glyph of Menlo, 37 of CJK, 60 of an emoji. The font's bounding box, with the bitmap trimmed after, in place of each glyph's bounds, drew no faster: the drawing takes the outline that the bounds parsed.
- **On several threads**, 1 200 new CJK glyphs take 24 ms on one, 7.5 to 8 on four, 6 on twelve: CoreGraphics draws a glyph's rows on threads of its own already. Three threads start and end in 43 µs.
- **Readied ahead**, on another thread, the fallback and the faces of Chinese, Japanese, Korean and emoji take 17 to 20 ms, after which `cjk.txt`'s first frame takes 2.0 ms; they add 4.3 MB to the footprint, 3.6 of which the fallback's first look.

## What a frame does now

- The grid's layout collects the glyphs that the atlas lacks; CoreText draws them together, on the main thread and three others, for 2 ms at most, and the layout is done again with them. The faces of the characters that no frame met are looked up for 2 ms at most, in the frame's own time; what is left shows as empty cells, which the frames after, which the frame asks for, draw.
- After the first frame, a thread readies the fallback and its CJK and emoji faces.
- An emoji takes the emoji face found before, when it has the character, rather than the cascade's 113 µs.

The time that a frame gives to new glyphs, chosen on 31 October 2026 between these, measured with a build that took them from the environment:

| Lookups + drawing, a frame | A page of new Chinese shows whole in | Its frames | A screen of all-new CJK |
|---|---|---|---|
| 0.3 + 0.3 ms | 0.94 s, 58 frames | p50 1.43 ms, the longest 3.07 | several seconds |
| 1 + 1 ms | 160 to 180 ms, 9 or 10 frames | p50 2.0 ms | about 0.5 s |
| **2 + 2 ms** | **50 to 75 ms, 3 frames** | **p50 3.1 to 5.4 ms, the longest 5.4 to 8.7** | **about 0.22 s** |

Before, the page took one frame of about 25 ms, and a 4K screen of all-new CJK one of 300 ms. The longest frames, up to 23 ms, came of the first frame of CJK, about 50 ms after the app's start, which waited for the thread that readied the fallback; text that comes later finds it ready. [PERFORMANCE.md](../PERFORMANCE.md#budgets) has the budget chosen with it: a frame that draws new glyphs, ≤ 10 ms.

## The four workloads

On screen, the bundle of ba9fabc against 105dc42's, three runs of each; a frame's time, at 3840 × 2224 pixels:

| Workload | MiB/s | Frames a run | p50 | p99, by run | p99 before, by run |
|---|---|---|---|---|---|
| `ascii.txt` | 301.6, from 117.5 | 185 to 192, from 469 to 482 | 0.28 ms | 1.84, 1.44, 1.41 | 0.32, 0.33, 0.34 |
| `seq.txt` | 177.8, from 118.0 | 24 to 26 | 0.29 ms | 1.45, 1.86, 1.71 | 1.95, 1.61, 1.85 |
| `sgr.txt` | 239.5, from 140.4 | 28 to 30 | 0.51 ms | 4.01, 2.68, 2.38 | 5.78, 5.57, 5.05 |
| `cjk.txt` | 183.6, from 110.6 | 36 to 41 | 0.49 ms | 1.96, 2.66, 2.70 | 16.25, 15.85, 15.34 |

- **CJK's and SGR's first frames** of new glyphs take 2 to 4 ms, from 15 to 16 and 5 to 6: within the budget of a frame of new glyphs.
- **ASCII's p99** is its first frames now, whose glyphs are new: with three times the throughput, a run has 190 frames where it had 470, and its first frames, of 2.2 to 2.8 ms before too, no longer fall below the 99th percentile. Its p90 is 0.34 ms, from 0.27.
- **CJK's median frame**, 0.49 ms from 0.29, waits longer for the grid's lock: the session parses the output in slices of up to 64 KiB since the output's change, where it read a KiB at a time, and a slice of CJK takes about 0.3 ms to parse.

## Against the earlier measures

Every measure of F2 again, in one session, on the bundles of ba9fabc and 105dc42 in turn, on AC power, at 60 Hz, with nobody at the Mac:

| Measure | ba9fabc | 105dc42 | Earlier |
|---|---|---|---|
| Warm startup, the first frame, median of 10 runs, twice | 273.9 and 265.1 ms | 274.8 and 273.3 ms | [startup-F2-53](startup-F2-53.md#after-the-drawing-made-beside-appkits-launch): the minimal AppKit app's first window plus 30 to 33 ms; today 257.2 ms, plus 8 to 17 |
| A key's echo, 200 synthetic keys, twice: p50, p99 | 21.2 and 22.5 ms; 38.8 and 44.3 | 22.6 and 23.8 ms; 39.6 and 36.4 | [latency-F2-54](latency-F2-54.md#after-the-keys-frames-came-sooner), at 60 Hz: 21.2 ms at the median |
| Idle, steady cursor: package idle and interrupt wakeups a second | 0, 0.38 | 0, 0.38 | [idle-memory-F2-55](idle-memory-F2-55.md): 0, 0.35 |
| Footprint without the GPU, about 80 s after the start | 24.8 MB | 23.7 MB | [idle-memory-F2-55](idle-memory-F2-55.md): 24.0 MB three minutes after the start; budget 35 |
| The VT core, `lokyr-throughput`, vtebench's scenarios | equal or faster in each; `unicode` 1.02 of `alacritty_terminal`'s rate, the bottom region 0.69 | `unicode` 1.03, the bottom region 0.70 | [throughput-F1-133](throughput-F1-133.md) |
| The core, `cjk.txt` | 204.7 MiB/s, 0.75 | 154.4 MiB/s, 0.59 | [onscreen-F2-56](onscreen-F2-56.md#after-the-cores-print-of-clusters) |

- **No measure fell back** but the steady frames of output, which wait a little longer for the grid's lock, as above.
- **The footprint** grew by 1.1 MB, not the 4.3 that the fallback took in the layout outside the app: the shell's prompt, whose symbols the configured font lacks, looks the fallback up in both.
