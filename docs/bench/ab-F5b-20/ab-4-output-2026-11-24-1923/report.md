# ttybench session ab-4-output-2026-11-24-1923

Measured from 2026-11-24 19:23 to 19:26 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 24 runs: 24 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## Budgets

2 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| throughput · Its rate to the quiet, its median, text plain | lokyr-a | 313.0 MiB/s | ≥ 275 MiB/s | ✔ held |
| throughput · Its rate to the quiet, its median, text plain | lokyr-b | 313.9 MiB/s | ≥ 275 MiB/s | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| throughput · Its rate to the last write (MiB/s), text plain | 313.3 | 314.1 | +0.9 (+0.3 %) | -3.7 … +6.3 | within the noise |
| throughput · Its rate to the last write (MiB/s), text colors | 282.7 | 282.2 | -0.5 (-0.2 %) | -9.0 … +3.6 | within the noise |
| throughput · Its rate to the last write (MiB/s), text wide | 330.6 | 330.5 | -0.1 (-0.0 %) | -1.8 … +6.1 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text plain | 313.0 | 313.9 | +0.9 (+0.3 %) | -3.8 … +6.3 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text colors | 282.5 | 282.0 | -0.5 (-0.2 %) | -8.9 … +3.6 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text wide | 330.3 | 330.3 | -0.1 (-0.0 %) | -1.8 … +6.0 | within the noise |

## throughput

### Its rate to the last write (MiB/s), text plain

