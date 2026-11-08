# The cursor's movement's cost (F3-72)

Measured on 2026-11-08 in one session, between 11:20 and 11:31, the user away: one build, of commit 24aaaaa, the app of `just bundle`, of `[cursor] movement = "instant"` against `"snappy"`. The reports are in [movement-F3-72](movement-F3-72/), with the session's script, [`run.sh`](movement-F3-72/run.sh), its configs, its analysis, [`analyze.py`](movement-F3-72/analyze.py), and its tables, [`analysis.txt`](movement-F3-72/analysis.txt).

## Method

As [F3-190's](ab-F3-190.md#method), of F2-134's config, Menlo at 13 points, 120 × 40 cells, an opaque window: the latency of 200 keys, eight rounds of balanced orders, AB BA BA AB, after a warm-up of 20 keys of each setting, the same pause of 2 s before each run; idle over 60 s and memory over 10 s of the app started with `open -n`, 20 s after its start, two rounds. The latency's program, `cat` in raw mode, echoes each key, so that each moves the cursor a cell, which slides under `"snappy"`.

## Machine

- Apple M5 Pro, macOS 27.2, on AC power, the main display a Dell P2721Q at 60 Hz, a scale of 2.
- The load average 1.3 to 3.2, the user away.

## Results

| Measure | `instant` | `snappy` | Budget |
|---|---|---|---|
| Latency at 60 Hz, p50, the mean of 8 rounds | 19.54 ms | 22.16 ms | ≤ 2 frames |
| Latency at 60 Hz, p99, the mean of 8 rounds | 44.79 ms | 39.59 ms | ≤ 4.5 frames |
| Its samples in the first refresh, of 200 a round | 53 to 87 | 23 to 36 | |
| Its samples in the fourth refresh or later | 6 of 1592 | 1 of 1591 | |
| Idle, package idle wakeups | 0 and 0/s | 0 and 0/s | 0/s |
| Idle, interrupt wakeups | 0.33 and 0.35/s | 0.35 and 0.35/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest | 26.7 and 26.7 MB | 26.7 and 26.7 MB | ≤ 35 MB |
| GPU memory | 30.1 MB | 30.1 MB | |

- **Every budget holds**, the slide's too: the latency's p50 1.33 refreshes, its p99 2.4; at rest, the same wakeups and memory, as the slide takes no frame nor timer of the app's.
- **The slide costs 2.6 ms of the latency's median**, in each of the eight rounds, `snappy`'s p50 22.0 to 22.8 ms against `instant`'s 18.0 to 20.7, whatever the place in the round: a key's echo shows in the first refresh about a sixth of the time against a third, the others a refresh later. The frame tells the cursor's layer its new place with a Core Animation animation of its position, in the same transaction as the grid's drawable; the window server then shows the grid's new frame a refresh later more often. The text that a key changes is not held by the app: its frame is drawn as soon, and shows later.
- **The tail is no longer**: the p99 5 ms shorter of `snappy`, within the spread of the rounds, 4.4 to 5.6 ms, and one sample in the fourth refresh against six.
- `instant` is the default, of no cost; `snappy` takes the cost, which the guide tells.
