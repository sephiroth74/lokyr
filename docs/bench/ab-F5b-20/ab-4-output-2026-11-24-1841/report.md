# ttybench session ab-4-output-2026-11-24-1841

Measured from 2026-11-24 18:41 to 18:44 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 24 runs: 24 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## Budgets

0 held, 2 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| throughput · Its rate to the quiet, its median, text plain | lokyr-a | 236.6 MiB/s | ≥ 275 MiB/s | ✘ not held |
| throughput · Its rate to the quiet, its median, text plain | lokyr-b | 236.7 MiB/s | ≥ 275 MiB/s | ✘ not held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| throughput · Its rate to the last write (MiB/s), text plain | 236.7 | 236.8 | +0.1 (+0.0 %) | -65.4 … +66.6 | within the noise |
| throughput · Its rate to the last write (MiB/s), text colors | 224.5 | 206.2 | -18.4 (-8.2 %) | -71.9 … +54.3 | within the noise |
| throughput · Its rate to the last write (MiB/s), text wide | 254.5 | 254.6 | +0.1 (+0.0 %) | -69.8 … +72.6 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text plain | 236.6 | 236.7 | +0.1 (+0.0 %) | -65.3 … +66.5 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text colors | 224.4 | 206.1 | -18.3 (-8.2 %) | -71.8 … +54.2 | within the noise |
| throughput · Its rate to the quiet (MiB/s), text wide | 254.4 | 254.5 | +0.1 (+0.0 %) | -69.7 … +72.5 | within the noise |

## throughput

### Its rate to the last write (MiB/s), text plain

