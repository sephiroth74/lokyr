# Throughput after the performance fixes

Measured on 2026-10-23, from 00:54 to 00:59, with `just bench` on the code of 1124d46, on the machine of [the F0 baseline](baseline-F0.md): an Apple M5 Pro, macOS 27.0.1, on AC power, at night, with no sleep during the run and the fuzzing started after it. Rust 1.99.0, release builds, `alacritty_terminal` 0.26.0.

## Method

The same as [throughput-F1-50's](throughput-F1-50.md#method): both cores fed the same bytes in reads of 64 KiB by `bolt-throughput`, at 120 by 40, the median of 5 runs taken in turn; vtebench's scenarios at commit `ead8003`; `cat` of 1 GiB; and criterion for the VT core alone.

## Both cores

| Scenario | Bytes timed | VT core | `alacritty_terminal` | Ratio | Ratio before the fixes |
|---|---|---|---|---|---|
| cursor_motion | 8.5 MiB | 462.2 MiB/s | 378.8 MiB/s | 1.22 | 0.83 |
| dense_cells | 25.7 MiB | 536.3 MiB/s | 416.6 MiB/s | 1.29 | 0.94 |
| light_cells | 8.6 MiB | 4526.5 MiB/s | 319.7 MiB/s | 14.16 | 3.26 |
| medium_cells | 8.2 MiB | 337.9 MiB/s | 268.8 MiB/s | 1.26 | 0.74 |
| scrolling | 12.0 MiB | 168.7 MiB/s | 96.9 MiB/s | 1.74 | 0.95 |
| scrolling_bottom_region | 12.0 MiB | 157.0 MiB/s | 229.2 MiB/s | 0.68 | 0.04 |
| scrolling_bottom_small_region | 12.0 MiB | 109.1 MiB/s | 76.5 MiB/s | 1.43 | 0.17 |
| scrolling_fullscreen | 8.1 MiB | 3851.2 MiB/s | 297.4 MiB/s | 12.95 | 3.53 |
| scrolling_top_region | 12.0 MiB | 146.5 MiB/s | 45.1 MiB/s | 3.25 | 0.21 |
| scrolling_top_small_region | 12.0 MiB | 101.3 MiB/s | 79.7 MiB/s | 1.27 | 0.17 |
| sync_medium_cells | 8.5 MiB | 344.8 MiB/s | 283.2 MiB/s | 1.22 | 0.73 |
| unicode | 8.4 MiB | 400.5 MiB/s | 387.2 MiB/s | 1.03 | 0.39 |
| `cat` of 1 GiB | 1024 MiB | 940.2 MiB/s | 214.7 MiB/s | 4.38 | 3.33 |

`bolt-replay --bench` replays the same file at 959.5 MiB/s, the median of 5 runs, against 489.5 MiB/s before the fixes.

## The VT core alone

| Benchmark | Throughput | Before the fixes |
|---|---|---|
| parser, plain ASCII | 12.8 GiB/s | 12.6 GiB/s |
| parser, colored output | 1.15 GiB/s | 887 MiB/s |
| parser, Japanese text with emoji | 1.03 GiB/s | 738 MiB/s |
| parser, the cursor moved for every character | 1.03 GiB/s | 542 MiB/s |
| terminal, plain ASCII | 3.18 GiB/s | 1.00 GiB/s |
| terminal, colored output | 361 MiB/s | 313 MiB/s |
| terminal, Japanese text with emoji | 222 MiB/s | 138 MiB/s |
| terminal, the cursor moved for every character | 476 MiB/s | 305 MiB/s |
| grid, scrolling at the scrollback's limit | 211.7 M rows/s | 72.2 M rows/s |
| grid, writing a row of 120 columns and scrolling | 39.7 M rows/s | 10.6 M rows/s |
| reflow, 120 to 80 columns, 1 000 to 50 000 lines | 3.13 to 3.45 M rows/s | 828 000 to 850 000 rows/s |

## Against the budget

The budget is at least `alacritty_terminal`'s throughput in every scenario, but the bottom region, for which the criterion asks for 0.6 of it at least. Every scenario passes: the twelve others at 1.03 to 14.16, the bottom region at 0.68. `unicode` passes by the least, 3%.

Taken at rest, the ratios are above those of the measures taken while the fuzzing ran, as the fixes and the bottom scroll region's own work were made: the bottom region 0.68 against 0.65, and 0.66 after that work, `cursor_motion` 1.22 against 1.07 to 1.18.

## The search

The same run measured the search, whose report is [search-F1-54](search-F1-54.md). It was 14% to 18% slower than there: 31.1 ms over 100 000 lines against 27.2, 2.96 ms over a line of 1 000 000 cells against 2.5. The loop over a row's cells counts their columns with `(0..=u16::MAX).zip(…)`, so that a row of 65 535 columns does not overflow the count, and the inclusive range does not optimize as the open one did: with an open counter of `u32`, the search over 100 000 lines takes 25.9 ms, 16.7% less than the 31.1 above. A later change takes it, and the same loop in three other places.
