# ttybench session ttybench-2026-11-25-1449

Measured from 2026-11-25 14:51 to 14:55 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 4 runs: 4 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| a | 1.5.0 (202611250509) |
| b | 1.5.0 (202611251327) |

## A/B: b against a

Each metric's median of A, a, and of B, b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| startup · From the launch to its window (ms) | 318.4 | 311.3 | -7.1 (-2.2 %) | -16.9 … -0.9 | better |
| startup · From the launch to running the payload (ms) | 657.3 | 645.5 | -11.8 (-1.8 %) | -19.5 … -4.8 | better |
| startup · From the launch to the floor app's window (ms) | 256.7 | 253.8 | -2.9 (-1.1 %) | -13.6 … +7.2 | within the noise |
| startup · Its window after the floor app's (ms) | 60.34 | 58.11 | -2.23 (-3.7 %) | -18.36 … +10.78 | within the noise |

## startup

### From the launch to its window (ms)

`window_ms`. Conditions: cols 120, pause_s 2.0, rounds 12, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| a | 318.4 | 313.8 … 321.7 | 330.9 | 313.0 ± 18.4 | 250.3 … 337.1 | 24 (2) | 318.4, 318.9 |
| b | 311.3 | 302.0 … 314.7 | 318.4 | 307.7 ± 11.7 | 267.0 … 319.7 | 24 (2) | 309.9, 311.5 |

### From the launch to running the payload (ms)

`ready_ms`. Conditions: cols 120, pause_s 2.0, rounds 12, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| a | 657.3 | 652.6 … 661.7 | 670.8 | 651.7 ± 21.7 | 573.4 … 685.5 | 24 (2) | 657.0, 660.0 |
| b | 645.5 | 639.1 … 651.5 | 654.1 | 642.6 ± 12.1 | 610.6 … 665.1 | 24 (2) | 644.7, 649.1 |

### From the launch to the floor app's window (ms)

`floor_window_ms`. Conditions: cols 120, pause_s 2.0, rounds 12, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| a | 256.7 | 249.7 … 263.5 | 272.2 | 255.4 ± 13.3 | 214.9 … 279.1 | 24 (2) | 250.8, 259.4 |
| b | 253.8 | 244.7 … 258.1 | 266.5 | 252.1 ± 11.0 | 233.1 … 277.0 | 24 (2) | 254.5, 253.8 |

### Its window after the floor app's (ms)

`window_gap_ms`. Conditions: cols 120, pause_s 2.0, rounds 12, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| a | 60.34 | 52.25 … 70.40 | 87.18 | 57.52 ± 26.23 | -12.70 … 103.06 | 24 (2) | 61.53, 56.37 |
| b | 58.11 | 47.97 … 64.28 | 80.16 | 55.63 ± 17.23 | 15.55 … 83.88 | 24 (2) | 52.04, 62.50 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-startup-a` | 14:51:32 | 60 | done | 2.60 → 1.56 | WindowServer 18 % |
| `c1-r1-startup-b` | 14:52:34 | 60 | done | 1.56 → 4.38 | WindowServer 20 % |
| `c2-r1-startup-b` | 14:53:35 | 60 | done | 4.38 → 2.26 | wdavdaemon_unprivileged 25 %, WindowServer 18 % |
| `c2-r1-startup-a` | 14:54:37 | 60 | done | 2.08 → 1.20 | WindowServer 17 % |
