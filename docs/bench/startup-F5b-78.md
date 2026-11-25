# The first windows first

Measured on 25 November 2026, from 14:49 to 15:05, from a terminal with VS Code closed. Lokyr's start built the menu bar twice before its first window: once as the plugins' activation ended, and again after it. It also took its global keys twice, and set the Dock's icon, watched the config and started the updates' checks before that window. When the system started Lokyr to open a script, a folder or a link, that window opened only after all of this. The start now opens its first windows first, and sets the menu bar and the global keys once after them, together with the Dock's icon, the config's watch and the updates. This session measures that change against 1.5.0. Its script, [`run.sh`](startup-F5b-78/run.sh), its analysis, [`analyze.py`](startup-F5b-78/analyze.py), the results and [the tables](startup-F5b-78/analysis.txt) are in [startup-F5b-78](startup-F5b-78/).

## Method

The copies were two bundles signed alike, neither notarized: 1.5.0's tag (A) and the change's (B). Both used the sessions' [base config](../../tools/bench/sessions/configs/base/lokyr/config.toml), which leaves the updates' checks on.

- **ttybench**, [its session's report](startup-F5b-78/ttybench-2026-11-25-1449/report.md): 24 rounds of each copy in two cycles, AB BA, after each copy and the floor app were launched 20 times. Each round launches Lokyr by `open` of a script and the floor app alike, so every launch opens a script, with the same pause of 2 s before each launch.
- **lokyr-bench**: each copy's first frame on the display, its executable started directly, in four rounds of ten starts after two, AB BA BA AB.

The Mac was an Apple M5 Pro on AC power, the display a Dell U3417W at 60 Hz. The load average was 2.9 at the start and 1.2 to 4.8 later, with other apps open in the background.

## Results

| Measure | A, 1.5.0 | B | B − A, its 95 % interval |
|---|---|---|---|
| The window on screen, ttybench's median | 318.4 ms | 311.3 ms | **−7.1 ms**, −16.9 to −0.9 |
| The script running, ttybench's median | 657.3 ms | 645.5 ms | **−11.8 ms**, −19.5 to −4.8 |
| The first frame, lokyr-bench's mean of four rounds | 242.7 ms | 231.3 ms | **−11.3 ± 9.6 ms** |
| The window after the floor app's, ttybench's median | 60.3 ms | 58.1 ms | −2.2 ms, −18.4 to +10.8 |

- **The window shows 7 ms sooner when the start opens a script**, and the script runs 12 ms sooner. Both changes are beyond the noise.
- **The first frame comes 11 ms sooner on a default start.** In two rounds of four it came a whole refresh sooner, at 219 ms against 240 and 236 against 254.
- **The gap to the floor app moved by 2 ms, within its noise.** Over 24 rounds, the floor's own spread of some 16 ms between rounds hides a change of 7 ms in the gap, while the window's own figure shows it.

## What is left before the first window

Most of what remains before the first window is AppKit's own work. Creating the window takes some 25 ms. The window also comes later than the floor app's because of when it is made: the floor app shows its window before AppKit's launch runs, while Lokyr makes its window as that launch ends, once it knows whether the system started it to open files. The catalogs of the texts that Lokyr shows take 0.09 ms once their code is in memory, so reading them on another thread would gain little more than that code's first load.
