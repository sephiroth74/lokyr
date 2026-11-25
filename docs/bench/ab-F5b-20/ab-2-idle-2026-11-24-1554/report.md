# ttybench session ab-2-idle-2026-11-24-1554

Measured from 2026-11-24 15:55 to 16:02 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## Budgets

4 held, 0 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| idle · Interrupt wakeups a second, its mean | lokyr-a | 0.3476 /s | ≤ 0.5 /s | ✔ held |
| idle · Interrupt wakeups a second, its mean | lokyr-b | 0.3309 /s | ≤ 0.5 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-a | 0 /s | ≤ 0 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b | 0 /s | ≤ 0 /s | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval: a change whose interval holds 0 is within the noise.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| idle · Its CPU (%) | 0.0590 | 0.0550 | -0.0040 (-6.8 %) | -0.0094 … -0.0007 | better |
| idle · Package idle wakeups a second (/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| idle · Interrupt wakeups a second (/s) | 0.0000 | 0.0000 | +0.0000 | +0.0000 … +0.0000 | within the noise |
| idle · The power that the kernel bills it (mW) | 0.0696 | 0.0701 | +0.0005 (+0.7 %) | -0.0033 … +0.0039 | within the noise |

## idle

### Its CPU (%)

`cpu_percent`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0590 | 0.0570 … 0.0605 | 0.0686 | 0.0555 ± 0.0192 | 0.0225 … 0.2012 | 120 | 0.0595, 0.0592, 0.0578, 0.0558 |
| lokyr-b | 0.0550 | 0.0501 … 0.0573 | 0.0694 | 0.0504 ± 0.0155 | 0.0162 … 0.0851 | 120 | 0.0610, 0.0589, 0.0562, 0.0381 |

### Package idle wakeups a second (/s)

`package_idle_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 | 0, 0, 0, 0 |
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 | 0, 0, 0, 0 |

### Interrupt wakeups a second (/s)

`interrupt_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0000 | 0.0000 … 0.0000 | 0.9968 | 0.3476 ± 0.4929 | 0.0000 … 1.9888 | 120 | 0.0000, 0.0000, 0.0000, 0.0000 |
| lokyr-b | 0.0000 | 0.0000 … 0.0000 | 0.9956 | 0.3309 ± 0.4699 | 0.0000 … 0.9985 | 120 | 0.0000, 0.0000, 0.0000, 0.0000 |

### The power that the kernel bills it (mW)

`power_mw`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 20.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0696 | 0.0668 … 0.0721 | 0.1015 | 0.0731 ± 0.0393 | 0.0287 … 0.4458 | 120 | 0.0703, 0.0698, 0.0701, 0.0677 |
| lokyr-b | 0.0701 | 0.0676 … 0.0723 | 0.1099 | 0.0711 ± 0.0229 | 0.0309 … 0.1550 | 120 | 0.0705, 0.0726, 0.0701, 0.0681 |
