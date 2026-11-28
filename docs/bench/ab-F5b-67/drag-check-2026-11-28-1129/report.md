# ttybench session drag-check-2026-11-28-1129

Measured from 2026-11-28 11:29 to 11:47 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 12 runs: 12 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| motion · Frames a second of the motion (frames/s) | 58.08 | 57.97 | -0.11 (-0.2 %) | -0.96 … +0.74 | within the noise |
| motion · Frames missed (frames) | 18 | 19 | +1 (+5.6 %) | -6.5 … +9 | within the noise |
| motion · The compositor's delay (ms) | 13.25 | 12.98 | -0.27 (-2.0 %) | -0.71 … +0.04 | within the noise |
| motion · Its CPU (%) | 3.54 | 4.01 | +0.47 (+13.4 %) | -0.18 … +1.03 | within the noise |
| motion · WindowServer's CPU (%) | 31.90 | 34.59 | +2.69 (+8.4 %) | -1.74 … +6.16 | within the noise |

## motion

### Frames a second of the motion (frames/s)

`frames_per_s`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 58.08 | 57.76 … 58.55 | 58.74 | 58.13 ± 0.45 | 57.54 … 58.93 | 6 (6) | 57.97, 58.18, 58.93, 58.18, 57.97, 57.54 |
| lokyr-b | 57.97 | 57.12 … 58.82 | 58.93 | 57.97 ± 0.79 | 57.02 … 59.03 | 6 (6) | 58.18, 57.76, 58.61, 57.22, 59.03, 57.02 |

### Frames missed (frames)

`missed_frames`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 18 | 13 … 21 | 22 | 17.3 ± 4.6 | 9 … 23 | 6 (6) | 19, 17, 9, 17, 19, 23 |
| lokyr-b | 19 | 11.5 … 27 | 27.5 | 19.2 ± 7.1 | 10 … 28 | 6 (6) | 17, 21, 13, 26, 10, 28 |

### The compositor's delay (ms)

`compositor_delay_ms`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 13.25 | 12.95 … 13.64 | 26.71 | 15.04 ± 4.99 | 3.96 … 30.21 | 3279 (6) | 13.22, 13.99, 12.79, 13.00, 13.78, 12.95 |
| lokyr-b | 12.98 | 12.82 … 13.11 | 26.49 | 13.83 ± 3.93 | 4.59 … 30.86 | 3270 (6) | 13.31, 12.79, 13.00, 12.93, 12.77, 12.89 |

### Its CPU (%)

`terminal_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 3.54 | 3.00 … 4.09 | 4.17 | 3.54 ± 0.54 | 2.86 … 4.25 | 6 (6) | 2.86, 3.15, 3.92, 4.25, 3.24, 3.83 |
| lokyr-b | 4.01 | 3.61 … 4.18 | 4.22 | 3.93 ± 0.32 | 3.33 … 4.25 | 6 (6) | 3.33, 3.94, 4.08, 4.25, 3.88, 4.11 |

### WindowServer's CPU (%)

`window_server_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 31.90 | 28.92 … 35.91 | 36.26 | 32.24 ± 3.51 | 28.84 … 36.61 | 6 (6) | 29.01, 28.84, 36.61, 35.22, 29.51, 34.29 |
| lokyr-b | 34.59 | 32.45 … 35.57 | 35.67 | 34.21 ± 1.76 | 30.87 … 35.77 | 6 (6) | 30.87, 35.77, 34.23, 34.95, 35.38, 34.04 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-motion-lokyr-a` | 11:29:31 | 105 | done | 2.19 → 8.34 | wdavdaemon_enterprise 36 %, wdavdaemon 22 %, epsext 18 % |
| `c1-r1-motion-lokyr-b` | 11:31:18 | 90 | done | 7.75 → 4.57 | wdavdaemon_enterprise 48 %, wdavdaemon 18 %, epsext 17 % |
| `c2-r1-motion-lokyr-b` | 11:32:50 | 85 | done | 4.29 → 4.74 | wdavdaemon_enterprise 32 %, wdavdaemon 22 %, epsext 19 % |
| `c2-r1-motion-lokyr-a` | 11:34:17 | 85 | done | 4.74 → 6.27 | wdavdaemon_enterprise 22 %, wdavdaemon 16 %, epsext 14 % |
| `c3-r1-motion-lokyr-a` | 11:35:43 | 90 | done | 5.77 → 22.76 | wdavdaemon_enterprise 27 %, wdavdaemon 14 %, epsext 12 % |
| `c3-r1-motion-lokyr-b` | 11:37:15 | 85 | done | 21.02 → 10.00 | wdavdaemon_enterprise 35 %, wdavdaemon 21 %, epsext 18 % |
| `c4-r1-motion-lokyr-b` | 11:38:42 | 90 | done | 10.00 → 10.66 | Google Chrome Helper (Aperitif Renderer) 88 %, wdavdaemon_enterprise 44 %, wdavdaemon 22 % |
| `c4-r1-motion-lokyr-a` | 11:40:14 | 85 | done | 10.66 → 8.23 | wdavdaemon_enterprise 39 %, wdavdaemon 21 %, epsext 18 % |
| `c5-r1-motion-lokyr-a` | 11:41:41 | 88 | done | 7.97 → 6.00 | wdavdaemon_enterprise 33 %, wdavdaemon 21 %, epsext 17 % |
| `c5-r1-motion-lokyr-b` | 11:43:10 | 85 | done | 5.60 → 7.28 | wdavdaemon_enterprise 33 %, wdavdaemon 15 %, epsext 14 % |
| `c6-r1-motion-lokyr-b` | 11:44:37 | 84 | done | 7.28 → 7.31 | wdavdaemon_enterprise 37 %, wdavdaemon 20 %, epsext 17 % |
| `c6-r1-motion-lokyr-a` | 11:46:03 | 87 | done | 7.31 → 6.89 | wdavdaemon_enterprise 21 %, wdavdaemon 16 %, epsext 14 % |
