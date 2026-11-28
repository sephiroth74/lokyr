# ttybench session ttybench-2026-11-28-1348

Measured from 2026-11-28 13:50 to 13:56 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| v150 | 1.5.0 (202611242301) |
| v160 | 1.6.0 (202611281135) |

## Budgets

0 held, 2 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| startup · Its window after the floor app's, its median | v150 | 80.93 ms | ≤ 70 ms | ✘ not held |
| startup · Its window after the floor app's, its median | v160 | 76.57 ms | ≤ 70 ms | ✘ not held |

## A/B: v160 against v150

Each metric's median of A, v150, and of B, v160, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| startup · From the launch to its window (ms) | 335.9 | 327.7 | -8.2 (-2.4 %) | -13.2 … -2.4 | better |
| startup · From the launch to running the payload (ms) | 790.6 | 772.3 | -18.4 (-2.3 %) | -64.2 … +56.5 | within the noise |
| startup · From the launch to the floor app's window (ms) | 255.4 | 251.8 | -3.6 (-1.4 %) | -14.6 … +8.5 | within the noise |
| startup · Its window after the floor app's (ms) | 80.93 | 76.57 | -4.36 (-5.4 %) | -14.56 … +4.55 | within the noise |

## startup

### From the launch to its window (ms)

`window_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 335.9 | 331.6 … 337.2 | 352.9 | 335.0 ± 10.2 | 304.4 … 354.2 | 32 (4) | 333.5, 336.3, 330.9, 336.9 |
| v160 | 327.7 | 322.3 … 330.6 | 332.8 | 325.5 ± 8.1 | 302.8 … 342.8 | 32 (4) | 328.5, 329.6, 323.1, 326.8 |

### From the launch to running the payload (ms)

`ready_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 790.6 | 728.7 … 823.6 | 857.5 | 784.4 ± 50.9 | 698.4 … 914.2 | 32 (4) | 726.3, 791.8, 790.7, 821.4 |
| v160 | 772.3 | 740.6 … 801.5 | 982.4 | 791.1 ± 109.4 | 694.1 … 1209.9 | 32 (4) | 780.8, 778.3, 806.6, 753.1 |

### From the launch to the floor app's window (ms)

`floor_window_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 255.4 | 248.6 … 262.1 | 269.7 | 254.8 ± 10.4 | 228.8 … 272.2 | 32 (4) | 251.7, 259.3, 255.4, 255.9 |
| v160 | 251.8 | 244.3 … 260.4 | 268.0 | 251.8 ± 12.3 | 222.5 … 280.4 | 32 (4) | 250.6, 257.7, 245.3, 259.3 |

### Its window after the floor app's (ms)

`window_gap_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 80.93 | 73.90 … 86.43 | 107.39 | 80.16 ± 16.02 | 44.72 … 115.43 | 32 (4) | 78.21, 77.99, 72.10, 86.43 |
| v160 | 76.57 | 67.72 … 81.22 | 88.56 | 73.68 ± 13.39 | 25.18 … 99.25 | 32 (4) | 80.88, 71.77, 79.80, 73.73 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-startup-v150` | 13:50:33 | 41 | done | 2.51 → 1.81 | WindowServer 15 % |
| `c1-r1-startup-v160` | 13:51:15 | 41 | done | 1.81 → 1.84 | WindowServer 20 % |
| `c2-r1-startup-v160` | 13:51:58 | 41 | done | 1.78 → 1.80 | WindowServer 19 % |
| `c2-r1-startup-v150` | 13:52:41 | 41 | done | 1.80 → 2.20 | WindowServer 16 % |
| `c3-r1-startup-v150` | 13:53:24 | 41 | done | 2.42 → 5.26 | WindowServer 19 % |
| `c3-r1-startup-v160` | 13:54:07 | 42 | done | 5.26 → 6.50 | BackgroundShortcutRunner 47 %, siriactionsd 47 %, WindowServer 14 % |
| `c4-r1-startup-v160` | 13:54:51 | 41 | done | 6.50 → 3.85 | WindowServer 19 % |
| `c4-r1-startup-v150` | 13:55:34 | 41 | done | 3.62 → 3.10 | WindowServer 16 % |
