# Where 1.5.0's startup went

Measured on 25 November 2026, from 09:33 to 09:49, from a terminal with VS Code closed, 51 minutes after a restart. On 25 November, [1.5.0's DMG and 1.4.0's](startup-F5b-21.md) both started some 18 ms later than 1.4.0's DMG had on 22 November, and their gap to the minimal app was past its budget, at 55 ms of 35. This session looks for where that time went: in the launch's phases, in Launch Services' registrations of Lokyr's other copies, or in the measure itself. The session's script, [`run.sh`](startup-F5b-45/run.sh), its analysis, [`analyze.py`](startup-F5b-45/analyze.py), the reports and [the tables](startup-F5b-45/analysis.txt) are in [startup-F5b-45](startup-F5b-45/).

## Method

The copies were 1.4.0's and 1.5.0's apps of their releases' DMGs, both notarized and stapled, copied again after the restart and started 40 times each. The config was [the releases' config](ab-F3b-20/config.toml), with smooth scrolling and Blocks off. The session had three states:

1. **Few**: Lokyr's registrations in Launch Services as they were, five copies. Two of Instruments' App Launch traces of each copy, then five rounds of both startups.
2. **Many**: 20 more bundles of Lokyr's identifier and claims registered, 25 in all. Two traces of each copy again, then five rounds.
3. **Again**: those bundles unregistered and deleted, then four rounds.

Each round was ten warm runs after two, to the first frame on the display, with the minimal AppKit app as its floor, in the orders AB BA BA AB BA, and the same pause of 2 s before each run. The Mac was an Apple M5 Pro on AC power, the display a Dell U3417W at 60 Hz, and the load average was 2.3 to 4.5.

## Results

| State | 1.4.0 | 1.5.0 | The minimal app | Gaps, 1.4.0 and 1.5.0 |
|---|---|---|---|---|
| Few | 226.4 ms | 225.6 ms | 197.3 ms | 29.1 and 28.3 ms |
| Many | 223.2 ms | 221.3 ms | 169.8 ms | 53.4 and 51.5 ms |
| Again | 207.5 ms | 205.3 ms | 154.6 ms | 52.9 and 50.7 ms |

Each figure is the mean of the rounds' medians.

- **1.5.0 starts as fast as 1.4.0 in every state**: 0.8, 1.9 and 2.2 ms sooner. Its code is not where the time went.
- **The launch's phases are alike for both copies and in both states that were traced.** From the executable's launch:
  - AppKit's `applicationWillFinishLaunching` comes at 60 to 63 ms;
  - its scene's creation takes 32 to 39 ms;
  - `applicationDidFinishLaunching` takes 102 to 107 ms;
  - the app is in the foreground at 200 to 206 ms.
  The 20 more registrations moved none of these beyond the traces' spread.
- **The gap moved by 23 ms within one session, of no change to the copies.** In the third round after the registrations, the minimal app started 40 ms sooner, from 195 to 150 ms, and stayed there after they were taken away. Lokyr started one refresh sooner at the same time, from 220 to 203 ms. The registrations did not cause it, as the gap stayed at 51 to 53 ms once they were gone.
- **Just after the restart, the gap was within its budget**: 28.3 and 29.1 ms of 35, as on 22 November, which also came after a restart.

### Why the gap moves

The two figures behind the gap are not taken alike:

- **Lokyr's figure** is its own: its executable started directly, and the time from the process's start to its first frame shown on the display. A frame shows at a refresh, so the figure moves in whole refreshes: 250 of the session's 280 runs fall within 2 ms of a 60 Hz refresh, at 186, 203, 220, 236, 253 and 270 ms.
- **The minimal app's figure** comes from outside: the app launched through Launch Services by `open`, and the time from its process's start until the window server lists its window on screen, which a poll finds. It follows no refresh: 28 of its 140 runs fall near one, as chance would have it.

So a few milliseconds more of Lokyr's start can cost a whole refresh, 16.7 ms, and the minimal app's own launch path moves with the Mac's state on its own. Over the last days the gap read 20.8, 34.6, 55 and now 28 to 53 ms, of code that did not change.

## What follows

- **The startup's code is not a regression** of 1.5.0. Its gap past the budget on 25 November came of the Mac's state, as this session's swing shows.
- **The gap needs a floor measured as Lokyr is**: a minimal app started directly, that tells its own first frame on the display, so that both figures fall on refreshes and both move with the Mac. Until then, a release compares its startup with the last release's within one session, as every report since 1.4.0 has, and reads the gap as two refreshes, not 35 ms.
- **Where a faster start would come from**: `applicationDidFinishLaunching`, about 105 ms of the 200 to the foreground, holds the config's read, the plugins, the first window and its surface. The traces' time profiles are kept for that look, outside the repository.