`written_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 313.3 | 310.3 … 315.8 | 315.7 | 313.2 ± 2.5 | 310.3 … 315.8 | 4 (4) | 311.8, 314.7, 310.3, 315.8 |
| lokyr-b | 314.1 | 309.9 … 319.0 | 318.3 | 314.3 ± 3.7 | 309.9 … 319.0 | 4 (4) | 319.0, 309.9, 314.3, 313.9 |

### Its rate to the last write (MiB/s), text colors

`written_mib_s`. Conditions: mib 1024, rounds 1, text colors.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 282.7 | 278.8 … 283.1 | 283.1 | 281.8 ± 2.1 | 278.8 … 283.1 | 4 (4) | 283.1, 282.5, 282.9, 278.8 |
| lokyr-b | 282.2 | 273.8 … 284.3 | 284.0 | 280.6 ± 4.7 | 273.8 … 284.3 | 4 (4) | 282.3, 273.8, 282.1, 284.3 |

### Its rate to the last write (MiB/s), text wide

`written_mib_s`. Conditions: mib 1024, rounds 1, text wide.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 330.6 | 324.4 … 331.6 | 331.5 | 329.3 ± 3.3 | 324.4 … 331.6 | 4 (4) | 331.6, 331.2, 329.9, 324.4 |
| lokyr-b | 330.5 | 329.0 … 331.5 | 331.4 | 330.4 ± 1.1 | 329.0 … 331.5 | 4 (4) | 329.0, 330.7, 331.5, 330.4 |

### Its rate to the quiet (MiB/s), text plain

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 313.0 | 310.1 … 315.6 | 315.5 | 313.0 ± 2.5 | 310.1 … 315.6 | 4 (4) | 311.6, 314.4, 310.1, 315.6 |
| lokyr-b | 313.9 | 309.6 … 318.7 | 318.0 | 314.1 ± 3.7 | 309.6 … 318.7 | 4 (4) | 318.7, 309.6, 314.1, 313.7 |

### Its rate to the quiet (MiB/s), text colors

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text colors.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 282.5 | 278.6 … 283.0 | 283.0 | 281.7 ± 2.1 | 278.6 … 283.0 | 4 (4) | 283.0, 282.3, 282.7, 278.6 |
| lokyr-b | 282.0 | 273.6 … 284.1 | 283.8 | 280.4 ± 4.7 | 273.6 … 284.1 | 4 (4) | 282.1, 273.6, 281.9, 284.1 |

### Its rate to the quiet (MiB/s), text wide

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text wide.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 330.3 | 324.2 … 331.4 | 331.3 | 329.1 ± 3.3 | 324.2 … 331.4 | 4 (4) | 331.4, 331.0, 329.7, 324.2 |
| lokyr-b | 330.3 | 328.7 … 331.2 | 331.1 | 330.1 ± 1.0 | 328.7 … 331.2 | 4 (4) | 328.7, 330.4, 331.2, 330.1 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-throughput-lokyr-a` | 19:23:46 | 5 | done | 3.52 → 3.48 | WindowServer 35 %, wdavdaemon_unprivileged 29 %, Code Helper (Renderer) 20 % |
| `c1-r1-throughput-lokyr-b` | 19:23:53 | 4 | done | 3.48 → 3.44 | WindowServer 32 %, wdavdaemon_unprivileged 29 %, Code Helper (Renderer) 20 % |
| `c1-r2-throughput-lokyr-a` | 19:24:00 | 5 | done | 3.44 → 3.48 | WindowServer 34 %, wdavdaemon_unprivileged 22 %, BiomeAgent 20 % |
| `c1-r2-throughput-lokyr-b` | 19:24:07 | 5 | done | 3.44 → 3.57 | WindowServer 33 %, BiomeAgent 19 %, Code Helper (Renderer) 15 % |
| `c1-r3-throughput-lokyr-a` | 19:24:14 | 5 | done | 3.57 → 3.44 | WindowServer 35 %, MenuBarAgent 15 %, Code Helper 12 % |
| `c1-r3-throughput-lokyr-b` | 19:24:20 | 4 | done | 3.44 → 3.33 | WindowServer 36 %, wdavdaemon_unprivileged 24 %, Code Helper (Renderer) 21 % |
| `c2-r1-throughput-lokyr-b` | 19:24:27 | 4 | done | 3.14 → 3.13 | WindowServer 35 %, wdavdaemon_unprivileged 27 %, Code Helper (Renderer) 20 % |
| `c2-r1-throughput-lokyr-a` | 19:24:33 | 4 | done | 3.13 → 3.12 | WindowServer 32 %, Code Helper (Renderer) 18 %, BiomeAgent 17 % |
| `c2-r2-throughput-lokyr-b` | 19:24:39 | 5 | done | 3.12 → 3.19 | WindowServer 36 %, wdavdaemon_unprivileged 32 %, MenuBarAgent 16 % |
| `c2-r2-throughput-lokyr-a` | 19:24:46 | 5 | done | 3.25 → 3.23 | wdavdaemon_unprivileged 36 %, WindowServer 34 %, Code Helper (Renderer) 19 % |
| `c2-r3-throughput-lokyr-b` | 19:24:53 | 4 | done | 3.23 → 3.21 | Code Helper (Renderer) 58 %, WindowServer 35 %, Code Helper 15 % |
| `c2-r3-throughput-lokyr-a` | 19:24:59 | 4 | done | 3.21 → 3.20 | WindowServer 36 %, BiomeAgent 22 %, wdavdaemon_unprivileged 21 % |
| `c3-r1-throughput-lokyr-a` | 19:25:05 | 4 | done | 3.20 → 3.34 | Code Helper (Renderer) 98 %, WindowServer 34 %, wdavdaemon_unprivileged 22 % |
| `c3-r1-throughput-lokyr-b` | 19:25:11 | 4 | done | 3.47 → 3.27 | WindowServer 33 %, MenuBarAgent 15 %, Code Helper (Renderer) 14 % |
| `c3-r2-throughput-lokyr-a` | 19:25:18 | 5 | done | 3.27 → 3.25 | Code Helper (Renderer) 98 %, WindowServer 34 %, wdavdaemon_unprivileged 19 % |
| `c3-r2-throughput-lokyr-b` | 19:25:24 | 5 | done | 3.25 → 3.31 | WindowServer 32 %, BiomeAgent 18 %, wdavdaemon_unprivileged 15 % |
| `c3-r3-throughput-lokyr-a` | 19:25:31 | 4 | done | 3.29 → 3.29 | Code Helper (Renderer) 97 %, WindowServer 33 %, wdavdaemon_unprivileged 23 % |
| `c3-r3-throughput-lokyr-b` | 19:25:37 | 4 | done | 3.02 → 3.10 | WindowServer 36 %, wdavdaemon_unprivileged 22 %, Code Helper (Renderer) 18 % |
| `c4-r1-throughput-lokyr-b` | 19:25:43 | 4 | done | 3.10 → 3.09 | Code Helper (Renderer) 100 %, WindowServer 35 %, wdavdaemon_unprivileged 21 % |
| `c4-r1-throughput-lokyr-a` | 19:25:50 | 4 | done | 3.09 → 3.08 | WindowServer 34 %, Code Helper (Renderer) 15 %, MenuBarAgent 15 % |
| `c4-r2-throughput-lokyr-b` | 19:25:56 | 5 | done | 3.00 → 3.00 | Code Helper (Renderer) 96 %, WindowServer 33 %, BiomeAgent 18 % |
| `c4-r2-throughput-lokyr-a` | 19:26:03 | 5 | done | 3.00 → 3.00 | WindowServer 31 %, BiomeAgent 17 %, Code Helper (Renderer) 16 % |
| `c4-r3-throughput-lokyr-b` | 19:26:10 | 4 | done | 3.00 → 3.16 | Code Helper (Renderer) 95 %, WindowServer 34 %, BiomeAgent 17 % |
| `c4-r3-throughput-lokyr-a` | 19:26:16 | 4 | done | 3.31 → 3.31 | WindowServer 32 %, Code Helper (Renderer) 23 %, BiomeAgent 18 % |
