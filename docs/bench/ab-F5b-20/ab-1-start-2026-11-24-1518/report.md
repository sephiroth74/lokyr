# ttybench session ab-1-start-2026-11-24-1518

Measured from 2026-11-24 15:20 to 15:25 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval: a change whose interval holds 0 is within the noise.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| startup · From the launch to its window (ms) | 365.8 | 372.1 | +6.2 (+1.7 %) | -0.6 … +16.5 | within the noise |
| startup · From the launch to running the payload (ms) | 713.8 | 718.6 | +4.8 (+0.7 %) | -4.2 … +11.2 | within the noise |
| memory · Its memory's footprint (MiB) | 45.26 | 45.29 | +0.03 (+0.1 %) | -0.13 … +0.13 | within the noise |
| memory · Its memory's peak footprint (MiB) | 206.1 | 206.1 | +0.0 (+0.0 %) | -0.1 … +0.1 | within the noise |

## startup

### From the launch to its window (ms)

`window_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 365.8 | 356.2 … 371.2 | 381.8 | 363.1 ± 13.9 | 326.5 … 391.8 | 32 | 364.4, 366.1, 368.4, 352.6 |
| lokyr-b | 372.1 | 366.4 … 375.9 | 386.1 | 367.4 ± 18.1 | 309.9 … 398.6 | 32 | 370.1, 363.1, 374.0, 379.4 |

### From the launch to running the payload (ms)

`ready_ms`. Conditions: cols 120, pause_s 2.0, rounds 8, rows 40, season 0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 713.8 | 709.4 … 720.3 | 751.8 | 720.5 ± 44.0 | 668.6 … 940.7 | 32 | 716.9, 711.8, 718.9, 709.0 |
| lokyr-b | 718.6 | 711.4 … 724.0 | 739.8 | 714.6 ± 18.2 | 671.3 … 739.9 | 32 | 719.3, 712.6, 716.7, 720.9 |

## memory

### Its memory's footprint (MiB)

`footprint_mib`. Conditions: rounds 2, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 45.26 | 45.21 … 45.42 | 45.52 | 45.29 ± 0.14 | 45.10 … 45.56 | 8 | 45.22, 45.35, 45.25, 45.33 |
| lokyr-b | 45.29 | 45.22 … 45.36 | 45.39 | 45.29 ± 0.08 | 45.14 … 45.41 | 8 | 45.35, 45.30, 45.18, 45.31 |

### Its memory's peak footprint (MiB)

`peak_mib`. Conditions: rounds 2, settle_s 3.0, workload rest.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 206.1 | 206.0 … 206.2 | 206.3 | 206.1 ± 0.1 | 205.9 … 206.4 | 8 | 206.0, 206.2, 206.1, 206.1 |
| lokyr-b | 206.1 | 206.0 … 206.2 | 206.2 | 206.1 ± 0.1 | 205.9 … 206.2 | 8 | 206.2, 206.1, 206.0, 206.1 |
