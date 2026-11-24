# ttybench session scenario-floating-2026-11-24-2106

Measured from 2026-11-24 21:06 to 21:27 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 24 runs: 24 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-b | 1.4.0 (202611241345) |
| lokyr-b-floating | 1.4.0 (202611241345) |

## Budgets

5 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| idle · Interrupt wakeups a second, its mean | lokyr-b | 0.3390 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b-floating | 0.3307 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b-floating against lokyr-b | -0.0083 /s (-2.4 %), 95 % -0.1400 … +0.1155 | ≤ +0.1 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b | 0 /s | ≤ 0 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b-floating | 0 /s | ≤ 0 /s | ✔ held |

## A/B: lokyr-b-floating against lokyr-b

Each metric's median of A, lokyr-b, and of B, lokyr-b-floating; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| idle · Its CPU (%) | 0.0541 | 0.0537 | -0.0004 (-0.8 %) | -0.0027 … +0.0035 | within the noise |
| idle · Package idle wakeups a second (/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| idle · Interrupt wakeups a second (/s) | 0.0000 | 0.0000 | +0.0000 | +0.0000 … +0.0000 | within the noise |
| idle · The power that the kernel bills it (mW) | 0.0604 | 0.0596 | -0.0008 (-1.4 %) | -0.0026 … +0.0015 | within the noise |
| memory · Its memory's footprint (MiB) | 55.26 | 72.83 | +17.57 (+31.8 %) | +17.24 … +17.78 | **worse** |
| memory · Its memory's peak footprint (MiB) | 212.1 | 229.6 | +17.6 (+8.3 %) | +17.2 … +17.8 | **worse** |
| frames · Frames a second on the GPU (frames/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| frames · Refreshes of a new frame (%) | 0 | 0 | +0 | +0 … +0 | within the noise |
| frames · Frames shown a second (frames/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| frames · Frames dropped (frames) | 0 | 0 | +0 | +0 … +0 | within the noise |

## idle

### Its CPU (%)

`cpu_percent`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.0541 | 0.0503 … 0.0555 | 0.0687 | 0.0501 ± 0.0204 | 0.0184 … 0.2102 | 120 (4) | 0.0550, 0.0542, 0.0533, 0.0464 |
| lokyr-b-floating | 0.0537 | 0.0519 … 0.0545 | 0.0620 | 0.0497 ± 0.0119 | 0.0251 … 0.0755 | 120 (4) | 0.0539, 0.0540, 0.0531, 0.0512 |

### Package idle wakeups a second (/s)

`package_idle_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 (4) | 0, 0, 0, 0 |
| lokyr-b-floating | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 (4) | 0, 0, 0, 0 |

### Interrupt wakeups a second (/s)

`interrupt_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.0000 | 0.0000 … 0.0000 | 0.9947 | 0.3390 ± 0.4898 | 0.0000 … 1.9901 | 120 (4) | 0.0000, 0.0000, 0.0000, 0.0000 |
| lokyr-b-floating | 0.0000 | 0.0000 … 0.0000 | 0.9952 | 0.3307 ± 0.4697 | 0.0000 … 0.9976 | 120 (4) | 0.0000, 0.0000, 0.0000, 0.0000 |

### The power that the kernel bills it (mW)

`power_mw`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0.0604 | 0.0590 … 0.0622 | 0.1026 | 0.0612 ± 0.0286 | 0.0268 … 0.2883 | 120 (4) | 0.0604, 0.0597, 0.0595, 0.0617 |
| lokyr-b-floating | 0.0596 | 0.0589 … 0.0615 | 0.0787 | 0.0569 ± 0.0143 | 0.0282 … 0.0855 | 120 (4) | 0.0593, 0.0629, 0.0593, 0.0597 |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 55.26 | 55.05 … 55.50 | 55.49 | 55.27 ± 0.22 | 55.05 … 55.50 | 4 (4) | 55.50, 55.11, 55.41, 55.05 |
| lokyr-b-floating | 72.83 | 72.58 … 72.85 | 72.85 | 72.77 ± 0.13 | 72.58 … 72.85 | 4 (4) | 72.85, 72.58, 72.85, 72.82 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 212.1 | 211.9 … 212.3 | 212.3 | 212.1 ± 0.2 | 211.9 … 212.3 | 4 (4) | 212.3, 211.9, 212.2, 211.9 |
| lokyr-b-floating | 229.6 | 229.4 … 229.7 | 229.7 | 229.6 ± 0.1 | 229.4 … 229.7 | 4 (4) | 229.7, 229.4, 229.7, 229.6 |

## frames

### Frames a second on the GPU (frames/s)

`frames_per_s`. Conditions: awake true, refresh_hz 60.0, rounds 1, seconds 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |
| lokyr-b-floating | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |

### Refreshes of a new frame (%)

`refreshes_with_frame`. Conditions: awake true, refresh_hz 60.0, rounds 1, seconds 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |
| lokyr-b-floating | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |

### Frames shown a second (frames/s)

`frames_shown_per_s`. Conditions: awake true, refresh_hz 60.0, rounds 1, seconds 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |
| lokyr-b-floating | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |

### Frames dropped (frames)

`frames_dropped`. Conditions: awake true, refresh_hz 60.0, rounds 1, seconds 10.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |
| lokyr-b-floating | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 4 (4) | 0, 0, 0, 0 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-idle-lokyr-b` | 21:06:45 | 51 | done | 3.57 → 2.10 | wdavdaemon_unprivileged 24 %, Code Helper (Renderer) 22 %, WindowServer 19 % |
| `c1-r1-idle-lokyr-b-floating` | 21:07:38 | 51 | done | 2.10 → 1.91 | WindowServer 20 %, MenuBarAgent 16 %, Code Helper (Renderer) 15 % |
| `c1-r2-memory-lokyr-b` | 21:08:31 | 4 | done | 1.91 → 1.84 | Code Helper (Renderer) 21 %, WindowServer 20 %, BiomeAgent 18 % |
| `c1-r2-memory-lokyr-b-floating` | 21:08:37 | 4 | done | 1.84 → 2.01 | WindowServer 20 %, BiomeAgent 16 %, Code Helper (Renderer) 12 % |
| `c2-r1-idle-lokyr-b-floating` | 21:09:57 | 51 | done | 30.80 → 14.49 | Code Helper (Renderer) 64 %, WindowServer 22 %, Code Helper (Renderer) 20 % |
| `c2-r1-idle-lokyr-b` | 21:10:51 | 51 | done | 13.41 → 6.22 | Code Helper (Renderer) 119 %, WindowServer 20 %, wdavdaemon_unprivileged 20 % |
| `c2-r2-memory-lokyr-b-floating` | 21:11:44 | 4 | done | 6.22 → 5.80 | wdavdaemon_unprivileged 29 %, Code Helper (Renderer) 22 %, BiomeAgent 20 % |
| `c2-r2-memory-lokyr-b` | 21:11:50 | 4 | done | 5.80 → 5.41 | wdavdaemon_unprivileged 27 %, Code Helper (Renderer) 22 %, BiomeAgent 20 % |
| `c3-r1-idle-lokyr-b` | 21:12:58 | 51 | done | 3.98 → 2.16 | Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 21 %, BiomeAgent 20 % |
| `c3-r1-idle-lokyr-b-floating` | 21:13:51 | 52 | done | 2.15 → 1.52 | Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 21 %, WindowServer 20 % |
| `c3-r2-memory-lokyr-b` | 21:14:44 | 4 | done | 1.52 → 1.48 | wdavdaemon_unprivileged 18 %, MenuBarAgent 16 %, WindowServer 14 % |
| `c3-r2-memory-lokyr-b-floating` | 21:14:50 | 4 | done | 1.48 → 1.60 | Code Helper (Renderer) 19 %, WindowServer 18 %, BiomeAgent 15 % |
| `c4-r1-idle-lokyr-b-floating` | 21:15:58 | 52 | done | 2.70 → 1.71 | Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 19 %, BiomeAgent 18 % |
| `c4-r1-idle-lokyr-b` | 21:16:52 | 51 | done | 1.71 → 2.76 | WindowServer 18 %, BiomeAgent 17 %, Code Helper (Renderer) 12 % |
| `c4-r2-memory-lokyr-b-floating` | 21:17:45 | 4 | done | 2.76 → 2.78 | Code Helper (Renderer) 17 %, WindowServer 17 %, MenuBarAgent 16 % |
| `c4-r2-memory-lokyr-b` | 21:17:51 | 4 | done | 2.78 → 2.64 | Code Helper (Renderer) 19 %, wdavdaemon_unprivileged 19 %, WindowServer 18 % |
| `c1-r3-frames-lokyr-b` | 21:23:04 | 31 | done | 2.62 → 3.41 | wdavdaemon_enterprise 48 %, wdavdaemon 27 %, epsext 22 % |
| `c1-r3-frames-lokyr-b-floating` | 21:23:36 | 30 | done | 3.41 → 3.95 | wdavdaemon_enterprise 55 %, wdavdaemon 31 %, epsext 27 % |
| `c2-r3-frames-lokyr-b-floating` | 21:24:09 | 31 | done | 4.20 → 4.27 | duetexpertd 95 %, wdavdaemon_enterprise 56 %, wdavdaemon 32 % |
| `c2-r3-frames-lokyr-b` | 21:24:42 | 30 | done | 4.27 → 3.81 | wdavdaemon_enterprise 60 %, wdavdaemon 26 %, epsext 22 % |
| `c3-r3-frames-lokyr-b` | 21:25:14 | 30 | done | 3.74 → 4.46 | wdavdaemon_enterprise 37 %, WindowServer 23 %, wdavdaemon 22 % |
| `c3-r3-frames-lokyr-b-floating` | 21:25:47 | 30 | done | 4.46 → 4.18 | wdavdaemon_enterprise 55 %, wdavdaemon 30 %, epsext 25 % |
| `c4-r3-frames-lokyr-b-floating` | 21:26:19 | 30 | done | 4.49 → 4.66 | wdavdaemon_enterprise 49 %, wdavdaemon 28 %, epsext 24 % |
| `c4-r3-frames-lokyr-b` | 21:26:51 | 30 | done | 4.66 → 4.19 | wdavdaemon_enterprise 40 %, wdavdaemon 24 %, WindowServer 22 % |
