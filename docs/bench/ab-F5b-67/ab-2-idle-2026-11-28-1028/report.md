# ttybench session ab-2-idle-2026-11-28-1028

Measured from 2026-11-28 10:28 to 10:35 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## Budgets

4 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| idle · Interrupt wakeups a second, its mean | lokyr-a | 0.3301 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b | 0.3221 /s | ≤ 0.5 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-a | 0 /s | ≤ 0 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b | 0 /s | ≤ 0 /s | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| idle · Its CPU (%) | 0.0514 | 0.0523 | +0.0009 (+1.7 %) | -0.0010 … +0.0036 | within the noise |
| idle · Package idle wakeups a second (/s), its mean | 0 | 0 | +0 | +0 … +0 | within the noise |
| idle · Interrupt wakeups a second (/s), its mean | 0.3301 | 0.3221 | -0.0080 (-2.4 %) | -0.1318 … +0.1157 | within the noise |
| idle · The power that the kernel bills it (mW) | 0.0592 | 0.0586 | -0.0005 (-0.9 %) | -0.0023 … +0.0019 | within the noise |

## idle

### Its CPU (%)

`cpu_percent`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0514 | 0.0495 … 0.0531 | 0.0677 | 0.0469 ± 0.0140 | 0.0197 … 0.0817 | 120 (4) | 0.0521, 0.0513, 0.0505, 0.0511 |
| lokyr-b | 0.0523 | 0.0515 … 0.0537 | 0.0623 | 0.0473 ± 0.0135 | 0.0205 … 0.0771 | 120 (4) | 0.0520, 0.0528, 0.0517, 0.0537 |

### Package idle wakeups a second (/s)

`package_idle_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 (4) | 0, 0, 0, 0 |
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 (4) | 0, 0, 0, 0 |

### Interrupt wakeups a second (/s)

`interrupt_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0000 | 0.0000 … 0.0000 | 0.9929 | 0.3301 ± 0.4688 | 0.0000 … 0.9960 | 120 (4) | 0.0000, 0.0000, 0.0000, 0.0000 |
| lokyr-b | 0.0000 | 0.0000 … 0.0000 | 0.9940 | 0.3221 ± 0.4661 | 0.0000 … 0.9974 | 120 (4) | 0.0000, 0.0000, 0.0000, 0.0000 |

### The power that the kernel bills it (mW)

`power_mw`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0592 | 0.0572 … 0.0609 | 0.0856 | 0.0571 ± 0.0176 | 0.0270 … 0.1033 | 120 (4) | 0.0590, 0.0593, 0.0580, 0.0609 |
| lokyr-b | 0.0586 | 0.0580 … 0.0601 | 0.0763 | 0.0549 ± 0.0173 | 0.0270 … 0.1481 | 120 (4) | 0.0587, 0.0591, 0.0582, 0.0598 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-idle-lokyr-a` | 10:28:12 | 51 | done | 2.08 → 1.42 |  |
| `c1-r1-idle-lokyr-b` | 10:29:06 | 51 | done | 1.42 → 1.24 | wdavdaemon_unprivileged 22 %, Code Helper (Renderer) 22 %, BiomeAgent 21 % |
| `c2-r1-idle-lokyr-b` | 10:29:59 | 51 | done | 1.14 → 1.35 | wdavdaemon_unprivileged 128 %, Code Helper (Renderer) 114 %, wdavdaemon 35 % |
| `c2-r1-idle-lokyr-a` | 10:30:52 | 51 | done | 1.35 → 1.23 | Code Helper (Renderer) 22 %, epsext 20 %, wdavdaemon 19 % |
| `c3-r1-idle-lokyr-a` | 10:31:45 | 51 | done | 1.21 → 1.25 | mediaanalysisd 70 %, Code Helper (Renderer) 22 %, wdavdaemon_unprivileged 22 % |
| `c3-r1-idle-lokyr-b` | 10:32:39 | 51 | done | 1.25 → 1.27 | Code Helper (Renderer) 25 %, WindowServer 19 %, wdavdaemon_unprivileged 18 % |
| `c4-r1-idle-lokyr-b` | 10:33:32 | 51 | done | 1.25 → 0.96 | Code Helper (Renderer) 25 %, BiomeAgent 18 %, WindowServer 17 % |
| `c4-r1-idle-lokyr-a` | 10:34:25 | 51 | done | 0.96 → 1.10 | Code Helper (Renderer) 24 %, wdavdaemon_unprivileged 18 %, WindowServer 17 % |
