# ttybench session ab-5-scroll-2026-11-28-1046

Measured from 2026-11-28 10:46 to 10:51 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.5.0 (202611250509) |
| lokyr-b | 1.5.0 (202611280854) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b, or the statistic that the metric names, as the style chooses; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| scroll · Frames a second that moved the text (frames/s) | 15.87 | 15.87 | +0.00 (+0.0 %) | +0.00 … +0.00 | within the noise |
| scroll · Frames a second that moved the text (frames/s), of `smooth-scroll` | 60.79 | 60.84 | +0.05 (+0.1 %) | +0.00 … +0.25 | within the noise |
| scroll · The gap between two frames (ms) | 47.38 | 47.09 | -0.29 (-0.6 %) | -2.19 … +2.67 | within the noise |
| scroll · The gap between two frames (ms), of `smooth-scroll` | 16.39 | 16.45 | +0.06 (+0.4 %) | -0.02 … +0.21 | within the noise |

## scroll

### Frames a second that moved the text (frames/s)

`frames_per_s`. Conditions: amplitude 200.0, awake true, by pixels, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0, source window.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 15.87 | 15.87 … 15.87 | 15.87 | 15.87 ± 0.00 | 15.87 … 15.87 | 4 (4) | 15.87, 15.87, 15.87, 15.87 |
| lokyr-b | 15.87 | 15.87 … 15.87 | 15.87 | 15.87 ± 0.00 | 15.87 … 15.87 | 4 (4) | 15.87, 15.87, 15.87, 15.87 |

### Frames a second that moved the text (frames/s), of `smooth-scroll`

`frames_per_s`. Of the feature `smooth-scroll`. Conditions: amplitude 200.0, awake true, by pixels, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0, source window.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 60.79 | 60.60 … 60.79 | 60.79 | 60.74 ± 0.10 | 60.60 … 60.79 | 4 (4) | 60.79, 60.60, 60.79, 60.79 |
| lokyr-b | 60.84 | 60.79 … 60.89 | 60.89 | 60.84 ± 0.06 | 60.79 … 60.89 | 4 (4) | 60.79, 60.89, 60.79, 60.89 |

### The gap between two frames (ms)

`frame_gap_ms`. Conditions: amplitude 200.0, awake true, by pixels, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0, source window.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 47.38 | 45.89 … 49.08 | 138.72 | 62.53 ± 34.82 | 23.58 … 157.42 | 636 (4) | 47.53, 47.85, 47.00, 47.39 |
| lokyr-b | 47.09 | 45.80 … 49.82 | 136.41 | 62.51 ± 34.52 | 25.17 … 154.21 | 636 (4) | 47.23, 46.69, 48.44, 47.00 |

### The gap between two frames (ms), of `smooth-scroll`

`frame_gap_ms`. Of the feature `smooth-scroll`. Conditions: amplitude 200.0, awake true, by pixels, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0, source window.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 16.39 | 16.25 … 16.46 | 19.74 | 16.45 ± 2.00 | 5.33 … 33.54 | 2446 (4) | 16.44, 16.43, 16.44, 16.11 |
| lokyr-b | 16.45 | 16.42 … 16.50 | 18.76 | 16.43 ± 1.91 | 4.40 … 27.12 | 2450 (4) | 16.46, 16.49, 16.43, 16.43 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-scroll-lokyr-a` | 10:46:47 | 15 | done | 3.75 → 3.53 | Code Helper (Renderer) 22 %, WindowServer 20 %, wdavdaemon_unprivileged 17 % |
| `c1-r1-scroll-lokyr-b` | 10:47:04 | 15 | done | 3.53 → 2.97 | WindowServer 22 %, BiomeAgent 14 %, Code Helper (Renderer) 12 % |
| `c1-r2-scroll-lokyr-a` | 10:47:21 | 15 | done | 2.97 → 3.37 | WindowServer 43 %, wdavdaemon_unprivileged 23 %, Code Helper (Renderer) 22 % |
| `c1-r2-scroll-lokyr-b` | 10:47:38 | 15 | done | 3.18 → 2.78 | WindowServer 41 %, Code Helper (Renderer) 23 %, BiomeAgent 21 % |
| `c2-r1-scroll-lokyr-b` | 10:47:55 | 15 | done | 2.78 → 2.47 | BiomeAgent 33 %, WindowServer 24 %, wdavdaemon 18 % |
| `c2-r1-scroll-lokyr-a` | 10:48:12 | 15 | done | 2.43 → 3.54 | WindowServer 23 %, Code Helper (Renderer) 20 %, MenuBarAgent 19 % |
| `c2-r2-scroll-lokyr-b` | 10:48:29 | 15 | done | 3.54 → 3.21 | WindowServer 37 %, epsext 36 %, Python 35 % |
| `c2-r2-scroll-lokyr-a` | 10:48:45 | 15 | done | 3.21 → 3.30 | epsext 52 %, Python 44 %, WindowServer 31 % |
| `c3-r1-scroll-lokyr-a` | 10:49:02 | 15 | done | 3.44 → 3.86 | epsext 58 %, Python 57 %, wdavdaemon 26 % |
| `c3-r1-scroll-lokyr-b` | 10:49:19 | 15 | done | 3.86 → 3.67 | Code Helper (Renderer) 101 %, epsext 54 %, Python 47 % |
| `c3-r2-scroll-lokyr-a` | 10:49:36 | 15 | done | 3.67 → 3.44 | epsext 48 %, Python 43 %, WindowServer 30 % |
| `c3-r2-scroll-lokyr-b` | 10:49:52 | 15 | done | 3.24 → 3.11 | wdavdaemon_unprivileged 121 %, epsext 42 %, wdavdaemon 40 % |
| `c4-r1-scroll-lokyr-b` | 10:50:09 | 15 | done | 3.11 → 3.17 | epsext 56 %, Python 53 %, wdavdaemon_unprivileged 38 % |
| `c4-r1-scroll-lokyr-a` | 10:50:26 | 15 | done | 3.17 → 3.51 | Python 58 %, epsext 54 %, wdavdaemon 26 % |
| `c4-r2-scroll-lokyr-b` | 10:50:43 | 15 | done | 3.31 → 5.36 | WindowServer 37 %, Python 28 %, epsext 26 % |
| `c4-r2-scroll-lokyr-a` | 10:50:59 | 15 | done | 5.36 → 5.28 | WindowServer 40 %, epsext 31 %, Python 28 % |
