# ttybench session scenario-hidden-session-memory-2026-11-28-1109

Measured from 2026-11-28 11:09 to 11:13 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-a-hidden-session | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |
| lokyr-b-hidden-session | 1.5.0 (202611280854) |

## A/B: lokyr-b-hidden-session against lokyr-a-hidden-session

Each metric's median of A, lokyr-a-hidden-session, and of B, lokyr-b-hidden-session, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| memory · Its memory's footprint (MiB) | 93.82 | 84.20 | -9.62 (-10.3 %) | -11.78 … +4.52 | within the noise |
| memory · Its memory's peak footprint (MiB) | 251.1 | 257.6 | +6.6 (+2.6 %) | +2.1 … +26.1 | **worse** |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 1, settle_s 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 54.96 | 49.99 … 55.00 | 55.00 | 53.72 ± 2.49 | 49.99 … 55.00 | 4 (4) | 49.99, 54.92, 54.99, 55.00 |
| lokyr-a-hidden-session | 93.82 | 79.67 … 94.11 | 94.07 | 90.35 ± 7.12 | 79.67 … 94.11 | 4 (4) | 79.67, 93.77, 93.86, 94.11 |
| lokyr-b | 49.53 | 49.42 … 54.72 | 53.95 | 50.80 ± 2.61 | 49.42 … 54.72 | 4 (4) | 49.42, 49.56, 54.72, 49.50 |
| lokyr-b-hidden-session | 84.20 | 82.03 … 84.61 | 84.58 | 83.76 ± 1.18 | 82.03 … 84.61 | 4 (4) | 83.97, 84.61, 84.42, 82.03 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 1, settle_s 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 211.9 | 211.8 … 211.9 | 211.9 | 211.9 ± 0.0 | 211.8 … 211.9 | 4 (4) | 211.9, 211.8, 211.9, 211.9 |
| lokyr-a-hidden-session | 251.1 | 248.6 … 251.3 | 251.3 | 250.5 ± 1.3 | 248.6 … 251.3 | 4 (4) | 248.6, 251.0, 251.1, 251.3 |
| lokyr-b | 211.5 | 211.4 … 211.7 | 211.6 | 211.5 ± 0.1 | 211.4 … 211.7 | 4 (4) | 211.4, 211.5, 211.7, 211.5 |
| lokyr-b-hidden-session | 257.6 | 253.2 … 277.2 | 274.3 | 261.4 ± 10.7 | 253.2 … 277.2 | 4 (4) | 257.5, 277.2, 257.8, 253.2 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-memory-lokyr-a` | 11:09:59 | 11 | done | 3.66 → 5.67 | wdavdaemon_unprivileged 123 %, wdavdaemon 28 %, Code Helper (Renderer) 26 % |
| `c1-r1-memory-lokyr-a-hidden-session` | 11:10:12 | 11 | done | 5.67 → 5.03 | wdavdaemon_unprivileged 28 %, WindowServer 17 %, BiomeAgent 14 % |
| `c1-r1-memory-lokyr-b` | 11:10:25 | 11 | done | 5.11 → 4.40 |  |
| `c1-r1-memory-lokyr-b-hidden-session` | 11:10:38 | 11 | done | 4.40 → 4.03 | Code Helper (Renderer) 22 %, WindowServer 18 %, BiomeAgent 15 % |
| `c2-r1-memory-lokyr-a-hidden-session` | 11:10:51 | 11 | done | 3.86 → 3.35 | MenuBarAgent 16 %, Code Helper (Renderer) 15 %, WindowServer 14 % |
| `c2-r1-memory-lokyr-b` | 11:11:04 | 12 | done | 3.35 → 4.51 | Code Helper (Renderer) 24 %, wdavdaemon_unprivileged 18 %, WindowServer 16 % |
| `c2-r1-memory-lokyr-b-hidden-session` | 11:11:17 | 11 | done | 4.51 → 3.90 | Code Helper (Renderer) 22 %, WindowServer 20 %, wdavdaemon_unprivileged 18 % |
| `c2-r1-memory-lokyr-a` | 11:11:30 | 11 | done | 3.66 → 3.25 | spotlightknowledged.updater 27 %, Code Helper (Renderer) 23 %, wdavdaemon_unprivileged 21 % |
| `c3-r1-memory-lokyr-b` | 11:11:43 | 11 | done | 3.25 → 3.06 | Code Helper (Renderer) 20 %, WindowServer 18 %, BiomeAgent 16 % |
| `c3-r1-memory-lokyr-b-hidden-session` | 11:11:56 | 11 | done | 3.14 → 2.81 |  |
| `c3-r1-memory-lokyr-a` | 11:12:09 | 11 | done | 2.81 → 2.53 | WindowServer 18 %, BiomeAgent 13 %, MenuBarAgent 11 % |
| `c3-r1-memory-lokyr-a-hidden-session` | 11:12:22 | 11 | done | 2.57 → 2.32 | Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 19 %, BiomeAgent 16 % |
| `c4-r1-memory-lokyr-b-hidden-session` | 11:12:35 | 11 | done | 2.32 → 2.43 |  |
| `c4-r1-memory-lokyr-a` | 11:12:48 | 11 | done | 2.43 → 2.59 | WindowServer 16 %, MenuBarAgent 12 % |
| `c4-r1-memory-lokyr-a-hidden-session` | 11:13:01 | 11 | done | 2.38 → 2.17 | Google Chrome Helper (Aperitif Renderer) 16 %, MenuBarAgent 16 %, WindowServer 15 % |
| `c4-r1-memory-lokyr-b` | 11:13:14 | 11 | done | 2.17 → 2.15 | Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 18 %, WindowServer 16 % |
