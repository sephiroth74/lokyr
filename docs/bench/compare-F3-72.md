# Lokyr again, against the other terminals of the first comparison

Measured on 2026-11-08, between 11:49 and 11:55, unattended: Lokyr alone, the build of 8 November, of commit 883c868, of [the first comparison](compare-F2-134.md)'s tool and conditions, `compare.py`'s `check`, `startup`, `idle`, `throughput` and `energycat` of its terminal `lokyr`, so that the comparison's charts take Lokyr's figures of now beside the other terminals', which the first comparison measured once of their versions then and which are not measured again. The session's script, [`run.sh`](compare-F3-72/run.sh), runs no `setup`, which writes iTerm2's and Terminal.app's settings: Lokyr's config of the first comparison alone, in the XDG directory of the measures, and the empty ZDOTDIR. Its raw results are in [compare-F3-72](compare-F3-72/), in the first comparison's layout; `python3 tools/bench/report_charts.py compare` draws the charts, Lokyr's of this session, the others' of the first comparison's.

## Conditions

As the first comparison's: Menlo at 13 points, no ligatures, 120 × 40 cells, which `check` saw, a steady cursor, an opaque window, about 10 000 rows of scrollback, a plain zsh. The machine the same, an Apple M5 Pro, macOS 27.2, on AC power; the main display a Dell P2721Q at 60 Hz, where the first comparison's was a panel of 6016 × 3384 at 60 Hz; the load average 1.6 to 2.9. The first comparison's sessions ran with other apps open and a scanner at work, which moved a session's figures, as its report tells: Lokyr's figures here against the others' there compare within those few percent.

## Results

| Measure | Lokyr, 31 October | Lokyr, 8 November | Its place of 7 | The best of the others, 31 October |
|---|---|---|---|---|
| Startup to the first window, median of 10 | 256.2 ms | 235.9 ms | 1st | Ghostty, 330.6 ms |
| Memory at rest, without the GPU's | 25.0 MB | 26.7 MB | 1st | Alacritty, 31.3 MB |
| Interrupt wakeups at rest | 0.38 a second | 0.40 a second | 3rd | Alacritty, 0.37 |
| `cat` of 1 GiB on screen, median of 5 | 295.5 MiB/s | 315.4 MiB/s | 1st | Ghostty, 305.6 MiB/s |
| The CPU's energy to show that GiB | 17.3 J | 16.0 J | 1st | kitty, 41.1 J |

- **The startup** is 20 ms shorter than the first comparison's, the first window at 236 ms and the first frame at 234 to 236 ms, as the releases' sessions found of 0.5.1, 235 ms: the first comparison's machine was the busier, as its report tells.
- **The memory** is 1.7 MB larger, of F3's features, as the releases' sessions found, 26.1 MB of 0.5.1 and 26.7 of this build.
- **The wakeups** are 0.40 a second of one run of 60 s, where the releases' sessions find 0.33 to 0.35 of two each: within a run's spread, and below the budget of 0.5; Alacritty's 0.37 and Terminal.app's 0.38 come first.
- **The throughput of `cat`** is 7 % higher than the first comparison's, 315 MiB/s against 296, ahead of Ghostty's 306 of then; the releases' sessions find 318 to 330 MiB/s of `tools/bench/onscreen.py` alone.
- **The energy** to show `cat` of 1 GiB is 16.0 J against 17.3.
- The latency, of Typometer, is the first comparison's, of Lokyr of 1 November, measured by hand, still to measure again.
