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

   The startup's gap is ttybench's from 1.6.0 on, each round's window less ttybench's floor app's, launched alike, of `ab/1-start`; the gap to a minimal app's first window keeps its values up to 1.5.0. The memory without the GPU's, `seq`'s throughput and a frame's time on the CPU have no ttybench record: they keep their values of the earlier tools, which their rows tell.

The budgets of the steps are the releases': the startup's window 70 ms at most after the floor app's, of the median of its rounds, the latency's 2 refreshes at the median and 4.5 at the 99th percentile of a display at 60 Hz, the interrupt wakeups at rest 0.5 a second at most and the package's none, of the mean of their intervals, as a wakeup comes a few times a minute and most intervals of a second hold none, and `cat`'s 275 MiB/s.

## The scenarios of a release's features

A feature that the release before lacks has no A/B of the two copies: its scenario measures the copy measured, B, against itself, of a config whose plugin builds the scenario as Lokyr starts. Each scenario's config in `configs/<scenario>` names its plugin of [`scenarios/plugins`](scenarios/plugins) in its `plugins.lock`, as a folder that `sleip dev` would name, and grants its capabilities in its `permissions.toml`, so that Lokyr asks nothing; `just bench-prepare` puts both in place.

- [`scenarios/broadcast.toml`](scenarios/broadcast.toml): a key's latency of B alone, and of B whose typed input goes to eight panes of the tab: the measured pane above, of the window's whole width, and seven below, each of a program that takes the keys and draws nothing, so that the capture sees the measured pane alone while every key goes to eight terminals.
- [`scenarios/hidden-session.toml`](scenarios/hidden-session.toml): a window at rest of B alone, and of B whose second session's window hides, of a program at rest in its shell's place, as a login shell's prompt may wake each second of its own.
- [`scenarios/floating.toml`](scenarios/floating.toml): a window at rest, its memory and its frames, of B alone and of B of a popup of `cat` at rest over its pane.

```sh
just bench-session scenarios/broadcast
just bench-session scenarios/hidden-session
just bench-session scenarios/floating
```

## The terminals compared

Give Terminal.app's and iTerm2's default profiles the configs' font, size and window by hand, as they read no config folder, then run the four steps of [`compare/`](compare). Six terminals take six cycles to balance their orders: each step runs for a while, the Mac left alone.

## What ttybench replaces

ttybench measures the startup, the memory, the wakeups at rest, the latency, the throughput on screen, the scroll and the window's motion of any terminal alike. The scripts of `tools/bench` that measured them, `compare.py`, `onscreen.py`, `onscreen_compare.py`, `motion_extract.py`, `powermetrics_extract.py`, the Typometer driver and `lokyr-bench`'s `startup`, `idle`, `memory` and `motion`, retire once a release's A/B has measured with ttybench; `lokyr-bench`'s frames on the CPU, the startup's floor of a minimal app and `releases.py` stay.
