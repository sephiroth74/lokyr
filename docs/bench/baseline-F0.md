# Baseline of existing terminals (F0)

Measured on 2026-10-12 with `bolt-bench`, to check the [budgets](../PERFORMANCE.md#budgets) against the terminals people use today. The raw reports are in [baseline-F0/](baseline-F0/).

To repeat the measurement:
- run `tools/bench/baseline.sh <app> <name> <dir>` once per terminal;
- then print the tables with `tools/bench/baseline-summary.sh <dir>`.

## Method

For each terminal, `baseline.sh`:
1. times the startup over 10 launches, after 2 warmup launches (`bolt-bench startup`);
2. launches the terminal once more, and leaves it idle for 10 s;
3. samples its memory once a second for 10 s (`bolt-bench memory`);
4. counts its wakeups for 60 s (`bolt-bench idle`), with its window in front and nobody at the keyboard;
5. quits it.

The startup time runs from the start of the process to its first window on screen, which approximates the first presented frame. The footprint is `phys_footprint`, and the probe reports GPU memory separately.

A minimal AppKit app, the F0 spike's `Spike.app`, is measured the same way. It opens one empty window and does nothing else, so it gives the floor that macOS itself sets on this machine.

## Machine

- Apple M5 Pro (20 cores), 64 GB of memory, macOS 27.0.1, on AC power, normal power mode.
- Display: 3840 × 2160 at 60 Hz, without scaling, so points and pixels are the same.

## Terminals and their settings

| Terminal | Version | Font | Settings | Window (points) |
|---|---|---|---|---|
| Terminal.app | 2.15 (488) | SF Mono Terminal 12 | The installed "Clear Dark" profile, 120 × 30 | 877 × 499 |
| iTerm2 | 3.7.4 | BlexMono Nerd Font Mono 22 | The installed profile: 80 × 25, blur on, 10% transparency. It restored two earlier windows | 987 × 717, twice |
| Ghostty | Development build 62fa23bfa (18019) | Agave Nerd Font Mono 20 | The installed config: ligatures and blur on, 90% opacity | 1521 × 925 |
| kitty | 0.49.2 | Default: `monospace` 11 | Defaults | 889 × 612 |
| WezTerm | 20240203-110809-5046fc22 | Default: JetBrains Mono 12 | Defaults | 574 × 453 |
| Alacritty | 0.17.0, built from source | Default: Menlo 11.25 | Defaults | 800 × 632 |
| Minimal AppKit app | The F0 spike's `Spike.app` | — | One empty window | 640 × 432 |

- **Deviations from the [comparison method](../PERFORMANCE.md#comparing-with-other-terminals)**:
  - Fonts, font sizes and window sizes differ, and some terminals have blur or ligatures on. Matching them would have meant changing the machine's own settings for Terminal, iTerm2 and Ghostty.
  - The shell is the machine's zsh, with its prompt.
  - iTerm2 ran with the two windows it restored.
- **Sources**: kitty and WezTerm come from Homebrew. Homebrew disabled the Alacritty cask on 2026-09-01, because the app does not pass Gatekeeper, so Alacritty was built from source with the steps of its `make app`.

## Results

### Startup

| Terminal | First window, median | p90 | Spread | From the launch request, median | First launch |
|---|---|---|---|---|---|
| Minimal AppKit app | 176 ms | 203 ms | 20.3% | 198 ms | 780 ms |
| Terminal.app | 303 ms | 305 ms | 4.5% | 321 ms | 1631 ms |
| Alacritty | 312 ms | 324 ms | 8.0% | 331 ms | 1473 ms |
| Ghostty | 331 ms | 513 ms | 57.5% | 356 ms | 3007 ms |
| WezTerm | 372 ms | 380 ms | 7.1% | 394 ms | 1434 ms |
| kitty | 761 ms | 824 ms | 11.8% | 793 ms | 3143 ms |
| iTerm2 | 791 ms | 911 ms | 18.0% | 813 ms | 2852 ms |

- **Spread**: the range from the 10th to the 90th percentile, as a fraction of the median.
- **Ghostty is bimodal**: seven launches took 322–357 ms, the other three 485–528 ms.
- **First launch**: the first warmup launch, after the terminal had been closed for a while. It is 2–10 times slower than the following ones, and the warmup runs keep it out of the statistics.
- **LaunchServices** adds about 20 ms to every launch.

### Memory

| Terminal | Footprint | GPU memory | Footprint without GPU memory |
|---|---|---|---|
| Minimal AppKit app | 19.1 MB | 0.1 MB | 19.0 MB |
| Alacritty | 49.8 MB | 16.4 MB | 33.6 MB |
| Terminal.app | 38.6 MB | 5.0 MB | 33.7 MB |
| Ghostty | 150.5 MB | 112.4 MB | 38.1 MB |
| WezTerm | 49.4 MB | 7.8 MB | 41.6 MB |
| kitty | 181.0 MB | 113.5 MB | 67.5 MB |
| iTerm2, two windows | 244.5 MB | 126.3 MB | 118.2 MB |

- Medians of 10 samples, 10 s after the launch.
- **GPU memory** varies by a factor of 25. In Ghostty, kitty and iTerm2, most of it (89–97 MB) is GPU memory that the process owns without mapping it, such as textures and glyph atlases.

### Idle

| Terminal | Package idle wakeups | Interrupt wakeups |
|---|---|---|
| Minimal AppKit app | 0/s | 0.02/s |
| Alacritty | 0/s | 0.37/s |
| kitty | 0/s | 0.48/s |
| Terminal.app | 0/s | 0.55/s |
| WezTerm | 0/s | 1.82/s |
| iTerm2 | 0/s | 9.22/s |
| Ghostty | 0.03/s | 16.55/s |

- Package idle wakeups bring the CPU out of idle: they are what `top -stats idlew` counts. Almost every terminal has none.
- Interrupt wakeups count every wakeup caused by an interrupt, such as a timer. They tell the terminals apart.

## Against the budgets

| Budget | Before | Baseline | Decision |
|---|---|---|---|
| First presented frame | ≤ 100 ms warm, ≤ 250 ms cold | The minimal AppKit app shows its window after 176 ms. The fastest terminals take 303–312 ms, and first launches 0.8–3.1 s | **≤ 200 ms warm**. The cold target will be set in F2, after measuring it with `sudo purge` |
| Footprint without GPU memory, 1 window and 1 pane | ≤ 30 MB | Floor 19.0 MB; the leanest terminals, Alacritty and Terminal.app, are at 33.6 MB | **≤ 35 MB**, on a par with the leanest |
| Idle | 0 wakeups/s; ≤ 2/s with a blinking cursor | Package idle wakeups are almost always 0; interrupt wakeups range from 0.02/s to 16.55/s | **0 package idle wakeups and ≤ 0.5 interrupt wakeups per second**; ≤ 2/s with a blinking cursor stays |

- The project owner took these decisions on 2026-10-12, and [PERFORMANCE.md](../PERFORMANCE.md#budgets) applies them.
- GPU memory keeps its budget in drawables per window. Its total, which the glyph atlases dominate in some terminals, will be revisited with the F2 renderer.
