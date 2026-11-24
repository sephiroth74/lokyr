# ttybench session ab-6-motion-2026-11-24-1904

Measured from 2026-11-24 19:04 to 19:23 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 12 runs: 12 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| motion · Frames a second of the motion (frames/s), kind drag | 58.77 | 59.09 | +0.31 (+0.5 %) | -1.50 … +2.12 | within the noise |
| motion · Frames a second of the motion (frames/s), kind resize | 58.82 | 58.82 | -0.00 (-0.0 %) | -1.39 … +1.38 | within the noise |
| motion · Frames missed (frames), kind drag | 11.5 | 8.5 | -3 (-26.1 %) | -20 … +14 | within the noise |
| motion · Frames missed (frames), kind resize | 11 | 11 | +0 (+0.0 %) | -13 … +13 | within the noise |
| motion · The compositor's delay (ms), kind drag | 12.60 | 26.01 | +13.41 (+106.4 %) | +0.18 … +15.96 | **worse** |
| motion · The compositor's delay (ms), kind resize | 11.43 | 11.83 | +0.40 (+3.5 %) | -0.08 … +0.66 | within the noise |
| motion · Its CPU (%), kind drag | 3.89 | 3.44 | -0.45 (-11.6 %) | -0.90 … -0.00 | better |
| motion · Its CPU (%), kind resize | 31.57 | 31.20 | -0.36 (-1.2 %) | -1.91 … +1.19 | within the noise |
| motion · WindowServer's CPU (%), kind drag | 35.48 | 28.60 | -6.88 (-19.4 %) | -10.52 … -3.24 | better |
| motion · WindowServer's CPU (%), kind resize | 44.37 | 44.21 | -0.16 (-0.4 %) | -4.05 … +3.72 | within the noise |
| frames · A frame's GPU time (ms), of `smooth-scroll` | 0.273 | 0.273 | +0.000 (+0.0 %) | -0.001 … +0.001 | within the noise |
| frames · Frames a second on the GPU (frames/s), of `smooth-scroll` | 59.25 | 59.55 | +0.30 (+0.5 %) | +0.10 … +0.50 | higher |
| frames · The gap between two frames (ms), of `smooth-scroll` | 16.42 | 16.43 | +0.01 (+0.0 %) | -0.06 … +0.06 | within the noise |
| frames · Refreshes of a new frame (%), of `smooth-scroll` | 98.59 | 99.17 | +0.58 (+0.6 %) | +0.17 … +0.99 | higher |
| frames · Frames shown a second (frames/s), of `smooth-scroll` | 59.16 | 59.50 | +0.35 (+0.6 %) | +0.10 … +0.59 | higher |
| frames · Frames dropped (frames), of `smooth-scroll` | 0.5 | 0 | -0.5 (-100.0 %) | -1 … +0 | within the noise |

## motion

### Frames a second of the motion (frames/s), kind drag

`frames_per_s`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 58.77 | 57.76 … 59.78 | 59.68 | 58.77 ± 1.43 | 57.76 … 59.78 | 2 (2) | 59.78, 57.76 |
| lokyr-b | 59.09 | 58.29 … 59.88 | 59.80 | 59.09 ± 1.13 | 58.29 … 59.88 | 2 (2) | 59.88, 58.29 |

### Frames a second of the motion (frames/s), kind resize

`frames_per_s`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 58.82 | 58.18 … 59.46 | 59.40 | 58.82 ± 0.91 | 58.18 … 59.46 | 2 (2) | 59.46, 58.18 |
| lokyr-b | 58.82 | 58.07 … 59.56 | 59.49 | 58.82 ± 1.05 | 58.07 … 59.56 | 2 (2) | 59.56, 58.07 |

### Frames missed (frames), kind drag

`missed_frames`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 11.5 | 2 … 21 | 20.1 | 11.5 ± 13.4 | 2 … 21 | 2 (2) | 2, 21 |
| lokyr-b | 8.5 | 1 … 16 | 15.2 | 8.5 ± 10.6 | 1 … 16 | 2 (2) | 1, 16 |

### Frames missed (frames), kind resize

