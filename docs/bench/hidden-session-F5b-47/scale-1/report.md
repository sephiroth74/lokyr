# ttybench session scenario-hidden-session-memory-2026-11-25-1140

Measured from 2026-11-25 11:40 to 11:43 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-a-hidden-session | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611251020) |
| lokyr-b-hidden-session | 1.5.0 (202611251020) |

## A/B: lokyr-b-hidden-session against lokyr-a-hidden-session

Each metric's median of A, lokyr-a-hidden-session, and of B, lokyr-b-hidden-session; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| memory · Its memory's footprint (MiB) | 60.26 | 57.84 | -2.42 (-4.0 %) | -3.16 … -2.10 | better |
| memory · Its memory's peak footprint (MiB) | 221.4 | 228.0 | +6.6 (+3.0 %) | +5.2 … +9.9 | **worse** |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 1, settle_s 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 43.35 | 41.27 … 45.38 | 45.36 | 43.33 ± 2.28 | 41.27 … 45.38 | 4 (4) | 41.46, 45.24, 41.27, 45.38 |
| lokyr-a-hidden-session | 60.26 | 60.13 … 60.30 | 60.30 | 60.24 ± 0.08 | 60.13 … 60.30 | 4 (4) | 60.22, 60.13, 60.30, 60.30 |
| lokyr-b | 45.24 | 41.60 … 45.36 | 45.34 | 44.36 ± 1.84 | 41.60 … 45.36 | 4 (4) | 45.36, 41.60, 45.24, 45.24 |
| lokyr-b-hidden-session | 57.84 | 57.10 … 58.10 | 58.09 | 57.72 ± 0.46 | 57.10 … 58.10 | 4 (4) | 57.63, 58.10, 58.05, 57.10 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 1, settle_s 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 206.3 | 206.2 … 206.9 | 206.8 | 206.4 ± 0.3 | 206.2 … 206.9 | 4 (4) | 206.9, 206.2, 206.2, 206.3 |
| lokyr-a-hidden-session | 221.4 | 218.0 … 221.6 | 221.6 | 220.6 ± 1.7 | 218.0 … 221.6 | 4 (4) | 221.5, 221.3, 221.6, 218.0 |
| lokyr-b | 206.4 | 206.2 … 206.7 | 206.7 | 206.4 ± 0.3 | 206.2 … 206.7 | 4 (4) | 206.7, 206.5, 206.2, 206.2 |
| lokyr-b-hidden-session | 228.0 | 226.6 … 228.4 | 228.4 | 227.7 ± 0.8 | 226.6 … 228.4 | 4 (4) | 227.7, 226.6, 228.2, 228.4 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-memory-lokyr-a` | 11:40:13 | 11 | done | 2.07 → 2.61 | Google Chrome Helper (Aperitif Renderer) 28 %, studio 27 %, WindowServer 25 % |
| `c1-r1-memory-lokyr-a-hidden-session` | 11:40:26 | 11 | done | 2.48 → 2.40 | WindowServer 31 %, studio 19 %, wdavdaemon_unprivileged 18 % |
| `c1-r1-memory-lokyr-b` | 11:40:39 | 11 | done | 2.40 → 2.72 | Code Helper (Renderer) 75 %, WindowServer 29 %, studio 22 % |
| `c1-r1-memory-lokyr-b-hidden-session` | 11:40:53 | 11 | done | 2.72 → 3.18 | launchd 34 %, WindowServer 31 %, studio 28 % |
| `c2-r1-memory-lokyr-a-hidden-session` | 11:41:06 | 11 | done | 3.65 → 4.05 | WindowServer 28 %, Code Helper (Renderer) 23 %, studio 21 % |
| `c2-r1-memory-lokyr-b` | 11:41:18 | 11 | done | 4.05 → 3.74 | WindowServer 31 %, studio 23 %, Code Helper (Renderer) 23 % |
| `c2-r1-memory-lokyr-b-hidden-session` | 11:41:31 | 11 | done | 3.44 → 2.99 | WindowServer 30 %, studio 25 %, Code Helper (Renderer) 20 % |
| `c2-r1-memory-lokyr-a` | 11:41:44 | 11 | done | 2.99 → 2.73 | WindowServer 26 %, Code Helper (Renderer) 21 %, MenuBarAgent 15 % |
| `c3-r1-memory-lokyr-b` | 11:41:57 | 11 | done | 2.73 → 2.46 | Code Helper (Renderer) 31 %, epsext 29 %, wdavdaemon 27 % |
| `c3-r1-memory-lokyr-b-hidden-session` | 11:42:10 | 11 | done | 2.26 → 2.28 | Code Helper (Renderer) 74 %, WindowServer 27 %, MenuBarAgent 16 % |
| `c3-r1-memory-lokyr-a` | 11:42:23 | 11 | done | 2.28 → 2.38 | launchd 39 %, WindowServer 31 %, wdavdaemon_unprivileged 21 % |
| `c3-r1-memory-lokyr-a-hidden-session` | 11:42:36 | 11 | done | 2.19 → 1.93 | WindowServer 27 %, Code Helper (Renderer) 20 %, MenuBarAgent 16 % |
| `c4-r1-memory-lokyr-b-hidden-session` | 11:42:49 | 11 | done | 1.93 → 1.71 | WindowServer 27 %, studio 21 %, MenuBarAgent 17 % |
| `c4-r1-memory-lokyr-a` | 11:43:02 | 11 | done | 1.73 → 1.62 | Code Helper (Renderer) 44 %, WindowServer 27 %, MenuBarAgent 15 % |
| `c4-r1-memory-lokyr-a-hidden-session` | 11:43:14 | 11 | done | 1.62 → 2.10 | wdavdaemon_unprivileged 94 %, Code Helper (Renderer) 50 %, WindowServer 28 % |
| `c4-r1-memory-lokyr-b` | 11:43:27 | 11 | done | 2.10 → 2.09 | Code Helper (Renderer) 80 %, wdavdaemon_unprivileged 56 %, WindowServer 27 % |
