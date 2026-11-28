# ttybench session ab-6-motion-2026-11-28-1051

Measured from 2026-11-28 10:51 to 11:09 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 12 runs: 12 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| motion · Frames a second of the motion (frames/s), kind drag | 58.66 | 58.23 | -0.43 (-0.7 %) | -1.60 … +0.74 | within the noise |
| motion · Frames a second of the motion (frames/s), kind resize | 56.64 | 57.01 | +0.37 (+0.7 %) | -0.53 … +1.28 | within the noise |
| motion · Frames missed (frames), kind drag | 12.5 | 17 | +4.5 (+36.0 %) | -6 … +15 | within the noise |
| motion · Frames missed (frames), kind resize | 31.5 | 28 | -3.5 (-11.1 %) | -12 … +5 | within the noise |
| motion · The compositor's delay (ms), kind drag | 13.36 | 13.14 | -0.22 (-1.6 %) | -0.51 … -0.07 | better |
| motion · The compositor's delay (ms), kind resize | 11.90 | 12.05 | +0.15 (+1.3 %) | -0.16 … +0.44 | within the noise |
| motion · Its CPU (%), kind drag | 3.65 | 4.38 | +0.73 (+20.0 %) | +0.44 … +1.02 | **worse** |
| motion · Its CPU (%), kind resize | 30.92 | 30.51 | -0.41 (-1.3 %) | -0.79 … -0.04 | better |
| motion · WindowServer's CPU (%), kind drag | 27.36 | 33.86 | +6.49 (+23.7 %) | +4.49 … +8.50 | **worse** |
| motion · WindowServer's CPU (%), kind resize | 40.30 | 39.56 | -0.73 (-1.8 %) | -0.91 … -0.55 | better |
| frames · A frame's GPU time (ms), of `smooth-scroll` | 0.273 | 0.273 | +0.000 (+0.1 %) | -0.000 … +0.001 | within the noise |
| frames · Frames a second on the GPU (frames/s), of `smooth-scroll` | 59.45 | 59.60 | +0.15 (+0.3 %) | -0.00 … +0.30 | within the noise |
| frames · The gap between two frames (ms), of `smooth-scroll` | 16.42 | 16.42 | +0.00 (+0.0 %) | -0.04 … +0.04 | within the noise |
| frames · Refreshes of a new frame (%), of `smooth-scroll` | 98.92 | 99.00 | +0.08 (+0.1 %) | -0.50 … +0.66 | within the noise |
| frames · Frames shown a second (frames/s), of `smooth-scroll` | 59.35 | 59.40 | +0.05 (+0.1 %) | -0.30 … +0.40 | within the noise |
| frames · Frames dropped (frames), of `smooth-scroll` | 1 | 1 | +0 (+0.0 %) | -2 … +2 | within the noise |

## motion

### Frames a second of the motion (frames/s), kind drag

`frames_per_s`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 58.66 | 58.29 … 59.03 | 58.99 | 58.66 ± 0.52 | 58.29 … 59.03 | 2 (2) | 59.03, 58.29 |
| lokyr-b | 58.23 | 57.44 … 59.03 | 58.95 | 58.23 ± 1.13 | 57.44 … 59.03 | 2 (2) | 57.44, 59.03 |

### Frames a second of the motion (frames/s), kind resize

`frames_per_s`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 56.64 | 55.95 … 57.33 | 57.26 | 56.64 ± 0.98 | 55.95 … 57.33 | 2 (2) | 57.33, 55.95 |
| lokyr-b | 57.01 | 56.80 … 57.22 | 57.20 | 57.01 ± 0.30 | 56.80 … 57.22 | 2 (2) | 56.80, 57.22 |

### Frames missed (frames), kind drag

`missed_frames`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 12.5 | 9 … 16 | 15.6 | 12.5 ± 4.9 | 9 … 16 | 2 (2) | 9, 16 |
| lokyr-b | 17 | 10 … 24 | 23.3 | 17 ± 9.9 | 10 … 24 | 2 (2) | 24, 10 |

