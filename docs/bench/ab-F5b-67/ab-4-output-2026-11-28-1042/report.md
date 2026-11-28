# ttybench session ab-4-output-2026-11-28-1042

Measured from 2026-11-28 10:42 to 10:45 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 24 runs: 24 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## Budgets

2 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| throughput · Its rate to the quiet, its median, text plain | lokyr-a | 315.4 MiB/s | ≥ 275 MiB/s | ✔ held |
| throughput · Its rate to the quiet, its median, text plain | lokyr-b | 316.2 MiB/s | ≥ 275 MiB/s | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| throughput · Its rate to the last write (MiB/s), text plain | 315.6 | 316.4 | +0.8 (+0.3 %) | -4.1 … +4.5 | within the noise |
| throughput · Its rate to the last write (MiB/s), text colors | 278.7 | 283.3 | +4.7 (+1.7 %) | -10.3 … +13.3 | within the noise |
| throughput · Its rate to the last write (MiB/s), text wide | 328.2 | 325.1 | -3.2 (-1.0 %) | -22.3 … +7.3 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text plain | 315.4 | 316.2 | +0.8 (+0.3 %) | -4.1 … +4.5 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text colors | 278.5 | 283.2 | +4.7 (+1.7 %) | -10.3 … +13.3 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text wide | 328.0 | 324.9 | -3.1 (-1.0 %) | -22.3 … +7.3 | within the noise |

## throughput

### Its rate to the last write (MiB/s), text plain

