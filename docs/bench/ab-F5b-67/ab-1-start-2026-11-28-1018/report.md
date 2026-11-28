# ttybench session ab-1-start-2026-11-28-1018

Measured from 2026-11-28 10:20 to 10:27 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## Budgets

0 held, 2 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| startup · Its window after the floor app's, its median | lokyr-a | 92.49 ms | ≤ 70 ms | ✘ not held |
| startup · Its window after the floor app's, its median | lokyr-b | 85.96 ms | ≤ 70 ms | ✘ not held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| startup · From the launch to its window (ms) | 354.6 | 357.5 | +2.9 (+0.8 %) | -12.9 … +10.3 | within the noise |
| startup · From the launch to running the payload (ms) | 792.6 | 796.5 | +3.9 (+0.5 %) | -66.7 … +68.9 | within the noise |
| startup · From the launch to the floor app's window (ms) | 262.6 | 264.6 | +2.0 (+0.8 %) | -6.6 … +11.0 | within the noise |
| startup · Its window after the floor app's (ms) | 92.49 | 85.96 | -6.54 (-7.1 %) | -15.81 … +10.27 | within the noise |
| memory · Its memory's footprint (MiB) | 55.01 | 54.55 | -0.46 (-0.8 %) | -0.67 … -0.30 | better |
| memory · Its memory's peak footprint (MiB) | 211.8 | 211.4 | -0.5 (-0.2 %) | -0.7 … -0.3 | better |

## startup

### From the launch to its window (ms)

`window_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 354.6 | 351.4 … 362.4 | 370.9 | 355.3 ± 11.6 | 331.9 … 379.6 | 32 (4) | 360.0, 352.8, 353.2, 360.1 |
| lokyr-b | 357.5 | 346.2 … 363.1 | 377.9 | 355.4 ± 15.4 | 326.5 … 386.4 | 32 (4) | 355.6, 358.2, 346.2, 361.2 |

### From the launch to running the payload (ms)

`ready_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 792.6 | 750.4 … 839.6 | 890.3 | 806.2 ± 79.8 | 701.0 … 1118.0 | 32 (4) | 748.6, 789.6, 805.9, 834.7 |
| lokyr-b | 796.5 | 745.9 … 826.2 | 885.0 | 793.8 ± 58.4 | 704.4 … 961.9 | 32 (4) | 769.0, 756.3, 789.0, 825.8 |

### From the launch to the floor app's window (ms)

`floor_window_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 262.6 | 255.8 … 270.2 | 279.1 | 263.1 ± 11.5 | 231.5 … 285.4 | 32 (4) | 261.6, 265.1, 263.5, 258.0 |
| lokyr-b | 264.6 | 259.7 … 271.7 | 287.3 | 264.8 ± 12.7 | 245.6 … 290.0 | 32 (4) | 264.4, 263.2, 267.2, 265.7 |

### Its window after the floor app's (ms)

`window_gap_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 92.49 | 86.13 … 99.87 | 117.06 | 92.24 ± 16.74 | 49.93 … 124.65 | 32 (4) | 99.29, 91.11, 87.65, 97.42 |
| lokyr-b | 85.96 | 79.09 … 101.45 | 126.39 | 90.61 ± 19.64 | 58.26 … 130.76 | 32 (4) | 93.86, 95.33, 81.30, 87.14 |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 2, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 55.01 | 54.91 … 55.22 | 55.22 | 55.04 ± 0.13 | 54.91 … 55.22 | 8 (4) | 55.00, 55.06, 55.01, 55.10 |
| lokyr-b | 54.55 | 54.46 … 54.64 | 54.67 | 54.56 ± 0.09 | 54.44 … 54.69 | 8 (4) | 54.51, 54.64, 54.46, 54.62 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 2, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 211.8 | 211.7 … 212.0 | 212.0 | 211.9 ± 0.1 | 211.7 … 212.0 | 8 (4) | 211.8, 211.9, 211.8, 211.9 |
| lokyr-b | 211.4 | 211.3 … 211.5 | 211.5 | 211.4 ± 0.1 | 211.3 … 211.5 | 8 (4) | 211.3, 211.4, 211.3, 211.4 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-startup-lokyr-a` | 10:20:30 | 41 | done | 3.76 → 2.84 |  |
| `c1-r1-startup-lokyr-b` | 10:21:13 | 41 | done | 2.84 → 3.01 |  |
| `c1-r2-memory-lokyr-a` | 10:21:56 | 10 | done | 2.85 → 2.56 | Code Helper (Renderer) 21 %, BiomeAgent 20 %, wdavdaemon_unprivileged 18 % |
| `c1-r2-memory-lokyr-b` | 10:22:08 | 10 | done | 2.56 → 3.20 | MenuBarAgent 16 %, WindowServer 16 %, jetbrains-toolbox 15 % |
| `c2-r1-startup-lokyr-b` | 10:22:20 | 41 | done | 2.95 → 1.94 |  |
| `c2-r1-startup-lokyr-a` | 10:23:03 | 41 | done | 1.94 → 1.60 |  |
| `c2-r2-memory-lokyr-b` | 10:23:47 | 10 | done | 1.56 → 2.60 | Code Helper (Renderer) 44 %, WindowServer 18 %, BiomeAgent 16 % |
| `c2-r2-memory-lokyr-a` | 10:23:59 | 10 | done | 2.60 → 2.35 | Code Helper (Renderer) 96 %, BiomeAgent 16 %, wdavdaemon_unprivileged 16 % |
| `c3-r1-startup-lokyr-a` | 10:24:11 | 41 | done | 2.24 → 2.48 |  |
| `c3-r1-startup-lokyr-b` | 10:24:54 | 41 | done | 2.48 → 5.88 |  |
| `c3-r2-memory-lokyr-a` | 10:25:37 | 10 | done | 5.49 → 4.95 | launchd 44 %, Code Helper (Renderer) 15 %, MenuBarAgent 15 % |
| `c3-r2-memory-lokyr-b` | 10:25:49 | 10 | done | 4.95 → 4.27 | Code Helper (Renderer) 22 %, BiomeAgent 17 %, WindowServer 17 % |
| `c4-r1-startup-lokyr-b` | 10:26:01 | 41 | done | 3.92 → 3.01 |  |
| `c4-r1-startup-lokyr-a` | 10:26:45 | 41 | done | 3.01 → 2.62 | WindowServer 12 % |
| `c4-r2-memory-lokyr-b` | 10:27:28 | 10 | done | 2.49 → 2.26 | Code Helper (Renderer) 108 %, WindowServer 26 %, BiomeAgent 16 % |
| `c4-r2-memory-lokyr-a` | 10:27:40 | 10 | done | 2.26 → 2.23 | Google Chrome Helper (Aperitif Renderer) 45 %, MenuBarAgent 15 %, WindowServer 13 % |
