# Throughput on the code that F1 ends with (F1-56)

Measured on 2026-10-25 at about 08:50, with `bolt-throughput` alone, on the code of 29048e6, whose VT core is 7959169's, on the machine of [the F0 baseline](baseline-F0.md): an Apple M5 Pro, macOS 27.0.1, on AC power, after the fuzzing of the 24 hours had ended, with no build running. Rust 1.99.0, release builds. [F1-133's figures](throughput-F1-133.md) were measured on 840cee6, before F1-140 and F1-147 changed how a wrap from the screen's last column treats the rows' soft-wrap marks, on the path of a wide character that does not fit there.

## Method

The same as [F1-51's](throughput-F1-51.md#method) for both cores: the same bytes fed to each in reads of 64 KiB, at 120 by 40, the median of 5 runs taken in turn; vtebench's scenarios at commit `ead8003`; and `cat` of 1 GiB. The criterion benchmarks were not run again: reflow's code has not changed since F1-133's measure.

## Both cores

| Scenario | Bytes timed | VT core | `alacritty_terminal` | Ratio | Ratio in F1-133 |
|---|---|---|---|---|---|
| cursor_motion | 8.5 MiB | 472.0 MiB/s | 380.4 MiB/s | 1.24 | 1.24 |
| dense_cells | 25.7 MiB | 545.3 MiB/s | 432.8 MiB/s | 1.26 | 1.36 |
| light_cells | 8.6 MiB | 4640.7 MiB/s | 335.9 MiB/s | 13.81 | 14.00 |
| medium_cells | 8.2 MiB | 344.5 MiB/s | 279.0 MiB/s | 1.23 | 1.28 |
| scrolling | 12.0 MiB | 160.4 MiB/s | 96.2 MiB/s | 1.67 | 1.75 |
| scrolling_bottom_region | 12.0 MiB | 160.5 MiB/s | 236.5 MiB/s | 0.68 | 0.67 |
| scrolling_bottom_small_region | 12.0 MiB | 111.2 MiB/s | 78.5 MiB/s | 1.42 | 1.43 |
| scrolling_fullscreen | 8.1 MiB | 4031.6 MiB/s | 311.4 MiB/s | 12.95 | 12.18 |
| scrolling_top_region | 12.0 MiB | 150.0 MiB/s | 45.4 MiB/s | 3.30 | 3.36 |
| scrolling_top_small_region | 12.0 MiB | 102.8 MiB/s | 81.7 MiB/s | 1.26 | 1.30 |
| sync_medium_cells | 8.5 MiB | 360.0 MiB/s | 297.8 MiB/s | 1.21 | 1.22 |
| unicode | 8.4 MiB | 413.7 MiB/s | 397.0 MiB/s | 1.04 | 1.04 |
| `cat` of 1 GiB | 1024 MiB | 981.0 MiB/s | 225.0 MiB/s | 4.36 | 4.81 |

Every scenario keeps at least `alacritty_terminal`'s rate but the bottom region, at 0.68 of it, above the 0.6 of [ADR 0015](../adr/0015-vt-core-throughput-exception.md): F1's throughput criterion holds on its final code. The VT core's own rates are F1-133's within about 6%, but scrolling_fullscreen's, 23% higher, and `cat`'s, 9% higher; the ratios that moved most, dense_cells and `cat`, did so with `alacritty_terminal`'s rate, 6% and 20% higher than in F1-133's run, as the machine's load varies between runs.
