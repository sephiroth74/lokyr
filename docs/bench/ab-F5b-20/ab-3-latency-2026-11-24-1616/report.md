# ttybench session ab-3-latency-2026-11-24-1616

Measured from 2026-11-24 16:16 to 16:22 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## Budgets

4 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| latency · A key to its echo, its median | lokyr-a | 14.12 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its median | lokyr-b | 15.84 ms | ≤ 33.3 ms | ✔ held |
| latency · A key to its echo, its p99 | lokyr-a | 27.97 ms | ≤ 75 ms | ✔ held |
| latency · A key to its echo, its p99 | lokyr-b | 27.53 ms | ≤ 75 ms | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| latency · A key to its echo (ms) | 14.12 | 15.84 | +1.71 (+12.1 %) | -0.10 … +4.75 | within the noise |
| latency · Keys of no echo seen (keys) | 0 | 0 | +0 | +0 … +0 | within the noise |

## latency

### A key to its echo (ms)

`latency_ms`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 200, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 14.12 | 11.07 … 15.88 | 22.70 | 14.18 ± 5.05 | 4.77 … 37.19 | 800 (4) | 11.45, 10.97, 15.85, 15.90 |
| lokyr-b | 15.84 | 15.58 … 16.12 | 23.74 | 15.85 ± 4.85 | 4.32 … 39.30 | 800 (4) | 15.63, 15.91, 15.90, 15.96 |

### Keys of no echo seen (keys)

`missed_keys`. Conditions: awake true, beside rest, cols 120, echo tty, jitter_ms 16.666667, keys 200, phase random, refresh_hz 60.0, rounds 1, rows 40, source window, spacing_ms 150.0, to process.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-latency-lokyr-a` | 16:16 | 37 | done |  |  |
| `c1-r1-latency-lokyr-b` | 16:17 | 37 | done |  |  |
| `c2-r1-latency-lokyr-b` | 16:18 | 37 | done |  |  |
| `c2-r1-latency-lokyr-a` | 16:18 | 37 | done |  |  |
| `c3-r1-latency-lokyr-a` | 16:19 | 37 | done |  |  |
| `c3-r1-latency-lokyr-b` | 16:20 | 37 | done |  |  |
| `c4-r1-latency-lokyr-b` | 16:20 | 37 | done |  |  |
| `c4-r1-latency-lokyr-a` | 16:21 | 37 | done |  |  |
