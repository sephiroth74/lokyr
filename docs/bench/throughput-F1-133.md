# Throughput on the code of the review fixes (F1-133)

Measured on 2026-10-23 at 19:22, with `bolt-throughput` alone, on the code of 840cee6, on the machine of [the F0 baseline](baseline-F0.md): an Apple M5 Pro, macOS 27.0.1, on AC power, with no fuzzing or build running. Rust 1.99.0, release builds. [F1-51's figures](throughput-F1-51.md) were measured on dc61be7, before seven tasks changed the VT core: F1-120, F1-130 and F1-134 how printing treats spacer heads, F1-121 DECRC in origin mode, F1-126 the loops over a row's cells, F1-128 an accessor of the saved cursor, and F1-129 how a page list starts and how `row_text` reads a head.

## Method

The same as [F1-51's](throughput-F1-51.md#method) for both cores: the same bytes fed to each in reads of 64 KiB, at 120 by 40, the median of 5 runs taken in turn; vtebench's scenarios at commit `ead8003`; and `cat` of 1 GiB. The criterion benchmarks were not run again, but reflow's.

## Both cores

| Scenario | Bytes timed | VT core | `alacritty_terminal` | Ratio | Ratio in F1-51 |
|---|---|---|---|---|---|
| cursor_motion | 8.5 MiB | 487.9 MiB/s | 394.3 MiB/s | 1.24 | 1.22 |
| dense_cells | 25.7 MiB | 551.1 MiB/s | 406.7 MiB/s | 1.36 | 1.29 |
| light_cells | 8.6 MiB | 4503.3 MiB/s | 321.6 MiB/s | 14.00 | 14.16 |
| medium_cells | 8.2 MiB | 344.8 MiB/s | 268.7 MiB/s | 1.28 | 1.26 |
| scrolling | 12.0 MiB | 165.3 MiB/s | 94.7 MiB/s | 1.75 | 1.74 |
| scrolling_bottom_region | 12.0 MiB | 152.3 MiB/s | 226.4 MiB/s | 0.67 | 0.68 |
| scrolling_bottom_small_region | 12.0 MiB | 111.1 MiB/s | 77.9 MiB/s | 1.43 | 1.43 |
| scrolling_fullscreen | 8.1 MiB | 3268.9 MiB/s | 268.3 MiB/s | 12.18 | 12.95 |
| scrolling_top_region | 12.0 MiB | 141.5 MiB/s | 42.2 MiB/s | 3.36 | 3.25 |
| scrolling_top_small_region | 12.0 MiB | 99.3 MiB/s | 76.2 MiB/s | 1.30 | 1.27 |
| sync_medium_cells | 8.5 MiB | 341.5 MiB/s | 278.8 MiB/s | 1.22 | 1.22 |
| unicode | 8.4 MiB | 405.4 MiB/s | 389.0 MiB/s | 1.04 | 1.03 |
| `cat` of 1 GiB | 1024 MiB | 900.9 MiB/s | 187.5 MiB/s | 4.81 | 4.38 |

Every scenario keeps at least `alacritty_terminal`'s rate but the bottom region, at 0.67 of it, above the 0.6 of [ADR 0015](../adr/0015-vt-core-throughput-exception.md). The ratios near 1, `unicode` and the bottom region among them, are F1-51's within a few hundredths; dense_cells rose by 0.07, and the large ones, where `alacritty_terminal` is the slower by far, moved by up to 0.8 between the two runs, `cat` of 1 GiB up as `alacritty_terminal` was slower this time.

## Reflow

`benches/reflow.rs`, 120 to 80 columns: 1 000 lines in 324 µs, 10 000 in 2.92 ms, 50 000 in 15.4 ms; about 0.3 µs a row, against about 1 µs when [F1-29](../tasks/F1-vt-core.md#f1-29--reflow-properties) measured it.
