# ttybench session ab-4-output-large-2026-11-28-1045

Measured from 2026-11-28 10:45 to 10:46 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a-large | 1.5.0 (202611250509) |
| lokyr-b-large | 1.5.0 (202611280854) |

## A/B: lokyr-b-large against lokyr-a-large

Each metric's median of A, lokyr-a-large, and of B, lokyr-b-large, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| throughput · Its rate to the last write (MiB/s) | 323.0 | 314.6 | -8.4 (-2.6 %) | -14.8 … -3.5 | **worse** |
| throughput · Its rate to the quiet (MiB/s) | 322.8 | 314.4 | -8.4 (-2.6 %) | -14.8 … -3.5 | **worse** |

## throughput

### Its rate to the last write (MiB/s)

`written_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a-large | 323.0 | 318.1 … 325.4 | 325.2 | 322.4 ± 3.2 | 318.1 … 325.4 | 4 (4) | 321.9, 324.2, 318.1, 325.4 |
| lokyr-b-large | 314.6 | 308.3 … 316.5 | 316.3 | 313.5 ± 3.7 | 308.3 … 316.5 | 4 (4) | 313.6, 315.6, 316.5, 308.3 |

### Its rate to the quiet (MiB/s)

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a-large | 322.8 | 317.9 … 325.2 | 325.0 | 322.2 ± 3.2 | 317.9 … 325.2 | 4 (4) | 321.7, 323.9, 317.9, 325.2 |
| lokyr-b-large | 314.4 | 308.1 … 316.3 | 316.1 | 313.3 ± 3.7 | 308.1 … 316.3 | 4 (4) | 313.4, 315.4, 316.3, 308.1 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-throughput-lokyr-a-large` | 10:45:33 | 6 | done | 3.00 → 3.08 | WindowServer 28 %, Code Helper (Renderer) 23 %, wdavdaemon_unprivileged 20 % |
| `c1-r1-throughput-lokyr-b-large` | 10:45:41 | 4 | done | 3.08 → 2.91 | WindowServer 24 %, wdavdaemon_unprivileged 17 %, Code Helper (Renderer) 17 % |
| `c2-r1-throughput-lokyr-b-large` | 10:45:47 | 6 | done | 2.76 → 2.78 | WindowServer 25 %, wdavdaemon_unprivileged 15 %, Code Helper (Renderer) 15 % |
| `c2-r1-throughput-lokyr-a-large` | 10:45:55 | 4 | done | 2.78 → 2.64 | WindowServer 27 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 18 % |
| `c3-r1-throughput-lokyr-a-large` | 10:46:02 | 4 | done | 2.42 → 2.42 | WindowServer 26 %, MenuBarAgent 12 % |
| `c3-r1-throughput-lokyr-b-large` | 10:46:08 | 4 | done | 2.31 → 2.37 | wdavdaemon_unprivileged 33 %, WindowServer 25 %, Code Helper (Renderer) 22 % |
| `c4-r1-throughput-lokyr-b-large` | 10:46:14 | 4 | done | 2.37 → 2.42 | WindowServer 23 %, wdavdaemon_unprivileged 21 %, Code Helper (Renderer) 17 % |
| `c4-r1-throughput-lokyr-a-large` | 10:46:21 | 4 | done | 2.42 → 3.34 | WindowServer 29 %, Code Helper (Renderer) 22 %, BiomeAgent 20 % |