`missed_frames`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 11 | 5 … 17 | 16.4 | 11 ± 8.5 | 5 … 17 | 2 (2) | 5, 17 |
| lokyr-b | 11 | 4 … 18 | 17.3 | 11 ± 9.9 | 4 … 18 | 2 (2) | 4, 18 |

### The compositor's delay (ms), kind drag

`compositor_delay_ms`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 12.60 | 12.26 … 13.05 | 13.63 | 12.48 ± 0.85 | 6.66 … 14.40 | 1105 (2) | 12.28, 13.03 |
| lokyr-b | 26.01 | 13.18 … 28.26 | 29.32 | 21.06 ± 7.47 | 5.90 … 30.46 | 1111 (2) | 28.24, 13.21 |

### The compositor's delay (ms), kind resize

`compositor_delay_ms`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 11.43 | 11.27 … 11.85 | 26.48 | 12.75 ± 4.87 | 4.85 … 28.64 | 1106 (2) | 11.31, 11.80 |
| lokyr-b | 11.83 | 11.63 … 11.99 | 27.92 | 16.08 ± 7.32 | 5.55 … 28.78 | 1106 (2) | 11.92, 11.75 |

### Its CPU (%), kind drag

`terminal_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 3.89 | 3.87 … 3.90 | 3.90 | 3.89 ± 0.02 | 3.87 … 3.90 | 2 (2) | 3.90, 3.87 |
| lokyr-b | 3.44 | 3.00 … 3.87 | 3.83 | 3.44 ± 0.62 | 3.00 … 3.87 | 2 (2) | 3.87, 3.00 |

### Its CPU (%), kind resize

`terminal_cpu_percent`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 31.57 | 31.53 … 31.60 | 31.60 | 31.57 ± 0.05 | 31.53 … 31.60 | 2 (2) | 31.53, 31.60 |
| lokyr-b | 31.20 | 29.69 … 32.72 | 32.57 | 31.20 ± 2.14 | 29.69 … 32.72 | 2 (2) | 32.72, 29.69 |

### WindowServer's CPU (%), kind drag

`window_server_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 35.48 | 32.17 … 38.79 | 38.45 | 35.48 ± 4.68 | 32.17 … 38.79 | 2 (2) | 38.79, 32.17 |
| lokyr-b | 28.60 | 28.26 … 28.93 | 28.90 | 28.60 ± 0.47 | 28.26 … 28.93 | 2 (2) | 28.93, 28.26 |

### WindowServer's CPU (%), kind resize

`window_server_cpu_percent`. Conditions: amplitude 200.0, awake true, kind resize, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 44.37 | 43.12 … 45.62 | 45.50 | 44.37 ± 1.77 | 43.12 … 45.62 | 2 (2) | 45.62, 43.12 |
| lokyr-b | 44.21 | 41.58 … 46.84 | 46.58 | 44.21 ± 3.72 | 41.58 … 46.84 | 2 (2) | 46.84, 41.58 |

## frames

### A frame's GPU time (ms), of `smooth-scroll`

`frame_gpu_ms`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.273 | 0.272 … 0.273 | 0.278 | 0.278 ± 0.036 | 0.181 … 0.734 | 1194 (2) | 0.273, 0.272 |
| lokyr-b | 0.273 | 0.272 … 0.274 | 0.288 | 0.283 ± 0.057 | 0.122 … 0.858 | 1200 (2) | 0.274, 0.273 |

### Frames a second on the GPU (frames/s), of `smooth-scroll`

`frames_per_s`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 59.25 | 59.16 … 59.35 | 59.34 | 59.25 ± 0.14 | 59.16 … 59.35 | 2 (2) | 59.16, 59.35 |
| lokyr-b | 59.55 | 59.45 … 59.65 | 59.64 | 59.55 ± 0.14 | 59.45 … 59.65 | 2 (2) | 59.65, 59.45 |

### The gap between two frames (ms), of `smooth-scroll`

`frame_gap_ms`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 16.42 | 16.39 … 16.46 | 17.42 | 16.52 ± 1.30 | 8.31 … 32.99 | 1192 (2) | 16.41, 16.43 |
| lokyr-b | 16.43 | 16.38 … 16.48 | 17.44 | 16.48 ± 1.14 | 4.74 … 33.44 | 1198 (2) | 16.44, 16.41 |