`written_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 315.6 | 312.4 … 320.5 | 319.9 | 316.0 ± 3.5 | 312.4 … 320.5 | 4 (4) | 312.4, 314.6, 320.5, 316.7 |
| lokyr-b | 316.4 | 314.3 … 319.0 | 318.7 | 316.5 ± 2.0 | 314.3 … 319.0 | 4 (4) | 316.1, 319.0, 314.3, 316.7 |

### Its rate to the last write (MiB/s), text colors

`written_mib_s`. Conditions: mib 1024, rounds 1, text colors.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 278.7 | 270.0 … 286.3 | 286.2 | 278.4 ± 8.8 | 270.0 … 286.3 | 4 (4) | 285.7, 271.6, 286.3, 270.0 |
| lokyr-b | 283.3 | 268.4 … 283.9 | 283.9 | 279.7 ± 7.6 | 268.4 … 283.9 | 4 (4) | 283.9, 283.5, 268.4, 283.1 |

### Its rate to the last write (MiB/s), text wide

`written_mib_s`. Conditions: mib 1024, rounds 1, text wide.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 328.2 | 317.8 … 330.5 | 330.2 | 326.2 ± 5.7 | 317.8 … 330.5 | 4 (4) | 317.8, 328.2, 330.5, 328.3 |
| lokyr-b | 325.1 | 305.9 … 328.9 | 328.4 | 321.2 ± 10.4 | 305.9 … 328.9 | 4 (4) | 325.5, 328.9, 324.6, 305.9 |

### Its rate to the quiet (MiB/s), text plain

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 315.4 | 312.2 … 320.3 | 319.7 | 315.8 ± 3.4 | 312.2 … 320.3 | 4 (4) | 312.2, 314.3, 320.3, 316.4 |
| lokyr-b | 316.2 | 314.1 … 318.8 | 318.5 | 316.3 ± 2.0 | 314.1 … 318.8 | 4 (4) | 315.8, 318.8, 314.1, 316.5 |

### Its rate to the quiet (MiB/s), text colors

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text colors.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 278.5 | 269.9 … 286.1 | 286.0 | 278.2 ± 8.8 | 269.9 … 286.1 | 4 (4) | 285.5, 271.5, 286.1, 269.9 |
| lokyr-b | 283.2 | 268.2 … 283.9 | 283.8 | 279.6 ± 7.6 | 268.2 … 283.9 | 4 (4) | 283.9, 283.4, 268.2, 282.9 |

### Its rate to the quiet (MiB/s), text wide

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text wide.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 328.0 | 317.6 … 330.3 | 329.9 | 326.0 ± 5.7 | 317.6 … 330.3 | 4 (4) | 317.6, 328.0, 330.3, 328.0 |
| lokyr-b | 324.9 | 305.7 … 328.7 | 328.2 | 321.0 ± 10.4 | 305.7 … 328.7 | 4 (4) | 325.3, 328.7, 324.4, 305.7 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-throughput-lokyr-a` | 10:42:35 | 4 | done | 1.50 → 1.50 | WindowServer 23 %, Code Helper (Renderer) 18 %, wdavdaemon_unprivileged 16 % |
| `c1-r1-throughput-lokyr-b` | 10:42:42 | 4 | done | 1.38 → 1.43 | WindowServer 26 %, BiomeAgent 15 %, wdavdaemon 11 % |
| `c1-r2-throughput-lokyr-a` | 10:42:48 | 5 | done | 1.43 → 1.55 | WindowServer 24 %, Code Helper (Renderer) 15 %, MenuBarAgent 14 % |
| `c1-r2-throughput-lokyr-b` | 10:42:55 | 5 | done | 2.07 → 1.90 | Code Helper (Renderer) 103 %, WindowServer 26 %, wdavdaemon_unprivileged 18 % |
| `c1-r3-throughput-lokyr-a` | 10:43:02 | 5 | done | 1.90 → 1.91 | WindowServer 26 %, Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 19 % |
| `c1-r3-throughput-lokyr-b` | 10:43:09 | 4 | done | 1.91 → 2.16 | WindowServer 26 %, Code Helper (Renderer) 21 %, BiomeAgent 18 % |
| `c2-r1-throughput-lokyr-b` | 10:43:16 | 4 | done | 2.06 → 2.06 | WindowServer 26 %, Code Helper (Renderer) 23 %, wdavdaemon_unprivileged 20 % |
| `c2-r1-throughput-lokyr-a` | 10:43:22 | 4 | done | 1.98 → 2.06 | WindowServer 25 %, Code Helper (Renderer) 21 %, BiomeAgent 16 % |
| `c2-r2-throughput-lokyr-b` | 10:43:28 | 5 | done | 2.06 → 2.14 | Code Helper (Renderer) 91 %, WindowServer 26 %, BiomeAgent 19 % |
| `c2-r2-throughput-lokyr-a` | 10:43:35 | 5 | done | 2.14 → 2.28 | WindowServer 29 %, wdavdaemon_unprivileged 24 %, Code Helper (Renderer) 23 % |
| `c2-r3-throughput-lokyr-b` | 10:43:42 | 4 | done | 2.18 → 2.25 | WindowServer 29 %, BiomeAgent 14 %, Code Helper (Renderer) 13 % |
| `c2-r3-throughput-lokyr-a` | 10:43:48 | 4 | done | 2.25 → 2.55 | WindowServer 23 %, Code Helper (Renderer) 15 %, MenuBarAgent 14 % |
| `c3-r1-throughput-lokyr-a` | 10:43:55 | 4 | done | 2.55 → 2.50 | WindowServer 25 %, Code Helper (Renderer) 20 %, wdavdaemon_unprivileged 17 % |
| `c3-r1-throughput-lokyr-b` | 10:44:01 | 4 | done | 2.30 → 2.30 | WindowServer 24 %, MenuBarAgent 15 %, Code Helper (Renderer) 13 % |
| `c3-r2-throughput-lokyr-a` | 10:44:07 | 5 | done | 2.44 → 2.48 | Google Chrome Helper (Aperitif Renderer) 41 %, WindowServer 30 %, MenuBarAgent 14 % |
| `c3-r2-throughput-lokyr-b` | 10:44:14 | 5 | done | 2.48 → 2.60 | WindowServer 20 % |
| `c3-r3-throughput-lokyr-a` | 10:44:21 | 4 | done | 2.48 → 2.48 | WindowServer 30 %, Code Helper (Renderer) 22 %, BiomeAgent 19 % |
| `c3-r3-throughput-lokyr-b` | 10:44:27 | 4 | done | 2.36 → 2.41 | Code Helper (Renderer) 87 %, WindowServer 30 %, wdavdaemon_unprivileged 24 % |
| `c4-r1-throughput-lokyr-b` | 10:44:33 | 4 | done | 2.41 → 2.62 | WindowServer 13 %, runningboardd 10 % |
| `c4-r1-throughput-lokyr-a` | 10:44:39 | 4 | done | 2.62 → 2.65 | WindowServer 22 %, Code Helper (Renderer) 17 %, MenuBarAgent 16 % |
| `c4-r2-throughput-lokyr-b` | 10:44:46 | 5 | done | 2.43 → 2.24 | WindowServer 26 %, Code Helper (Renderer) 23 %, wdavdaemon_unprivileged 19 % |
| `c4-r2-throughput-lokyr-a` | 10:44:53 | 5 | done | 2.24 → 2.38 | WindowServer 24 %, BiomeAgent 16 %, wdavdaemon 11 % |
| `c4-r3-throughput-lokyr-b` | 10:45:00 | 4 | done | 2.38 → 2.43 | Code Helper (Renderer) 35 %, WindowServer 25 %, wdavdaemon_unprivileged 15 % |
| `c4-r3-throughput-lokyr-a` | 10:45:06 | 4 | done | 2.23 → 2.23 | WindowServer 25 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 19 % |