### Frames missed (frames), kind resize

`missed_frames`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 31.5 | 25 … 38 | 37.4 | 31.5 ± 9.2 | 25 … 38 | 2 (2) | 25, 38 |
| lokyr-b | 28 | 26 … 30 | 29.8 | 28 ± 2.8 | 26 … 30 | 2 (2) | 30, 26 |

### The compositor's delay (ms), kind drag

`compositor_delay_ms`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 13.36 | 13.32 … 13.50 | 26.88 | 15.71 ± 5.30 | 5.46 … 30.40 | 1103 (2) | 13.34, 13.45 |
| lokyr-b | 13.14 | 12.93 … 13.27 | 13.71 | 12.93 ± 0.66 | 9.84 … 14.36 | 1095 (2) | 13.26, 12.95 |

### The compositor's delay (ms), kind resize

`compositor_delay_ms`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 11.90 | 11.63 … 12.07 | 12.88 | 12.04 ± 3.06 | 4.54 … 29.19 | 1065 (2) | 11.95, 11.88 |
| lokyr-b | 12.05 | 11.80 … 12.17 | 26.71 | 12.62 ± 4.04 | 4.29 … 30.67 | 1072 (2) | 11.94, 12.12 |

### Its CPU (%), kind drag

`terminal_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 3.65 | 3.43 … 3.87 | 3.85 | 3.65 ± 0.32 | 3.43 … 3.87 | 2 (2) | 3.87, 3.43 |
| lokyr-b | 4.38 | 4.31 … 4.45 | 4.44 | 4.38 ± 0.10 | 4.31 … 4.45 | 2 (2) | 4.31, 4.45 |

### Its CPU (%), kind resize

`terminal_cpu_percent`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 30.92 | 30.86 … 30.98 | 30.98 | 30.92 ± 0.09 | 30.86 … 30.98 | 2 (2) | 30.86, 30.98 |
| lokyr-b | 30.51 | 30.20 … 30.81 | 30.78 | 30.51 ± 0.44 | 30.20 … 30.81 | 2 (2) | 30.81, 30.20 |

### WindowServer's CPU (%), kind drag

`window_server_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 27.36 | 25.65 … 29.08 | 28.91 | 27.36 ± 2.43 | 25.65 … 29.08 | 2 (2) | 29.08, 25.65 |
| lokyr-b | 33.86 | 33.57 … 34.14 | 34.11 | 33.86 ± 0.41 | 33.57 … 34.14 | 2 (2) | 33.57, 34.14 |

### WindowServer's CPU (%), kind resize

`window_server_cpu_percent`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 40.30 | 40.22 … 40.38 | 40.37 | 40.30 ± 0.11 | 40.22 … 40.38 | 2 (2) | 40.38, 40.22 |
| lokyr-b | 39.56 | 39.46 … 39.66 | 39.65 | 39.56 ± 0.14 | 39.46 … 39.66 | 2 (2) | 39.46, 39.66 |

## frames

### A frame's GPU time (ms), of `smooth-scroll`

`frame_gpu_ms`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.273 | 0.272 … 0.273 | 0.276 | 0.273 ± 0.003 | 0.265 … 0.314 | 1198 (2) | 0.273, 0.273 |
| lokyr-b | 0.273 | 0.273 … 0.273 | 0.276 | 0.273 ± 0.010 | 0.266 … 0.613 | 1201 (2) | 0.273, 0.273 |

### Frames a second on the GPU (frames/s), of `smooth-scroll`

`frames_per_s`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 59.45 | 59.35 … 59.55 | 59.54 | 59.45 ± 0.14 | 59.35 … 59.55 | 2 (2) | 59.35, 59.55 |
| lokyr-b | 59.60 | 59.55 … 59.65 | 59.65 | 59.60 ± 0.07 | 59.55 … 59.65 | 2 (2) | 59.65, 59.55 |

### The gap between two frames (ms), of `smooth-scroll`