`written_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 236.7 | 208.5 … 302.2 | 295.8 | 246.0 ± 43.8 | 208.5 … 302.2 | 4 (4) | 208.5, 214.0, 259.4, 302.2 |
| lokyr-b | 236.8 | 222.3 … 301.3 | 293.5 | 249.3 ± 36.7 | 222.3 … 301.3 | 4 (4) | 222.3, 224.6, 249.0, 301.3 |

### Its rate to the last write (MiB/s), text colors

`written_mib_s`. Conditions: mib 1024, rounds 1, text colors.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 224.5 | 196.1 … 278.0 | 271.2 | 230.8 ± 34.9 | 196.1 … 278.0 | 4 (4) | 196.1, 232.6, 216.4, 278.0 |
| lokyr-b | 206.2 | 194.9 … 278.8 | 268.9 | 221.5 ± 38.9 | 194.9 … 278.8 | 4 (4) | 199.7, 194.9, 212.6, 278.8 |

### Its rate to the last write (MiB/s), text wide

`written_mib_s`. Conditions: mib 1024, rounds 1, text wide.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 254.5 | 214.9 … 323.7 | 314.3 | 261.9 ± 45.5 | 214.9 … 323.7 | 4 (4) | 214.9, 261.0, 248.1, 323.7 |
| lokyr-b | 254.6 | 222.5 … 320.7 | 310.9 | 263.1 ± 41.3 | 222.5 … 320.7 | 4 (4) | 222.5, 253.8, 255.3, 320.7 |

### Its rate to the quiet (MiB/s), text plain

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text plain.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 236.6 | 208.5 … 302.0 | 295.6 | 245.9 ± 43.8 | 208.5 … 302.0 | 4 (4) | 208.5, 213.9, 259.3, 302.0 |
| lokyr-b | 236.7 | 222.3 … 301.1 | 293.3 | 249.2 ± 36.6 | 222.3 … 301.1 | 4 (4) | 222.3, 224.6, 248.9, 301.1 |

### Its rate to the quiet (MiB/s), text colors

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text colors.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 224.4 | 196.0 … 277.9 | 271.1 | 230.7 ± 34.8 | 196.0 … 277.9 | 4 (4) | 196.0, 232.6, 216.3, 277.9 |
| lokyr-b | 206.1 | 194.8 … 278.7 | 268.7 | 221.4 ± 38.9 | 194.8 … 278.7 | 4 (4) | 199.7, 194.8, 212.5, 278.7 |

### Its rate to the quiet (MiB/s), text wide

`quiet_mib_s`. Conditions: mib 1024, rounds 1, text wide.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 254.4 | 214.8 … 323.5 | 314.1 | 261.8 ± 45.5 | 214.8 … 323.5 | 4 (4) | 214.8, 260.8, 248.0, 323.5 |
| lokyr-b | 254.5 | 222.5 … 320.4 | 310.6 | 263.0 ± 41.2 | 222.5 … 320.4 | 4 (4) | 222.5, 253.7, 255.2, 320.4 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-throughput-lokyr-a` | 18:41:17 | 7 | done | 4.92 → 5.00 | wdavdaemon_enterprise 101 %, Code Helper (Renderer) 98 %, epsext 48 % |
| `c1-r1-throughput-lokyr-b` | 18:41:26 | 6 | done | 5.00 → 5.95 | epsext 48 %, Code Helper (Renderer) 46 %, Python 40 % |
| `c1-r2-throughput-lokyr-a` | 18:41:35 | 7 | done | 5.95 → 7.40 | epsext 45 %, Python 34 %, WindowServer 30 % |
| `c1-r2-throughput-lokyr-b` | 18:41:44 | 7 | done | 6.88 → 6.73 | epsext 47 %, Python 34 %, WindowServer 30 % |
| `c1-r3-throughput-lokyr-a` | 18:41:52 | 6 | done | 6.67 → 6.62 | epsext 48 %, Python 42 %, WindowServer 30 % |
| `c1-r3-throughput-lokyr-b` | 18:42:01 | 6 | done | 6.62 → 6.57 | Python 40 %, epsext 36 %, WindowServer 28 % |
| `c2-r1-throughput-lokyr-b` | 18:42:09 | 6 | done | 6.57 → 6.45 | epsext 47 %, Python 36 %, WindowServer 29 % |
| `c2-r1-throughput-lokyr-a` | 18:42:18 | 7 | done | 6.33 → 6.22 | epsext 48 %, Python 42 %, WindowServer 26 % |
| `c2-r2-throughput-lokyr-b` | 18:42:26 | 7 | done | 6.22 → 7.29 | epsext 30 %, WindowServer 28 %, BiomeAgent 20 % |
| `c2-r2-throughput-lokyr-a` | 18:42:35 | 6 | done | 7.29 → 7.35 | mds 39 %, epsext 35 %, WindowServer 32 % |
| `c2-r3-throughput-lokyr-b` | 18:42:43 | 6 | done | 6.84 → 7.65 | Python 33 %, epsext 30 %, WindowServer 28 % |
| `c2-r3-throughput-lokyr-a` | 18:42:51 | 6 | done | 7.65 → 8.16 | epsext 34 %, WindowServer 29 %, Python 28 % |
| `c3-r1-throughput-lokyr-a` | 18:42:59 | 6 | done | 7.67 → 7.77 | epsext 36 %, WindowServer 31 %, Python 27 % |
| `c3-r1-throughput-lokyr-b` | 18:43:07 | 6 | done | 7.77 → 7.29 | Python 40 %, epsext 34 %, WindowServer 31 % |
| `c3-r2-throughput-lokyr-a` | 18:43:14 | 6 | done | 7.29 → 7.02 | epsext 36 %, Python 31 %, WindowServer 30 % |
| `c3-r2-throughput-lokyr-b` | 18:43:23 | 7 | done | 7.34 → 7.87 | mds 59 %, Python 41 %, WindowServer 33 % |
| `c3-r3-throughput-lokyr-a` | 18:43:32 | 6 | done | 7.87 → 8.05 | epsext 35 %, Python 35 %, WindowServer 30 % |
| `c3-r3-throughput-lokyr-b` | 18:43:39 | 6 | done | 8.05 → 8.69 | mds 62 %, WindowServer 30 %, mdworker_shared 19 % |
| `c4-r1-throughput-lokyr-b` | 18:43:47 | 5 | done | 8.69 → 8.67 | mds 53 %, WindowServer 36 %, Code Helper (Renderer) 18 % |
| `c4-r1-throughput-lokyr-a` | 18:43:54 | 5 | done | 8.67 → 8.30 | WindowServer 36 %, wdavdaemon_unprivileged 23 %, Code Helper (Renderer) 19 % |
| `c4-r2-throughput-lokyr-b` | 18:44:02 | 5 | done | 8.30 → 7.87 | WindowServer 33 %, wdavdaemon_unprivileged 26 %, BiomeAgent 18 % |
| `c4-r2-throughput-lokyr-a` | 18:44:09 | 5 | done | 7.64 → 7.67 | WindowServer 32 %, wdavdaemon_unprivileged 19 %, BiomeAgent 18 % |
| `c4-r3-throughput-lokyr-b` | 18:44:17 | 5 | done | 7.67 → 7.30 | WindowServer 34 %, BiomeAgent 21 %, mdworker_shared 15 % |
| `c4-r3-throughput-lokyr-a` | 18:44:24 | 5 | done | 7.11 → 6.78 | WindowServer 36 %, wdavdaemon_unprivileged 24 %, BiomeAgent 21 % |
