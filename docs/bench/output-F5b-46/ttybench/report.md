# ttybench session ab-4-output-large-2026-11-25-1210

Measured from 2026-11-25 12:10 to 12:11 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 120 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a-large | 1.4.0 (202611220721) |
| lokyr-b-large | 1.5.0 (202611250509) |

## A/B: lokyr-b-large against lokyr-a-large

Each metric's median of A, lokyr-a-large, and of B, lokyr-b-large; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| throughput · Its rate to the last write (MiB/s) | 294.9 | 294.6 | -0.4 (-0.1 %) | -11.9 … +20.4 | within the noise |
| throughput · Its rate to the quiet (MiB/s) | 294.8 | 294.5 | -0.3 (-0.1 %) | -11.9 … +20.4 | within the noise |

## throughput

### Its rate to the last write (MiB/s)

`written_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a-large | 294.9 | 274.1 … 297.1 | 296.8 | 290.3 ± 10.8 | 274.1 … 297.1 | 4 (4) | 274.1, 295.4, 297.1, 294.5 |
| lokyr-b-large | 294.6 | 283.0 … 296.2 | 296.2 | 292.1 ± 6.2 | 283.0 … 296.2 | 4 (4) | 283.0, 293.3, 295.8, 296.2 |

### Its rate to the quiet (MiB/s)

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a-large | 294.8 | 274.0 … 297.0 | 296.7 | 290.1 ± 10.8 | 274.0 … 297.0 | 4 (4) | 274.0, 295.3, 297.0, 294.4 |
| lokyr-b-large | 294.5 | 282.9 … 296.1 | 296.1 | 292.0 ± 6.2 | 282.9 … 296.1 | 4 (4) | 282.9, 293.2, 295.7, 296.1 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-throughput-lokyr-a-large` | 12:10:49 | 6 | done | 4.56 → 4.59 | wdavdaemon_enterprise 101 %, WindowServer 47 %, MenuBarAgent 21 % |
| `c1-r1-throughput-lokyr-b-large` | 12:10:57 | 5 | done | 4.79 → 4.79 | WindowServer 42 %, studio 22 %, MenuBarAgent 20 % |
| `c2-r1-throughput-lokyr-b-large` | 12:11:04 | 4 | done | 4.64 → 4.59 | WindowServer 43 %, MenuBarAgent 20 %, Code Helper (Renderer) 15 % |
| `c2-r1-throughput-lokyr-a-large` | 12:11:10 | 4 | done | 4.59 → 4.38 | WindowServer 39 %, Code Helper (Renderer) 13 % |
| `c3-r1-throughput-lokyr-a-large` | 12:11:17 | 4 | done | 4.38 → 4.43 | WindowServer 43 %, Code Helper (Renderer) 24 %, MenuBarAgent 22 % |
| `c3-r1-throughput-lokyr-b-large` | 12:11:23 | 4 | done | 4.40 → 4.29 | WindowServer 42 %, MenuBarAgent 17 %, Code Helper (Renderer) 11 % |
| `c4-r1-throughput-lokyr-b-large` | 12:11:29 | 4 | done | 4.29 → 4.50 | WindowServer 36 %, Code Helper 14 %, studio 12 % |
| `c4-r1-throughput-lokyr-a-large` | 12:11:36 | 4 | done | 4.50 → 4.30 | WindowServer 42 %, Code Helper (Renderer) 21 %, MenuBarAgent 20 % |
