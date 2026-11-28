# ttybench session scenario-hidden-session-memory-2026-11-25-1147

Measured from 2026-11-25 11:47 to 11:50 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 120 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

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
| memory · Its memory's footprint (MiB) | 96.33 | 85.22 | -11.11 (-11.5 %) | -12.50 … -9.77 | better |
| memory · Its memory's peak footprint (MiB) | 253.5 | 267.1 | +13.6 (+5.4 %) | +6.2 … +16.2 | **worse** |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 1, settle_s 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 56.88 | 56.61 … 57.17 | 57.13 | 56.89 ± 0.23 | 56.61 … 57.17 | 4 (4) | 56.61, 56.88, 56.89, 57.17 |
| lokyr-a-hidden-session | 96.33 | 96.10 … 96.50 | 96.50 | 96.32 ± 0.21 | 96.10 … 96.50 | 4 (4) | 96.50, 96.49, 96.17, 96.10 |
| lokyr-b | 54.38 | 51.77 … 56.96 | 56.93 | 54.37 ± 2.91 | 51.77 … 56.96 | 4 (4) | 56.81, 56.96, 51.77, 51.94 |
| lokyr-b-hidden-session | 85.22 | 83.85 … 86.42 | 86.41 | 85.18 ± 1.37 | 83.85 … 86.42 | 4 (4) | 83.85, 86.42, 84.14, 86.30 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 1, settle_s 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 213.8 | 213.6 … 214.1 | 214.1 | 213.8 ± 0.2 | 213.6 … 214.1 | 4 (4) | 213.6, 213.8, 213.9, 214.1 |
| lokyr-a-hidden-session | 253.5 | 253.3 … 253.7 | 253.7 | 253.5 ± 0.2 | 253.3 … 253.7 | 4 (4) | 253.7, 253.7, 253.3, 253.4 |
| lokyr-b | 213.8 | 213.7 … 213.8 | 213.8 | 213.8 ± 0.0 | 213.7 … 213.8 | 4 (4) | 213.8, 213.8, 213.7, 213.8 |
| lokyr-b-hidden-session | 267.1 | 259.7 … 269.7 | 269.4 | 265.9 ± 4.3 | 259.7 … 269.7 | 4 (4) | 266.9, 269.7, 267.3, 259.7 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-memory-lokyr-a` | 11:47:18 | 11 | done | 3.56 → 3.32 | Code Helper (Renderer) 84 %, WindowServer 36 %, wdavdaemon_unprivileged 24 % |
| `c1-r1-memory-lokyr-a-hidden-session` | 11:47:31 | 11 | done | 3.30 → 3.33 | WindowServer 34 %, Code Helper (Renderer) 24 %, MenuBarAgent 22 % |
| `c1-r1-memory-lokyr-b` | 11:47:44 | 11 | done | 3.33 → 3.12 | WindowServer 31 %, MenuBarAgent 21 %, Code Helper (Renderer) 16 % |
| `c1-r1-memory-lokyr-b-hidden-session` | 11:47:57 | 11 | done | 2.87 → 2.95 | Code Helper (Renderer) 46 %, WindowServer 36 %, Code Helper (Renderer) 22 % |
| `c2-r1-memory-lokyr-a-hidden-session` | 11:48:09 | 11 | done | 2.95 → 3.06 | WindowServer 34 %, MenuBarAgent 23 %, Code Helper (Renderer) 18 % |
| `c2-r1-memory-lokyr-b` | 11:48:22 | 11 | done | 3.06 → 3.42 | WindowServer 32 %, wdavdaemon_unprivileged 28 %, MenuBarAgent 22 % |
| `c2-r1-memory-lokyr-b-hidden-session` | 11:48:35 | 11 | done | 3.54 → 3.71 | Code Helper (Renderer) 90 %, WindowServer 33 %, MenuBarAgent 19 % |
| `c2-r1-memory-lokyr-a` | 11:48:48 | 11 | done | 3.71 → 4.04 | WindowServer 24 %, jetbrains-toolbox 24 %, Code Helper 15 % |
| `c3-r1-memory-lokyr-b` | 11:49:01 | 11 | done | 3.88 → 3.51 | WindowServer 20 %, studio 11 % |
| `c3-r1-memory-lokyr-b-hidden-session` | 11:49:14 | 11 | done | 3.51 → 3.45 | WindowServer 35 %, Code Helper (Renderer) 22 %, MenuBarAgent 21 % |
| `c3-r1-memory-lokyr-a` | 11:49:27 | 11 | done | 3.33 → 2.98 | WindowServer 36 %, wdavdaemon_unprivileged 25 %, MenuBarAgent 22 % |
| `c3-r1-memory-lokyr-a-hidden-session` | 11:49:40 | 11 | done | 2.98 → 3.31 | Code Helper (Renderer) 42 %, WindowServer 36 %, MenuBarAgent 26 % |
| `c4-r1-memory-lokyr-b-hidden-session` | 11:49:53 | 11 | done | 3.31 → 3.03 | deleted 68 %, backupd-helper 58 %, mds 46 % |
| `c4-r1-memory-lokyr-a` | 11:50:06 | 11 | done | 3.11 → 3.15 | backupd-helper 63 %, WindowServer 27 %, studio 12 % |
| `c4-r1-memory-lokyr-a-hidden-session` | 11:50:18 | 11 | done | 3.15 → 3.05 | WindowServer 32 %, MenuBarAgent 25 %, Code Helper (Renderer) 16 % |
| `c4-r1-memory-lokyr-b` | 11:50:31 | 11 | done | 3.04 → 2.88 | WindowServer 32 %, wdavdaemon_unprivileged 25 %, MenuBarAgent 20 % |
