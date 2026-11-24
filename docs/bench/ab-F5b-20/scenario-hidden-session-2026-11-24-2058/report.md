# ttybench session scenario-hidden-session-2026-11-24-2058

Measured from 2026-11-24 20:58 to 21:06 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-b | 1.4.0 (202611241345) |
| lokyr-b-hidden-session | 1.4.0 (202611241345) |

## Budgets

5 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| idle · Interrupt wakeups a second, its mean | lokyr-b | 0.3300 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b-hidden-session | 0.3301 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b-hidden-session against lokyr-b | +0.0001 /s (+0.0 %), 95 % -0.1237 … +0.1162 | ≤ +0.1 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b | 0 /s | ≤ 0 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b-hidden-session | 0 /s | ≤ 0 /s | ✔ held |

## A/B: lokyr-b-hidden-session against lokyr-b

Each metric's median of A, lokyr-b, and of B, lokyr-b-hidden-session; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| idle · Its CPU (%) | 0.0576 | 0.0571 | -0.0005 (-0.9 %) | -0.0057 … +0.0043 | within the noise |
| idle · Package idle wakeups a second (/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| idle · Interrupt wakeups a second (/s) | 0.0000 | 0.0000 | +0.0000 | +0.0000 … +0.0000 | within the noise |
| idle · The power that the kernel bills it (mW) | 0.0636 | 0.0636 | +0.0000 (+0.0 %) | -0.0024 … +0.0065 | within the noise |
| memory · Its memory's footprint (MiB) | 55.37 | 94.10 | +38.73 (+69.9 %) | +38.45 … +39.02 | **worse** |
| memory · Its memory's peak footprint (MiB) | 212.2 | 251.1 | +38.9 (+18.3 %) | +38.6 … +39.3 | **worse** |

## idle

### Its CPU (%)

`cpu_percent`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.0576 | 0.0525 … 0.0600 | 0.0669 | 0.0515 ± 0.0148 | 0.0189 … 0.0884 | 120 (4) | 0.0566, 0.0560, 0.0582, 0.0585 |
| lokyr-b-hidden-session | 0.0571 | 0.0527 … 0.0590 | 0.0687 | 0.0510 ± 0.0147 | 0.0240 … 0.0903 | 120 (4) | 0.0574, 0.0569, 0.0583, 0.0531 |

### Package idle wakeups a second (/s)

`package_idle_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 (4) | 0, 0, 0, 0 |
| lokyr-b-hidden-session | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 (4) | 0, 0, 0, 0 |

### Interrupt wakeups a second (/s)

`interrupt_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.0000 | 0.0000 … 0.0000 | 0.9938 | 0.3300 ± 0.4687 | 0.0000 … 0.9980 | 120 (4) | 0.0000, 0.0000, 0.0000, 0.0000 |
| lokyr-b-hidden-session | 0.0000 | 0.0000 … 0.0000 | 0.9942 | 0.3301 ± 0.4688 | 0.0000 … 0.9982 | 120 (4) | 0.0000, 0.0000, 0.0000, 0.0000 |

### The power that the kernel bills it (mW)

`power_mw`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.0636 | 0.0615 … 0.0657 | 0.0886 | 0.0610 ± 0.0177 | 0.0289 … 0.1130 | 120 (4) | 0.0616, 0.0647, 0.0641, 0.0631 |
| lokyr-b-hidden-session | 0.0636 | 0.0623 … 0.0694 | 0.0956 | 0.0629 ± 0.0219 | 0.0289 … 0.1599 | 120 (4) | 0.0642, 0.0636, 0.0627, 0.0662 |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 55.37 | 55.24 … 55.56 | 55.54 | 55.38 ± 0.14 | 55.24 … 55.56 | 4 (4) | 55.42, 55.56, 55.31, 55.24 |
| lokyr-b-hidden-session | 94.10 | 93.94 … 94.33 | 94.32 | 94.12 ± 0.20 | 93.94 … 94.33 | 4 (4) | 94.33, 93.96, 94.24, 93.94 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 212.2 | 211.8 … 212.4 | 212.4 | 212.1 ± 0.2 | 211.8 … 212.4 | 4 (4) | 212.2, 212.4, 212.1, 211.8 |
| lokyr-b-hidden-session | 251.1 | 250.9 … 251.3 | 251.3 | 251.1 ± 0.2 | 250.9 … 251.3 | 4 (4) | 251.3, 250.9, 251.2, 250.9 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-idle-lokyr-b` | 20:58:30 | 51 | done | 2.25 → 2.53 | WindowServer 33 %, wdavdaemon_unprivileged 26 %, Code Helper (Renderer) 20 % |
| `c1-r1-idle-lokyr-b-hidden-session` | 20:59:23 | 51 | done | 2.41 → 2.54 | Code Helper (Renderer) 99 %, WindowServer 34 %, wdavdaemon_unprivileged 34 % |
| `c1-r2-memory-lokyr-b` | 21:00:16 | 4 | done | 2.54 → 2.42 | duetexpertd 67 %, WindowServer 32 %, Code Helper (Renderer) 20 % |
| `c1-r2-memory-lokyr-b-hidden-session` | 21:00:22 | 4 | done | 2.42 → 2.94 | WindowServer 36 %, Code Helper (Renderer) 21 %, MenuBarAgent 20 % |
| `c2-r1-idle-lokyr-b-hidden-session` | 21:00:28 | 51 | done | 3.19 → 2.71 | WindowServer 32 %, wdavdaemon_unprivileged 24 %, Code Helper (Renderer) 20 % |
| `c2-r1-idle-lokyr-b` | 21:01:22 | 51 | done | 2.71 → 3.14 | Code Helper (Renderer) 94 %, WindowServer 31 %, BiomeAgent 18 % |
| `c2-r2-memory-lokyr-b-hidden-session` | 21:02:15 | 4 | done | 3.14 → 2.97 | WindowServer 32 %, wdavdaemon_unprivileged 21 %, Code Helper (Renderer) 20 % |
| `c2-r2-memory-lokyr-b` | 21:02:21 | 4 | done | 2.97 → 3.05 | WindowServer 30 %, Code Helper (Renderer) 20 %, wdavdaemon_unprivileged 19 % |
| `c3-r1-idle-lokyr-b` | 21:02:27 | 51 | done | 3.05 → 3.43 | Code Helper (Renderer) 103 %, WindowServer 30 %, wdavdaemon_unprivileged 23 % |
| `c3-r1-idle-lokyr-b-hidden-session` | 21:03:20 | 51 | done | 3.43 → 10.86 | WindowServer 32 %, wdavdaemon_unprivileged 22 %, Code Helper (Renderer) 20 % |
| `c3-r2-memory-lokyr-b` | 21:04:14 | 4 | done | 10.23 → 10.23 | WindowServer 30 %, MenuBarAgent 17 %, Code Helper (Renderer) 16 % |
| `c3-r2-memory-lokyr-b-hidden-session` | 21:04:20 | 4 | done | 9.57 → 9.12 | Code Helper (Renderer) 94 %, WindowServer 31 %, wdavdaemon_unprivileged 18 % |
| `c4-r1-idle-lokyr-b-hidden-session` | 21:04:26 | 51 | done | 9.12 → 5.62 | wdavdaemon_unprivileged 126 %, WindowServer 35 %, wdavdaemon 30 % |
| `c4-r1-idle-lokyr-b` | 21:05:19 | 51 | done | 5.65 → 3.86 | wdavdaemon_unprivileged 51 %, Code Helper (Renderer) 46 %, WindowServer 32 % |
| `c4-r2-memory-lokyr-b-hidden-session` | 21:06:12 | 4 | done | 3.86 → 4.19 | WindowServer 32 %, wdavdaemon_unprivileged 28 %, Code Helper (Renderer) 19 % |
| `c4-r2-memory-lokyr-b` | 21:06:18 | 4 | done | 4.09 → 4.09 | Code Helper (Renderer) 55 %, WindowServer 32 %, wdavdaemon_unprivileged 21 % |