### Refreshes of a new frame (%), of `smooth-scroll`

`refreshes_with_frame`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 98.59 | 98.43 … 98.76 | 98.74 | 98.59 ± 0.23 | 98.43 … 98.76 | 2 (2) | 98.43, 98.76 |
| lokyr-b | 99.17 | 98.92 … 99.42 | 99.39 | 99.17 ± 0.35 | 98.92 … 99.42 | 2 (2) | 99.42, 98.92 |

### Frames shown a second (frames/s), of `smooth-scroll`

`frames_shown_per_s`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 59.16 | 59.06 … 59.25 | 59.24 | 59.16 ± 0.14 | 59.06 … 59.25 | 2 (2) | 59.06, 59.25 |
| lokyr-b | 59.50 | 59.35 … 59.65 | 59.64 | 59.50 ± 0.21 | 59.35 … 59.65 | 2 (2) | 59.65, 59.35 |

### Frames dropped (frames), of `smooth-scroll`

`frames_dropped`. Of the feature `smooth-scroll`. Conditions: awake true, by pixels, refresh_hz 60.0, rounds 1, seconds 10.0, workload scroll.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 0.5 | 0 … 1 | 0.9 | 0.5 ± 0.7 | 0 … 1 | 2 (2) | 1, 0 |
| lokyr-b | 0 | 0 … 0 | 0 | 0 ± 0 | 0 … 0 | 2 (2) | 0, 0 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-motion-lokyr-a` | 19:04:35 | 144 | done | 5.51 → 6.56 | WindowServer 24 %, wdavdaemon_enterprise 19 %, wdavdaemon 15 % |
| `c1-r1-motion-lokyr-b` | 19:07:01 | 111 | done | 6.20 → 10.48 | mds 44 %, wdavdaemon_enterprise 30 %, WindowServer 23 % |
| `c1-r2-motion-lokyr-a` | 19:08:55 | 131 | done | 9.88 → 10.35 | mds 40 %, wdavdaemon_enterprise 26 %, WindowServer 23 % |
| `c1-r2-motion-lokyr-b` | 19:11:07 | 124 | done | 10.35 → 10.41 | wdavdaemon_enterprise 26 %, WindowServer 26 %, wdavdaemon 20 % |
| `c1-r3-frames-lokyr-a` | 19:13:14 | 37 | done | 10.41 → 9.87 | wdavdaemon_enterprise 71 %, WindowServer 24 %, wdavdaemon 23 % |
| `c1-r3-frames-lokyr-b` | 19:13:53 | 36 | done | 9.87 → 28.21 | wdavdaemon_enterprise 64 %, WindowServer 27 %, wdavdaemon 20 % |
| `c2-r1-motion-lokyr-b` | 19:14:31 | 114 | done | 26.11 → 14.06 | BackgroundShortcutRunner 68 %, wdavdaemon_enterprise 32 %, siriactionsd 28 % |
| `c2-r1-motion-lokyr-a` | 19:16:27 | 109 | done | 14.06 → 7.67 | wdavdaemon_enterprise 27 %, wdavdaemon 25 %, epsext 20 % |
| `c2-r2-motion-lokyr-b` | 19:18:18 | 105 | done | 7.67 → 4.51 | wdavdaemon_enterprise 46 %, wdavdaemon 20 %, epsext 18 % |
| `c2-r2-motion-lokyr-a` | 19:20:05 | 109 | done | 4.31 → 8.01 | wdavdaemon_enterprise 27 %, wdavdaemon 23 %, epsext 17 % |
| `c2-r3-frames-lokyr-b` | 19:21:57 | 34 | done | 7.53 → 6.51 | wdavdaemon_enterprise 56 %, wdavdaemon 19 %, epsext 15 % |
| `c2-r3-frames-lokyr-a` | 19:22:33 | 35 | done | 6.51 → 5.62 | wdavdaemon_enterprise 58 %, wdavdaemon 21 %, epsext 17 % |
