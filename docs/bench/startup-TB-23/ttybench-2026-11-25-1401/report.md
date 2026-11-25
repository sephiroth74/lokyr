# ttybench session ttybench-2026-11-25-1401

Measured from 2026-11-25 14:01 to 14:04 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 2 runs: 2 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| v150 | 1.5.0 (202611242301) |

## startup

### From the launch to its window (ms)

`window_ms`. Conditions: cols 120, pause_s 2.0, rounds 16, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 330.6 | 323.1 … 333.1 | 346.1 | 329.3 ± 10.6 | 300.0 … 351.2 | 32 (2) | 331.4, 329.8 |

### From the launch to running the payload (ms)

`ready_ms`. Conditions: cols 120, pause_s 2.0, rounds 16, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 694.5 | 690.5 … 701.7 | 714.0 | 695.7 ± 13.8 | 654.2 … 720.3 | 32 (2) | 700.2, 692.4 |

### From the launch to the floor app's window (ms)

`floor_window_ms`. Conditions: cols 120, pause_s 2.0, rounds 16, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 259.8 | 252.9 … 263.7 | 272.7 | 255.6 ± 16.0 | 203.0 … 284.0 | 32 (2) | 258.8, 260.2 |

### Its window after the floor app's (ms)

`window_gap_ms`. Conditions: cols 120, pause_s 2.0, rounds 16, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| v150 | 70.38 | 67.09 … 75.45 | 99.56 | 73.66 ± 15.24 | 47.05 … 128.07 | 32 (2) | 72.28, 69.41 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-startup-v150` | 14:01:55 | 80 | done | 2.46 → 9.72 | wdavdaemon_unprivileged 22 %, WindowServer 17 %, tccd 14 % |
| `c2-r1-startup-v150` | 14:03:17 | 80 | done | 9.72 → 3.76 | WindowServer 14 % |
