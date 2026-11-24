# ttybench session drag-check-2026-11-24-1926

Measured from 2026-11-24 19:26 to 19:39 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 8 runs: 8 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| motion · Frames a second of the motion (frames/s) | 57.87 | 58.13 | +0.26 (+0.5 %) | -1.17 … +1.38 | within the noise |
| motion · Frames missed (frames) | 20 | 17.5 | -2.5 (-12.5 %) | -13 … +11 | within the noise |
| motion · The compositor's delay (ms) | 12.72 | 12.45 | -0.26 (-2.1 %) | -0.45 … +0.08 | within the noise |
| motion · Its CPU (%) | 4.24 | 4.46 | +0.21 (+5.0 %) | -0.27 … +0.74 | within the noise |
| motion · WindowServer's CPU (%) | 34.19 | 35.57 | +1.37 (+4.0 %) | -1.11 … +3.18 | within the noise |

## motion

### Frames a second of the motion (frames/s)

`frames_per_s`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 57.87 | 57.54 … 58.93 | 58.80 | 58.05 ± 0.63 | 57.54 … 58.93 | 4 (4) | 58.93, 57.54, 57.66, 58.08 |
| lokyr-b | 58.13 | 57.33 … 59.04 | 59.02 | 58.16 ± 0.95 | 57.33 … 59.04 | 4 (4) | 58.93, 59.04, 57.34, 57.33 |

### Frames missed (frames)

`missed_frames`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 20 | 10 … 23 | 22.9 | 18.2 ± 5.9 | 10 … 23 | 4 (4) | 10, 23, 22, 18 |
| lokyr-b | 17.5 | 9 … 25 | 25 | 17.2 ± 9.0 | 9 … 25 | 4 (4) | 10, 9, 25, 25 |

### The compositor's delay (ms)

`compositor_delay_ms`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 12.72 | 12.41 … 12.81 | 13.60 | 12.60 ± 1.47 | 4.79 … 28.60 | 2183 (4) | 12.75, 12.25, 12.79, 12.81 |
| lokyr-b | 12.45 | 12.32 … 12.66 | 13.71 | 12.84 ± 2.72 | 6.75 … 30.65 | 2187 (4) | 12.70, 12.33, 12.62, 12.31 |

### Its CPU (%)

`terminal_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 4.24 | 3.96 … 4.40 | 4.38 | 4.21 ± 0.19 | 3.96 … 4.40 | 4 (4) | 4.27, 4.40, 4.22, 3.96 |
| lokyr-b | 4.46 | 3.98 … 4.99 | 4.92 | 4.47 ± 0.42 | 3.98 … 4.99 | 4 (4) | 3.98, 4.99, 4.38, 4.53 |

### WindowServer's CPU (%)

`window_server_cpu_percent`. Conditions: amplitude 200.0, awake true, kind drag, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 34.19 | 33.03 … 36.67 | 36.39 | 34.52 ± 1.61 | 33.03 … 36.67 | 4 (4) | 34.77, 36.67, 33.61, 33.03 |
| lokyr-b | 35.57 | 34.07 … 37.06 | 36.89 | 35.56 ± 1.26 | 34.07 … 37.06 | 4 (4) | 34.07, 37.06, 35.18, 35.95 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-motion-lokyr-a` | 19:26:36 | 101 | done | 2.84 → 5.58 | wdavdaemon_enterprise 29 %, wdavdaemon 24 %, epsext 20 % |
| `c1-r1-motion-lokyr-b` | 19:28:19 | 92 | done | 5.58 → 7.52 | wdavdaemon 17 %, epsext 16 %, wdavdaemon_enterprise 15 % |
| `c2-r1-motion-lokyr-b` | 19:29:52 | 92 | done | 6.92 → 5.32 | wdavdaemon 19 %, epsext 15 %, wdavdaemon_enterprise 14 % |
| `c2-r1-motion-lokyr-a` | 19:31:27 | 93 | done | 4.97 → 5.33 | wdavdaemon 25 %, epsext 19 %, wdavdaemon_enterprise 19 % |
| `c3-r1-motion-lokyr-a` | 19:33:02 | 92 | done | 4.98 → 5.25 | wdavdaemon 19 %, wdavdaemon_enterprise 14 %, epsext 14 % |
| `c3-r1-motion-lokyr-b` | 19:34:35 | 102 | done | 5.25 → 5.80 | wdavdaemon 20 %, epsext 16 %, wdavdaemon_enterprise 15 % |
| `c4-r1-motion-lokyr-b` | 19:36:20 | 90 | done | 5.80 → 10.67 | wdavdaemon 26 %, epsext 20 %, wdavdaemon_enterprise 19 % |
| `c4-r1-motion-lokyr-a` | 19:37:52 | 92 | done | 9.89 → 7.90 | wdavdaemon 23 %, epsext 18 %, wdavdaemon_enterprise 18 % |
