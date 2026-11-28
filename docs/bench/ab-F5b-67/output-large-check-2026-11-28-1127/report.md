# ttybench session output-large-check-2026-11-28-1127

Measured from 2026-11-28 11:27 to 11:29 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a-large | 1.5.0 (202611250509) |
| lokyr-b-large | 1.5.0 (202611280854) |

## A/B: lokyr-b-large against lokyr-a-large

Each metric's median of A, lokyr-a-large, and of B, lokyr-b-large, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| throughput · Its rate to the last write (MiB/s) | 323.0 | 318.9 | -4.2 (-1.3 %) | -8.8 … +3.5 | within the noise |
| throughput · Its rate to the quiet (MiB/s) | 322.8 | 318.7 | -4.1 (-1.3 %) | -8.8 … +3.4 | within the noise |

## throughput

### Its rate to the last write (MiB/s)

`written_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a-large | 323.0 | 316.0 … 325.9 | 328.0 | 322.2 ± 4.8 | 314.9 … 328.7 | 8 (8) | 323.0, 314.9, 326.8, 316.0, 328.7, 321.1, 323.9, 323.0 |
| lokyr-b-large | 318.9 | 315.8 … 326.5 | 328.9 | 320.1 ± 5.8 | 312.8 … 330.2 | 8 (8) | 318.3, 319.4, 312.8, 316.6, 330.2, 326.5, 315.1, 322.0 |

### Its rate to the quiet (MiB/s)

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a-large | 322.8 | 315.8 … 325.7 | 327.9 | 322.0 ± 4.8 | 314.7 … 328.5 | 8 (8) | 322.8, 314.7, 326.6, 315.8, 328.5, 320.9, 323.7, 322.8 |
| lokyr-b-large | 318.7 | 315.6 … 326.2 | 328.6 | 319.9 ± 5.8 | 312.6 … 329.9 | 8 (8) | 318.2, 319.2, 312.6, 316.3, 329.9, 326.2, 314.9, 321.8 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-throughput-lokyr-a-large` | 11:27:29 | 5 | done | 1.34 → 1.48 | WindowServer 26 %, BiomeAgent 14 %, Code Helper (Renderer) 12 % |
| `c1-r1-throughput-lokyr-b-large` | 11:27:36 | 4 | done | 1.48 → 1.68 | WindowServer 28 %, BiomeAgent 13 %, Vorssaint 13 % |
| `c2-r1-throughput-lokyr-b-large` | 11:27:42 | 4 | done | 1.54 → 1.54 | WindowServer 28 %, Code Helper (Renderer) 22 %, Code Helper (Renderer) 17 % |
| `c2-r1-throughput-lokyr-a-large` | 11:27:48 | 4 | done | 1.42 → 1.47 | Code Helper (Renderer) 49 %, WindowServer 25 %, wdavdaemon_unprivileged 19 % |
| `c3-r1-throughput-lokyr-a-large` | 11:27:55 | 4 | done | 1.47 → 1.59 | WindowServer 27 %, BiomeAgent 14 %, wdavdaemon 12 % |
| `c3-r1-throughput-lokyr-b-large` | 11:28:01 | 4 | done | 1.59 → 1.70 | WindowServer 27 %, wdavdaemon_unprivileged 26 %, Code Helper (Renderer) 21 % |
| `c4-r1-throughput-lokyr-b-large` | 11:28:08 | 4 | done | 1.88 → 1.88 | Google Chrome Helper (Aperitif Renderer) 86 %, WindowServer 31 %, Code Helper (Renderer) 12 % |
| `c4-r1-throughput-lokyr-a-large` | 11:28:14 | 4 | done | 1.81 → 2.39 | WindowServer 28 %, Code Helper (Renderer) 23 %, wdavdaemon_unprivileged 17 % |
| `c5-r1-throughput-lokyr-a-large` | 11:28:20 | 4 | done | 2.39 → 2.44 | BackgroundShortcutRunner 63 %, siriactionsd 28 %, WindowServer 21 % |
| `c5-r1-throughput-lokyr-b-large` | 11:28:26 | 4 | done | 2.44 → 2.40 | WindowServer 24 % |
| `c6-r1-throughput-lokyr-b-large` | 11:28:33 | 4 | done | 2.29 → 2.29 | WindowServer 28 %, Code Helper (Renderer) 22 %, BiomeAgent 18 % |
| `c6-r1-throughput-lokyr-a-large` | 11:28:39 | 4 | done | 2.35 → 2.40 | WindowServer 27 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 19 % |
| `c7-r1-throughput-lokyr-a-large` | 11:28:45 | 4 | done | 2.40 → 2.45 | WindowServer 26 %, Code Helper (Renderer) 12 %, MenuBarAgent 12 % |
| `c7-r1-throughput-lokyr-b-large` | 11:28:52 | 4 | done | 2.45 → 2.25 | WindowServer 30 %, Code Helper (Renderer) 23 %, wdavdaemon_unprivileged 20 % |
| `c8-r1-throughput-lokyr-b-large` | 11:28:58 | 4 | done | 2.07 → 2.07 | WindowServer 26 %, wdavdaemon_unprivileged 17 %, Code Helper (Renderer) 15 % |
| `c8-r1-throughput-lokyr-a-large` | 11:29:04 | 4 | done | 2.14 → 2.77 | WindowServer 27 %, Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 17 % |
