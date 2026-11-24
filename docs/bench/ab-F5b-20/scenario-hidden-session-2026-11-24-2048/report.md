# ttybench session scenario-hidden-session-2026-11-24-2048

Measured from 2026-11-24 20:48 to 20:56 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-b | 1.4.0 (202611241345) |
| lokyr-b-hidden-session | 1.4.0 (202611241345) |

## Budgets

3 held, 2 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| idle · Interrupt wakeups a second, its mean | lokyr-b | 0.348 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b-hidden-session | 1.348 /s | ≤ 0.5 /s | ✘ not held |
| idle · Interrupt wakeups a second, its mean | lokyr-b-hidden-session against lokyr-b | +1.000 /s (+287.4 %), 95 % +0.867 … +1.125 | ≤ +0.1 /s | ✘ not held |
| idle · Package idle wakeups a second, its mean | lokyr-b | 0 /s | ≤ 0 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b-hidden-session | 0 /s | ≤ 0 /s | ✔ held |

## A/B: lokyr-b-hidden-session against lokyr-b

Each metric's median of A, lokyr-b, and of B, lokyr-b-hidden-session; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| idle · Its CPU (%) | 0.059 | 0.130 | +0.071 (+119.8 %) | +0.065 … +0.075 | **worse** |
| idle · Package idle wakeups a second (/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| idle · Interrupt wakeups a second (/s) | 0.000 | 0.995 | +0.995 | +0.991 … +0.997 | **worse** |
| idle · The power that the kernel bills it (mW) | 0.067 | 0.219 | +0.152 (+227.7 %) | +0.148 … +0.160 | **worse** |
| memory · Its memory's footprint (MiB) | 55.28 | 86.36 | +31.09 (+56.2 %) | +30.75 … +31.24 | **worse** |
| memory · Its memory's peak footprint (MiB) | 212.1 | 242.9 | +30.8 (+14.5 %) | +30.5 … +31.0 | **worse** |

## idle

### Its CPU (%)

`cpu_percent`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.059 | 0.056 … 0.061 | 0.071 | 0.056 ± 0.021 | 0.023 … 0.224 | 120 (4) | 0.060, 0.059, 0.059, 0.057 |
| lokyr-b-hidden-session | 0.130 | 0.124 … 0.133 | 0.150 | 0.126 ± 0.021 | 0.076 … 0.215 | 120 (4) | 0.131, 0.129, 0.130, 0.126 |

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
| lokyr-b | 0.000 | 0.000 … 0.000 | 0.996 | 0.348 ± 0.493 | 0.000 … 1.988 | 120 (4) | 0.000, 0.000, 0.000, 0.000 |
| lokyr-b-hidden-session | 0.995 | 0.991 … 0.997 | 1.990 | 1.348 ± 0.495 | 0.000 … 1.996 | 120 (4) | 0.995, 0.997, 0.991, 0.992 |

### The power that the kernel bills it (mW)

`power_mw`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.067 | 0.064 … 0.069 | 0.093 | 0.067 ± 0.028 | 0.030 … 0.310 | 120 (4) | 0.067, 0.067, 0.067, 0.064 |
| lokyr-b-hidden-session | 0.219 | 0.215 … 0.226 | 0.307 | 0.231 ± 0.042 | 0.176 … 0.388 | 120 (4) | 0.217, 0.221, 0.218, 0.219 |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 55.28 | 55.13 … 55.46 | 55.45 | 55.28 ± 0.16 | 55.13 … 55.46 | 4 (4) | 55.39, 55.13, 55.46, 55.16 |
| lokyr-b-hidden-session | 86.36 | 86.03 … 86.39 | 86.39 | 86.29 ± 0.17 | 86.03 … 86.39 | 4 (4) | 86.35, 86.38, 86.03, 86.39 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 212.1 | 211.9 … 212.3 | 212.3 | 212.1 ± 0.2 | 211.9 … 212.3 | 4 (4) | 212.2, 211.9, 212.3, 212.0 |
| lokyr-b-hidden-session | 242.9 | 242.6 … 243.0 | 243.0 | 242.9 ± 0.2 | 242.6 … 243.0 | 4 (4) | 242.9, 243.0, 242.6, 242.8 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-idle-lokyr-b` | 20:48:55 | 51 | done | 2.20 → 2.03 | Code Helper (Renderer) 102 %, wdavdaemon_unprivileged 38 %, WindowServer 33 % |
| `c1-r1-idle-lokyr-b-hidden-session` | 20:49:48 | 51 | done | 1.95 → 2.59 | WindowServer 32 %, Code Helper (Renderer) 20 %, BiomeAgent 20 % |
| `c1-r2-memory-lokyr-b` | 20:50:41 | 4 | done | 2.59 → 3.11 | WindowServer 31 %, BiomeAgent 20 %, Code Helper (Renderer) 19 % |
| `c1-r2-memory-lokyr-b-hidden-session` | 20:50:47 | 4 | done | 2.94 → 2.94 | WindowServer 35 %, Code Helper (Renderer) 19 %, MenuBarAgent 14 % |
| `c2-r1-idle-lokyr-b-hidden-session` | 20:50:53 | 51 | done | 2.78 → 2.17 | WindowServer 31 %, Code Helper (Renderer) 17 %, BiomeAgent 17 % |
| `c2-r1-idle-lokyr-b` | 20:51:46 | 51 | done | 2.17 → 2.52 | Code Helper (Renderer) 106 %, Code Helper (Renderer) 70 %, WindowServer 33 % |
| `c2-r2-memory-lokyr-b-hidden-session` | 20:52:40 | 4 | done | 2.52 → 2.48 | duetexpertd 62 %, WindowServer 36 %, wdavdaemon_unprivileged 26 % |
| `c2-r2-memory-lokyr-b` | 20:52:46 | 4 | done | 2.48 → 2.44 | WindowServer 32 %, MenuBarAgent 17 %, Code Helper (Renderer) 12 % |
| `c3-r1-idle-lokyr-b` | 20:52:52 | 51 | done | 2.44 → 2.53 | Code Helper (Renderer) 46 %, Code Helper (Renderer) 41 %, WindowServer 34 % |
| `c3-r1-idle-lokyr-b-hidden-session` | 20:53:45 | 51 | done | 2.53 → 2.51 | Code Helper (Renderer) 106 %, WindowServer 34 %, wdavdaemon_unprivileged 21 % |
| `c3-r2-memory-lokyr-b` | 20:54:38 | 4 | done | 2.71 → 3.69 | WindowServer 32 %, Code Helper (Renderer) 21 %, wdavdaemon_unprivileged 16 % |
| `c3-r2-memory-lokyr-b-hidden-session` | 20:54:44 | 4 | done | 3.69 → 3.56 | WindowServer 33 %, wdavdaemon_unprivileged 28 %, Code Helper (Renderer) 21 % |
| `c4-r1-idle-lokyr-b-hidden-session` | 20:54:50 | 51 | done | 3.56 → 3.10 | wdavdaemon_unprivileged 68 %, WindowServer 35 %, Code Helper (Renderer) 20 % |
| `c4-r1-idle-lokyr-b` | 20:55:43 | 51 | done | 3.01 → 2.48 | Code Helper (Renderer) 44 %, WindowServer 33 %, wdavdaemon_unprivileged 21 % |
| `c4-r2-memory-lokyr-b-hidden-session` | 20:56:37 | 4 | done | 2.48 → 2.36 | WindowServer 33 %, launchd 30 %, wdavdaemon_unprivileged 23 % |
| `c4-r2-memory-lokyr-b` | 20:56:43 | 4 | done | 2.17 → 2.17 | Code Helper (Renderer) 74 %, WindowServer 34 %, wdavdaemon_unprivileged 22 % |
