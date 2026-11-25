# ttybench session ab-3-latency-2026-11-24-1827

Measured from 2026-11-24 18:27 to 18:33 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## Budgets

4 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| latency · A key to its echo, its median | lokyr-a | 16.81 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its median | lokyr-b | 16.34 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its p99 | lokyr-a | 31.16 ms | ≤ 75 ms | ✔ held |
| latency · A key to its echo, its p99 | lokyr-b | 27.97 ms | ≤ 75 ms | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| latency · A key to its echo (ms) | 16.81 | 16.34 | -0.47 (-2.8 %) | -1.57 … +0.70 | within the noise |
| latency · Keys of no echo seen (keys) | 0 | 0 | +0 | +0 … +0 | within the noise |

## latency

### A key to its echo (ms)

`latency_ms`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 16.81 | 15.99 … 17.59 | 25.83 | 17.33 ± 5.20 | 6.07 … 35.72 | 800 (8) | 13.85, 16.29, 17.32, 17.54, 16.90, 17.23, 17.34, 18.08 |
| lokyr-b | 16.34 | 15.73 … 17.29 | 25.30 | 16.88 ± 5.04 | 5.94 … 32.58 | 800 (8) | 14.28, 15.98, 15.80, 18.43, 16.44, 16.12, 17.85, 16.52 |

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
| `c1-r1-latency-lokyr-a` | 18:27:46 | 21 | done | 6.87 → 6.22 | epsext 42 %, WindowServer 41 %, Python 35 % |
| `c1-r1-latency-lokyr-b` | 18:28:09 | 21 | done | 6.22 → 5.35 | Code Helper (Renderer) 105 %, WindowServer 50 %, wdavdaemon_unprivileged 22 % |
| `c2-r1-latency-lokyr-b` | 18:28:33 | 21 | done | 5.08 → 4.91 | WindowServer 46 %, mds 22 %, BiomeAgent 21 % |
| `c2-r1-latency-lokyr-a` | 18:28:56 | 21 | done | 4.91 → 3.99 | WindowServer 54 %, MenuBarAgent 18 %, Code Helper (Renderer) 17 % |
| `c3-r1-latency-lokyr-a` | 18:29:19 | 21 | done | 3.99 → 3.84 | WindowServer 49 %, wdavdaemon_unprivileged 37 %, Code Helper (Renderer) 18 % |
| `c3-r1-latency-lokyr-b` | 18:29:43 | 21 | done | 3.54 → 3.35 | WindowServer 55 %, Code Helper (Renderer) 23 %, MenuBarAgent 17 % |
| `c4-r1-latency-lokyr-b` | 18:30:06 | 21 | done | 3.08 → 4.47 | Code Helper (Renderer) 99 %, WindowServer 51 %, MenuBarAgent 18 % |
| `c4-r1-latency-lokyr-a` | 18:30:30 | 21 | done | 4.47 → 3.93 | WindowServer 58 %, MenuBarAgent 19 %, Code Helper 15 % |
| `c5-r1-latency-lokyr-a` | 18:30:53 | 21 | done | 3.69 → 3.14 | WindowServer 52 %, Code Helper (Renderer) 20 %, wdavdaemon_unprivileged 18 % |
| `c5-r1-latency-lokyr-b` | 18:31:16 | 21 | done | 4.01 → 4.06 | WindowServer 48 %, Google Chrome Helper (Aperitif Renderer) 38 %, wdavdaemon_unprivileged 20 % |
| `c6-r1-latency-lokyr-b` | 18:31:40 | 21 | done | 4.06 → 5.19 | WindowServer 46 %, Code Helper (Renderer) 44 %, wdavdaemon_unprivileged 20 % |
| `c6-r1-latency-lokyr-a` | 18:32:03 | 21 | done | 5.19 → 5.19 | WindowServer 51 %, MenuBarAgent 19 %, Code Helper (Renderer) 18 % |
| `c7-r1-latency-lokyr-a` | 18:32:27 | 21 | done | 4.86 → 6.10 | jetbrains-toolbox 66 %, duetexpertd 60 %, linkd 56 % |
| `c7-r1-latency-lokyr-b` | 18:32:50 | 21 | done | 6.10 → 5.09 | WindowServer 48 %, BiomeAgent 25 %, siriactionsd 22 % |
| `c8-r1-latency-lokyr-b` | 18:33:14 | 21 | done | 5.09 → 5.47 | WindowServer 51 %, MenuBarAgent 20 %, Code Helper 19 % |
| `c8-r1-latency-lokyr-a` | 18:33:37 | 21 | done | 5.20 → 4.70 | WindowServer 49 %, BiomeAgent 23 %, wdavdaemon_unprivileged 22 % |
