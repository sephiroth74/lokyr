# Lokyr's sessions of ttybench

The releases' A/B and the comparison of the terminals, as sessions of [ttybench](https://github.com/sephiroth74/ttybench), the toolkit that measures terminal emulators on macOS: each session a file of the terminals and the tools that it runs, in balanced orders, whose report, charts and page ttybench writes into its folder at its end.

- [`terminals.toml`](terminals.toml): the terminals, `lokyr-a` and `lokyr-b` the two copies of an A/B, `lokyr` the installed app, and the terminals that the comparison measures beside it.
- [`configs/base`](configs/base): each terminal's config, alike: Menlo at 13 points, 120 × 40 cells, no ligatures, a cursor that doesn't blink, an opaque window. Lokyr's is the releases' config of the least work, smooth scrolling off. [`configs/smooth-scroll`](configs/smooth-scroll) turns Lokyr's smooth scrolling on, the feature `smooth-scroll`, which the sessions measure of Lokyr alone and tell skipped of the others.
- [`ab/`](ab): a release's A/B in six steps of a few minutes each, each of its own balanced orders, AB BA BA AB, so that the Mac is free between them, and [`ab/dry.toml`](ab/dry.toml), every step's tools of the least rounds, to check everything end to end first.
- [`compare/`](compare): the terminals compared, in four steps, of six cycles that balance six terminals' orders.

## A release's A/B

1. Build ttybench and put its binaries on the `PATH`, or name its dispatcher in `TTYBENCH`.
2. Give the Mac's app that runs ttybench the Accessibility and the Screen Recording permissions, as ttybench's README tells.
3. Put the two copies and the configs in `/tmp/lokyr/ttybench`: the release before of its DMG, the one measured of its DMG or its bundle. Sign both alike: a copy that is not notarized starts slower than one that is.

   ```sh
   just bench-prepare ~/Downloads/Lokyr-1.4.0.dmg target/Lokyr.app
   ```

4. Run the dry session, then each step, the Mac left alone while it runs:

   ```sh
   just bench-session ab/dry
   just bench-session ab/1-start
   just bench-session ab/2-idle
   just bench-session ab/3-latency
   just bench-session ab/4-output
   just bench-session ab/5-scroll
   just bench-session ab/6-motion
   ```

   Each step's folder, `/tmp/lokyr/ttybench/sessions/ab-<step>-<date>`, holds its `report.md`, its charts and its `index.html`. A step cut off goes on of `ttybench run --resume <folder>`.

5. Give the release's values to [`releases.json`](../../../docs/bench/releases.json), of the measures that name their ttybench record, B's statistic the release's value and A's its baseline, then draw the page and the charts again:

   ```sh
   just bench-import 1.5.0 1.4.0 ab-<report>.md /tmp/lokyr/ttybench/sessions/ab-*
   just release-charts
   ```

   The startup's gap to a minimal app, the memory without the GPU's, `seq`'s throughput and a frame's time on the CPU have no ttybench record: they keep their values of the earlier tools, which their rows tell.

The budgets of the steps are the releases': the latency's 2 refreshes at the median and 4.5 at the 99th percentile of a display at 60 Hz, the interrupt wakeups at rest 0.5 a second at most and the package's none, and `cat`'s 275 MiB/s.

## The terminals compared

Give Terminal.app's and iTerm2's default profiles the configs' font, size and window by hand, as they read no config folder, then run the four steps of [`compare/`](compare). Six terminals take six cycles to balance their orders: each step runs for a while, the Mac left alone.

## What ttybench replaces

ttybench measures the startup, the memory, the wakeups at rest, the latency, the throughput on screen, the scroll and the window's motion of any terminal alike. The scripts of `tools/bench` that measured them, `compare.py`, `onscreen.py`, `onscreen_compare.py`, `motion_extract.py`, `powermetrics_extract.py`, the Typometer driver and `lokyr-bench`'s `startup`, `idle`, `memory` and `motion`, retire once a release's A/B has measured with ttybench; `lokyr-bench`'s frames on the CPU, the startup's floor of a minimal app and `releases.py` stay.
