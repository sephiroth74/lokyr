# ttybench session ab-5-scroll-2026-11-24-1859

Measured from 2026-11-24 18:59 to 19:04 on Mac17,8 (Apple M5 Pro), macOS 27.2.1, the main display at 60 Hz. 16 runs: 16 done, 0 failed, 0 skipped.

| Terminal | Version |
|---|---|
| lokyr-a | 1.4.0 (202611220721) |
| lokyr-b | 1.4.0 (202611241345) |

## A/B: lokyr-b against lokyr-a

Each metric's median of A, lokyr-a, and of B, lokyr-b; the change, B's less A's, and its 95 % bootstrap interval, of the runs resampled, then their samples, as a run moves as a whole of the machine's state: a change whose interval holds 0 is within the noise, and a copy of fewer than two runs tells too few.

| Metric | A | B | Change | 95 % interval | |
|---|---|---|---|---|---|
| scroll · Frames a second that moved the text (frames/s) | 15.87 | 15.87 | +0.00 (+0.0 %) | +0.00 … +0.00 | within the noise |
| scroll · Frames a second that moved the text (frames/s), of `smooth-scroll` | 60.74 | 60.74 | +0.00 (+0.0 %) | -0.15 … +0.15 | within the noise |
| scroll · The gap between two frames (ms) | 50.01 | 50.04 | +0.03 (+0.1 %) | -1.45 … +1.54 | within the noise |
| scroll · The gap between two frames (ms), of `smooth-scroll` | 16.06 | 16.05 | -0.01 (-0.0 %) | -0.45 … +0.42 | within the noise |

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
| lokyr-a | 60.74 | 60.60 … 60.89 | 60.88 | 60.74 ± 0.13 | 60.60 … 60.89 | 4 (4) | 60.89, 60.69, 60.79, 60.60 |
| lokyr-b | 60.74 | 60.69 … 60.79 | 60.79 | 60.74 ± 0.06 | 60.69 … 60.79 | 4 (4) | 60.79, 60.69, 60.79, 60.69 |

### The gap between two frames (ms)

`frame_gap_ms`. Conditions: amplitude 200.0, awake true, by pixels, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0, source window.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 50.01 | 48.97 … 50.97 | 138.57 | 62.53 ± 35.56 | 14.82 … 183.70 | 636 (4) | 50.89, 49.98, 50.06, 48.97 |
| lokyr-b | 50.04 | 48.79 … 51.43 | 138.79 | 62.50 ± 35.68 | 14.17 … 162.32 | 636 (4) | 49.86, 50.44, 49.84, 50.13 |

### The gap between two frames (ms), of `smooth-scroll`

`frame_gap_ms`. Of the feature `smooth-scroll`. Conditions: amplitude 200.0, awake true, by pixels, rate 120.0, refresh_hz 60.0, rounds 1, seconds 10.0, source window.

| Terminal | Median | 95 % interval of the median | p95 | Mean ± sd | Min … max | Samples (runs) | Each cycle's median |
|---|---|---|---|---|---|---|---|
| lokyr-a | 16.06 | 15.80 … 16.37 | 26.77 | 16.45 ± 6.77 | 4.54 … 47.89 | 2446 (4) | 16.14, 16.08, 15.95, 16.27 |
| lokyr-b | 16.05 | 15.76 … 16.40 | 26.68 | 16.45 ± 6.72 | 4.70 … 46.41 | 2446 (4) | 15.91, 16.14, 16.30, 15.91 |

## Runs

Each run's end, the load average of a minute as it started and as it ended, and the busiest processes as it ended, of their share of a core over about the last minute: a run beside another program's work, as a scan of the files, may measure otherwise.

| Run | Started | Seconds | | Load | Busiest as it ended |
|---|---|---|---|---|---|
| `c1-r1-scroll-lokyr-a` | 18:59:53 | 15 | done | 3.99 → 3.70 | WindowServer 46 %, mds 20 %, Code Helper 18 % |
| `c1-r1-scroll-lokyr-b` | 19:00:11 | 15 | done | 3.64 → 3.95 | WindowServer 47 %, wdavdaemon_unprivileged 22 %, BiomeAgent 19 % |
| `c1-r2-scroll-lokyr-a` | 19:00:27 | 15 | done | 3.95 → 3.21 | Code Helper (Renderer) 96 %, WindowServer 40 %, donotdisturbd 23 % |
| `c1-r2-scroll-lokyr-b` | 19:00:44 | 15 | done | 3.20 → 3.00 | WindowServer 42 %, wdavdaemon_unprivileged 20 %, Code Helper 19 % |
| `c2-r1-scroll-lokyr-b` | 19:01:01 | 15 | done | 3.00 → 2.57 | mds 55 %, WindowServer 42 %, donotdisturbd 20 % |
| `c2-r1-scroll-lokyr-a` | 19:01:18 | 15 | done | 2.57 → 2.50 | mds 96 %, WindowServer 44 %, mds_stores 20 % |
| `c2-r2-scroll-lokyr-b` | 19:01:35 | 15 | done | 2.30 → 2.17 | Code Helper (Renderer) 100 %, WindowServer 40 %, mds 36 % |
| `c2-r2-scroll-lokyr-a` | 19:01:52 | 15 | done | 2.17 → 2.31 | WindowServer 38 %, BiomeAgent 27 %, donotdisturbd 22 % |
| `c3-r1-scroll-lokyr-a` | 19:02:08 | 15 | done | 2.31 → 2.80 | WindowServer 46 %, Code Helper (Renderer) 46 %, mds 21 % |
| `c3-r1-scroll-lokyr-b` | 19:02:25 | 15 | done | 2.90 → 4.98 | mds 77 %, WindowServer 44 %, wdavdaemon_unprivileged 25 % |
| `c3-r2-scroll-lokyr-a` | 19:02:42 | 15 | done | 4.98 → 4.63 | mds 41 %, WindowServer 41 %, DisplaysExt 23 % |
| `c3-r2-scroll-lokyr-b` | 19:02:59 | 15 | done | 4.63 → 5.20 | Code Helper (Renderer) 98 %, mds 92 %, WindowServer 45 % |
| `c4-r1-scroll-lokyr-b` | 19:03:16 | 15 | done | 4.94 → 4.66 | WindowServer 43 %, Code Helper 18 %, Code Helper (Renderer) 18 % |
| `c4-r1-scroll-lokyr-a` | 19:03:33 | 15 | done | 4.66 → 3.76 | mds 77 %, WindowServer 46 %, MenuBarAgent 19 % |
| `c4-r2-scroll-lokyr-b` | 19:03:50 | 15 | done | 3.70 → 4.79 | WindowServer 42 %, mds 29 %, Code Helper (Renderer) 22 % |
| `c4-r2-scroll-lokyr-a` | 19:04:06 | 15 | done | 4.79 → 5.37 | Code Helper (Renderer) 65 %, mds 54 %, WindowServer 45 % |