`frame_gap_ms`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 16.42 | 16.39 … 16.45 | 17.35 | 16.52 ± 1.08 | 11.98 … 26.91 | 1196 (2) | 16.42, 16.42 |
| lokyr-b | 16.42 | 16.39 … 16.45 | 17.33 | 16.52 ± 1.01 | 15.02 … 26.36 | 1199 (2) | 16.43, 16.42 |

### Refreshes of a new frame (%), of `smooth-scroll`

`refreshes_with_frame`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 98.92 | 98.59 … 99.25 | 99.22 | 98.92 ± 0.47 | 98.59 … 99.25 | 2 (2) | 98.59, 99.25 |
| lokyr-b | 99.00 | 98.76 … 99.25 | 99.23 | 99.00 ± 0.35 | 98.76 … 99.25 | 2 (2) | 99.25, 98.76 |

### Frames shown a second (frames/s), of `smooth-scroll`

`frames_shown_per_s`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 59.35 | 59.15 … 59.55 | 59.53 | 59.35 ± 0.28 | 59.15 … 59.55 | 2 (2) | 59.15, 59.55 |
| lokyr-b | 59.40 | 59.25 … 59.55 | 59.54 | 59.40 ± 0.21 | 59.25 … 59.55 | 2 (2) | 59.55, 59.25 |

### Frames dropped (frames), of `smooth-scroll`

`frames_dropped`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 1 | 0 … 2 | 1.9 | 1 ± 1.4 | 0 … 2 | 2 (2) | 2, 0 |
| lokyr-b | 1 | 0 … 2 | 1.9 | 1 ± 1.4 | 0 … 2 | 2 (2) | 0, 2 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-motion-lokyr-a` | 10:51:37 | 256 | done | 4.19 → 10.05 | epsext 47 %, wdavdaemon 29 %, Python 29 % |
| `c1-r1-motion-lokyr-b` | 10:55:56 | 130 | done | 10.05 → 7.47 | wdavdaemon_enterprise 29 %, wdavdaemon 14 %, epsext 13 % |
| `c1-r2-motion-lokyr-a` | 10:58:07 | 90 | done | 7.20 → 5.56 | wdavdaemon_enterprise 37 %, wdavdaemon 16 %, epsext 14 % |
| `c1-r2-motion-lokyr-b` | 10:59:39 | 90 | done | 5.19 → 7.67 | wdavdaemon_enterprise 33 %, wdavdaemon 16 %, epsext 14 % |
| `c1-r3-frames-lokyr-a` | 11:01:11 | 35 | done | 7.67 → 5.93 | wdavdaemon_enterprise 64 %, wdavdaemon 22 %, epsext 18 % |
| `c1-r3-frames-lokyr-b` | 11:01:48 | 34 | done | 5.46 → 4.31 | wdavdaemon_enterprise 55 %, wdavdaemon 21 %, epsext 17 % |
| `c2-r1-motion-lokyr-b` | 11:02:24 | 84 | done | 4.20 → 6.53 | wdavdaemon_enterprise 29 %, wdavdaemon 21 %, epsext 18 % |
| `c2-r1-motion-lokyr-a` | 11:03:50 | 83 | done | 6.53 → 5.94 | wdavdaemon_enterprise 34 %, wdavdaemon 17 %, epsext 16 % |
| `c2-r2-motion-lokyr-b` | 11:05:16 | 93 | done | 5.94 → 5.17 | wdavdaemon_enterprise 27 %, wdavdaemon 16 %, epsext 14 % |
| `c2-r2-motion-lokyr-a` | 11:06:51 | 92 | done | 5.17 → 7.31 | wdavdaemon_enterprise 25 %, wdavdaemon 18 %, epsext 15 % |
| `c2-r3-frames-lokyr-b` | 11:08:24 | 35 | done | 6.80 → 5.16 | wdavdaemon_enterprise 69 %, wdavdaemon 22 %, epsext 18 % |
| `c2-r3-frames-lokyr-a` | 11:09:01 | 34 | done | 5.16 → 3.97 | wdavdaemon_enterprise 56 %, wdavdaemon 20 %, epsext 16 % |
