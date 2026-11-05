# The latency beside a plugin on its thread that renders without pause (F3-35)

Measured on 2026-11-05, between 06:22 and 06:25, of the scripts and the analysis in [latency-F3-35](latency-F3-35/), `build.sh`, `run.sh` and `analyze.py`, with their reports and the session's log: whether a plugin on a thread of its own that keeps the main thread answering leaves the frames on time, as [F3-35](../tasks/F3-macos-mvp.md#f3-35--plugins-on-threads-of-their-own) asks.

## Method

- **Two bundles** of 846e519, the work of F3-35, built as releases of the build's version, `0.4.0-20261105`, and signed with the Developer ID, not notarized: `h`, the app as it ships; `w`, the same with `--features sample-worker`, `lokyr/sample-worker`, a plugin on its thread whose status bar renders without pause, each render reading 24 rows of the focused pane, a request each, and asking for the next render as it returns. Each request costs a turn of the main thread, its wake included.
- **The regime first**, as [F3-135](latency-F3-135.md)'s sessions: a short run of `h`, 50 keys, until its p50 is under 21 ms; it was at the first, 18.6 ms.
- **Six rounds** of F3-29's config, 200 keys a run, each bundle first in three of them; the process's CPU sampled with `ps` six seconds into each run.

## Results

| Bundle | p50, mean of 6 | p99, mean of 6 | Its range | Samples in the third refresh or later | In the fourth or later | The process's CPU |
|---|---|---|---|---|---|---|
| `h`, the app as it ships | 20.1 ms | 46.6 ms | 40.2 to 51.8 ms | 54 of 1194 | 7 | 5 % |
| `w`, beside the plugin that renders without pause | 22.5 ms | 41.1 ms | 40.1 to 42.4 ms | 211 of 1200 | 2 | 105 % |

By the place in the round, of both bundles, the p99's mean is 44.0 and 43.7 ms, first and second, the p50's 21.4 and 21.2 ms.

- **The frames stay on time, within the budget**: beside the plugin, no key's frame came past the fourth refresh, and the p99 was no longer than the app's alone, 41.1 ms against 46.6, its range narrower.
- **The median takes 2.4 ms more**: 18 % of the keys came in the third refresh or later, against 5 %, as the main thread answers the plugin's requests between the keys' turns.
- **The process works at 105 %**, against 5 %: the main thread answers the plugin's requests without pause, and the plugin's thread renders. This plugin never stops, as no plugin of ours would; the cost is the whole main thread's for a plugin that reads in a loop, which a turn per request makes.

## What would lower the cost

Neither is needed for the budget; both would lower the main thread's work beside such a plugin.

- **A render asked within a render waits for the next frame of its window**: at most one render a refresh, as a view shows no more often, rather than one as soon as the last returns.
- **A turn that only the plugins' messages woke** skips what a wake of the sessions does, as the panes' changes and the exits, and answers the requests alone.
