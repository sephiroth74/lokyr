# ttybench session scenario-broadcast-2026-11-24-2008

Measured from 2026-11-24 20:09 to 20:15 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-b | 1.4.0 (202611241345) |
| lokyr-b-broadcast | 1.4.0 (202611241345) |

## Budgets

4 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| latency · A key to its echo, its median, rows 40 | lokyr-b | 16.95 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its median, rows 20 | lokyr-b-broadcast | 17.33 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its p99, rows 40 | lokyr-b | 30.38 ms | ≤ 75 ms | ✔ held |
| latency · A key to its echo, its p99, rows 20 | lokyr-b-broadcast | 28.22 ms | ≤ 75 ms | ✔ held |

## A/B: lokyr-b-broadcast against lokyr-b

Each metric's median of A, lokyr-b, and of B, lokyr-b-broadcast; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| No metric of both | | | | | |

## latency

### A key to its echo (ms), rows 40

`latency_ms`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 16.95 | 16.34 … 17.75 | 25.80 | 17.78 ± 4.90 | 7.35 … 40.15 | 800 (8) | 16.79, 16.69, 16.93, 16.46, 16.04, 17.97, 18.20, 17.17 |

### A key to its echo (ms), rows 20

`latency_ms`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 20, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b-broadcast | 17.33 | 16.51 … 18.10 | 25.75 | 17.79 ± 4.91 | 5.55 … 33.68 | 800 (8) | 17.15, 17.39, 18.82, 17.89, 17.26, 18.23, 15.78, 16.54 |

### Keys of no echo seen (keys), rows 40

`missed_keys`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 8 (8) | 0, 0, 0, 0, 0, 0, 0, 0 |

### Keys of no echo seen (keys), rows 20

`missed_keys`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 100, phase random, refresh_hz 60.0, rounds 1, rows 20, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b-broadcast | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 8 (8) | 0, 0, 0, 0, 0, 0, 0, 0 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-latency-lokyr-b` | 20:09:18 | 21 | done | 2.62 → 2.46 | Code Helper (Renderer) 102 %, WindowServer 45 %, BiomeAgent 26 % |
| `c1-r1-latency-lokyr-b-broadcast` | 20:09:42 | 22 | done | 2.46 → 3.54 | wdavdaemon_unprivileged 46 %, WindowServer 45 %, Code Helper (Renderer) 21 % |
| `c2-r1-latency-lokyr-b-broadcast` | 20:10:06 | 22 | done | 3.54 → 3.92 | WindowServer 48 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 20 % |
| `c2-r1-latency-lokyr-b` | 20:10:29 | 22 | done | 3.92 → 3.07 | Code Helper (Renderer) 107 %, WindowServer 49 %, Code Helper 19 % |
| `c3-r1-latency-lokyr-b` | 20:10:53 | 21 | done | 3.31 → 2.70 | WindowServer 54 %, wdavdaemon_unprivileged 22 %, Code Helper (Renderer) 21 % |
| `c3-r1-latency-lokyr-b-broadcast` | 20:11:16 | 21 | done | 2.70 → 3.23 | WindowServer 51 %, Google Chrome Helper (Aperitif Renderer) 49 %, BiomeAgent 26 % |
| `c4-r1-latency-lokyr-b-broadcast` | 20:11:40 | 21 | done | 3.23 → 3.21 | WindowServer 47 %, Code Helper (Renderer) 35 %, wdavdaemon_unprivileged 19 % |
| `c4-r1-latency-lokyr-b` | 20:12:03 | 21 | done | 3.03 → 2.44 | WindowServer 53 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 20 % |
| `c5-r1-latency-lokyr-b` | 20:12:27 | 21 | done | 2.44 → 3.01 | WindowServer 56 %, Code Helper (Renderer) 48 %, Code Helper (Renderer) 22 % |
| `c5-r1-latency-lokyr-b-broadcast` | 20:12:50 | 21 | done | 3.01 → 3.16 | WindowServer 47 %, BiomeAgent 31 %, Code Helper 17 % |
| `c6-r1-latency-lokyr-b-broadcast` | 20:13:14 | 21 | done | 2.99 → 2.78 | Code Helper (Renderer) 102 %, WindowServer 46 %, wdavdaemon_unprivileged 19 % |
| `c6-r1-latency-lokyr-b` | 20:13:37 | 21 | done | 2.78 → 3.13 | WindowServer 50 %, BiomeAgent 24 %, Code Helper (Renderer) 22 % |
| `c7-r1-latency-lokyr-b` | 20:14:01 | 21 | done | 3.13 → 2.90 | WindowServer 43 %, Code Helper 19 %, MenuBarAgent 17 % |
| `c7-r1-latency-lokyr-b-broadcast` | 20:14:24 | 21 | done | 2.90 → 2.72 | wdavdaemon_unprivileged 122 %, Code Helper (Renderer) 79 %, wdavdaemon 34 % |
| `c8-r1-latency-lokyr-b-broadcast` | 20:14:48 | 21 | done | 2.67 → 3.21 | WindowServer 56 %, wdavdaemon_unprivileged 33 %, MenuBarAgent 21 % |
| `c8-r1-latency-lokyr-b` | 20:15:11 | 21 | done | 3.21 → 2.76 | WindowServer 53 %, Code Helper (Renderer) 23 %, MenuBarAgent 19 % |
