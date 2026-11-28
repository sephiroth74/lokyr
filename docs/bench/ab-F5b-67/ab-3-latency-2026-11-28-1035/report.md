# ttybench session ab-3-latency-2026-11-28-1035

Measured from 2026-11-28 10:35 to 10:42 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## Budgets

4 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| latency · A key to its echo, its median | lokyr-a | 11.41 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its median | lokyr-b | 11.42 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its p99 | lokyr-a | 26.33 ms | ≤ 75 ms | ✔ held |
| latency · A key to its echo, its p99 | lokyr-b | 24.54 ms | ≤ 75 ms | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| latency · A key to its echo (ms) | 11.41 | 11.42 | +0.01 (+0.1 %) | -0.18 … +0.18 | within the noise |
| latency · Keys of no echo seen (keys) | 0 | 0 | +0 | +0 … +0 | within the noise |

## latency

### A key to its echo (ms)

`latency_ms`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 11.41 | 11.31 … 11.52 | 14.77 | 11.98 ± 2.66 | 6.99 … 33.98 | 800 (8) | 11.43, 11.25, 11.34, 11.59, 11.31, 11.32, 11.60, 11.50 |
| lokyr-b | 11.42 | 11.28 … 11.55 | 14.63 | 11.92 ± 2.52 | 7.11 … 35.62 | 800 (8) | 11.44, 11.61, 11.36, 11.31, 11.57, 11.37, 11.11, 11.53 |

### Keys of no echo seen (keys)

`missed_keys`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 8 (8) | 0, 0, 0, 0, 0, 0, 0, 0 |
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 8 (8) | 0, 0, 0, 0, 0, 0, 0, 0 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-latency-lokyr-a` | 10:35:55 | 22 | done | 1.92 → 1.72 | WindowServer 26 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 20 % |
| `c1-r1-latency-lokyr-b` | 10:36:19 | 22 | done | 1.74 → 1.65 | WindowServer 28 %, MenuBarAgent 15 %, BiomeAgent 12 % |
| `c2-r1-latency-lokyr-b` | 10:36:42 | 22 | done | 1.60 → 1.80 | BiomeAgent 31 %, WindowServer 25 %, wdavdaemon 18 % |
| `c2-r1-latency-lokyr-a` | 10:37:06 | 22 | done | 1.80 → 1.51 | WindowServer 24 %, MenuBarAgent 16 %, Code Helper (Renderer) 13 % |
| `c3-r1-latency-lokyr-a` | 10:37:30 | 22 | done | 1.51 → 1.43 | WindowServer 26 %, Code Helper (Renderer) 23 %, MenuBarAgent 19 % |
| `c3-r1-latency-lokyr-b` | 10:37:54 | 22 | done | 1.32 → 1.25 | Code Helper (Renderer) 104 %, WindowServer 27 %, MenuBarAgent 17 % |
| `c4-r1-latency-lokyr-b` | 10:38:17 | 22 | done | 1.25 → 1.12 | WindowServer 25 %, Code Helper (Renderer) 20 %, MenuBarAgent 18 % |
| `c4-r1-latency-lokyr-a` | 10:38:41 | 22 | done | 1.12 → 1.27 | WindowServer 26 %, BiomeAgent 15 %, coreaudiod 12 % |
| `c5-r1-latency-lokyr-a` | 10:39:05 | 22 | done | 1.25 → 1.05 | WindowServer 22 %, BiomeAgent 18 %, coreaudiod 12 % |
| `c5-r1-latency-lokyr-b` | 10:39:29 | 22 | done | 1.05 → 1.09 | WindowServer 26 %, MenuBarAgent 18 %, wdavdaemon_unprivileged 14 % |
| `c6-r1-latency-lokyr-b` | 10:39:53 | 22 | done | 1.09 → 4.26 | wdavdaemon_unprivileged 89 %, WindowServer 25 %, wdavdaemon 18 % |
| `c6-r1-latency-lokyr-a` | 10:40:16 | 22 | done | 4.26 → 3.47 | wdavdaemon_unprivileged 66 %, wdavdaemon 25 %, WindowServer 24 % |
| `c7-r1-latency-lokyr-a` | 10:40:40 | 22 | done | 3.27 → 3.00 | WindowServer 25 %, MenuBarAgent 18 %, Code Helper (Renderer) 16 % |
| `c7-r1-latency-lokyr-b` | 10:41:04 | 22 | done | 3.00 → 2.58 | WindowServer 27 %, Code Helper (Renderer) 21 %, MenuBarAgent 18 % |
| `c8-r1-latency-lokyr-b` | 10:41:28 | 22 | done | 2.58 → 2.01 | WindowServer 26 %, Code Helper (Renderer) 21 %, MenuBarAgent 17 % |
| `c8-r1-latency-lokyr-a` | 10:41:51 | 22 | done | 2.00 → 1.72 | WindowServer 25 %, wdavdaemon_unprivileged 23 %, BiomeAgent 22 % |
