# ttybench session ab-2-idle-2026-11-24-1538

Measured from 2026-11-24 15:38 to 15:43 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## Budgets

3 held, 1 not held.

| Budget | Of | Value | Limit | |
|---|---|---|---|---|
| idle · Interrupt wakeups a second, its mean | lokyr-a | 0.5303 /s | ≤ 0.5 /s | ✘ not held |
| idle · Interrupt wakeups a second, its mean | lokyr-b | 0.4725 /s | ≤ 0.5 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-a | 0 /s | ≤ 0 /s | ✔ held |
| idle · Package idle wakeups a second, its mean | lokyr-b | 0 /s | ≤ 0 /s | ✔ held |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval: a change whose interval holds 0 is within the noise.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| idle · Its CPU (%) | 0.0588 | 0.0587 | -0.0001 (-0.2 %) | -0.0036 … +0.0020 | within the noise |
| idle · Package idle wakeups a second (/s) | 0 | 0 | +0 | +0 … +0 | within the noise |
| idle · Interrupt wakeups a second (/s) | 0.0000 | 0.0000 | +0.0000 | -0.9924 … +0.0000 | within the noise |
| idle · The power that the kernel bills it (mW) | 0.0787 | 0.0733 | -0.0054 (-6.8 %) | -0.0111 … +0.0016 | within the noise |

## idle

### Its CPU (%)

`cpu_percent`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0588 | 0.0571 … 0.0603 | 0.1180 | 0.0653 ± 0.0382 | 0.0211 … 0.3589 | 120 | 0.0594, 0.0599, 0.0571, 0.0591 |
| lokyr-b | 0.0587 | 0.0556 … 0.0599 | 0.1074 | 0.0618 ± 0.0261 | 0.0228 … 0.2011 | 120 | 0.0597, 0.0566, 0.0534, 0.0609 |

### Package idle wakeups a second (/s)

`package_idle_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 | 0, 0, 0, 0 |
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 120 | 0, 0, 0, 0 |

### Interrupt wakeups a second (/s)

`interrupt_wakeups_per_s`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0000 | 0.0000 … 0.9924 | 1.9858 | 0.5303 ± 0.8046 | 0.0000 … 3.9828 | 120 | 0.0000, 0.0000, 0.0000, 0.0000 |
| lokyr-b | 0.0000 | 0.0000 … 0.4961 | 0.9976 | 0.4725 ± 0.6699 | 0.0000 … 2.9923 | 120 | 0.0000, 0.0000, 0.0000, 0.0000 |

### The power that the kernel bills it (mW)

`power_mw`. Conditions: duration_s 30.0, interval_s 1.0, rounds 1, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.0787 | 0.0738 … 0.0828 | 0.1992 | 0.0929 ± 0.0784 | 0.0298 … 0.7404 | 120 | 0.0808, 0.0790, 0.0783, 0.0737 |
| lokyr-b | 0.0733 | 0.0694 … 0.0785 | 0.1862 | 0.0847 ± 0.0485 | 0.0292 … 0.3551 | 120 | 0.0713, 0.0803, 0.0799, 0.0677 |
