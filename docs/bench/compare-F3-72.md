# Lokyr again, against the other terminals of F2-134 (after F3-72)

Measured on 2026-11-08, between 11:49 and 11:55, the user away: Lokyr alone, the build after F3-72, of commit 24aaaaa, of [F2-134](compare-F2-134.md)'s tool and conditions, `compare.py`'s `check`, `startup`, `idle`, `throughput` and `energycat` of its terminal `lokyr`, so that the comparison's charts take Lokyr's figures of now beside the other terminals', which F2-134 measured once of their versions then and which are not measured again. The session's script, [`run.sh`](compare-F3-72/run.sh), runs no `setup`, which writes iTerm2's and Terminal.app's settings: Lokyr's config of F2-134 alone, in the XDG directory of the measures, and the empty ZDOTDIR. Its raw results are in [compare-F3-72](compare-F3-72/), in F2-134's layout; `python3 tools/bench/report_charts.py compare` draws the charts, Lokyr's of this session, the others' of F2-134's.

## Conditions

As F2-134's: Menlo at 13 points, no ligatures, 120 × 40 cells, which `check` saw, a steady cursor, an opaque window, about 10 000 rows of scrollback, a plain zsh. The machine the same, an Apple M5 Pro, macOS 27.2, on AC power; the main display a Dell P2721Q at 60 Hz, where F2-134's was a panel of 6016 × 3384 at 60 Hz; the load average 1.6 to 2.9. F2-134's sessions ran with the user's apps open and a scanner at work, which moved a session's figures, as its report tells: Lokyr's figures here against the others' there compare within those few percent.

## Results

| Measure | Lokyr, F2-134 | Lokyr, after F3-72 | Its place of 7 | The best of the others, F2-134 |
|---|---|---|---|---|
| Startup to the first window, median of 10 | 256.2 ms | 235.9 ms | 1st | Ghostty, 330.6 ms |
| Memory at rest, without the GPU's | 25.0 MB | 26.7 MB | 1st | Alacritty, 31.3 MB |
| Interrupt wakeups at rest | 0.38 a second | 0.40 a second | 3rd | Alacritty, 0.37 |
| `cat` of 1 GiB on screen, median of 5 | 295.5 MiB/s | 315.4 MiB/s | 1st | Ghostty, 305.6 MiB/s |
| The CPU's energy to show that GiB | 17.3 J | 16.0 J | 1st | kitty, 41.1 J |

- **The startup** is 20 ms shorter than F2-134's, the first window at 236 ms and the first frame at 234 to 236 ms, as the releases' sessions found of 0.5.1, 235 ms: F2-134's machine was the busier, as its report tells.
- **The memory** is 1.7 MB larger, of F3's features, as the releases' sessions found, 26.1 MB of 0.5.1 and 26.7 of this build.
- **The wakeups** are 0.40 a second of one run of 60 s, where the releases' sessions find 0.33 to 0.35 of two each: within a run's spread, and below the budget of 0.5; Alacritty's 0.37 and Terminal.app's 0.38 come first.
- **The throughput of `cat`** is 7 % higher than F2-134's, 315 MiB/s against 296, ahead of Ghostty's 306 of F2-134; the releases' sessions find 318 to 330 MiB/s of `tools/bench/onscreen.py` alone.
- **The energy** to show `cat` of 1 GiB is 16.0 J against 17.3.
- The latency, of Typometer, is F2-134's, of Lokyr of 1 November, measured by hand: the user's to measure again.
