# 1.5.0's startup of its release's DMG

Measured on 25 November 2026, from 00:28 to 00:39, from a terminal with VS Code closed. As for 1.4.0, [the releases' own disk images](startup-F5a-79.md) are compared here, both notarized and stapled, because a copy that is not notarized starts later than the same code notarized. The session's script, [`run.sh`](startup-F5b-21/run.sh), its analysis, [`analyze.py`](startup-F5b-21/analyze.py), the reports and [the tables](startup-F5b-21/analysis.txt) are in [startup-F5b-21](startup-F5b-21/).

## Method

The copies were 1.4.0's app of its release's DMG, build 202611220721, and 1.5.0's app of its release's DMG, build 202611242301, both notarized, their tickets stapled. Each copy was started 40 times first. Then came twelve rounds of both startups, in the orders AB BA BA AB:

- warm, 10 runs after 2, to the first frame on the display;
- each round of the minimal AppKit app as its floor;
- the same pause of 2 s before each run.

The config was [the releases' config](ab-F3b-20/config.toml), with smooth scrolling and Blocks off. The Mac was an Apple M5 Pro on AC power, up for 2 days and 8 hours. The display was a Dell P2721Q at 60 Hz and a scale of 2, and the load average was 1.8 to 5.5.

## Results

| Copy | Startup, the mean of twelve rounds' medians | Its gap to the minimal app, the mean of the rounds |
|---|---|---|
| 1.4.0 | 267.8 ms | 54.6 ms |
| 1.5.0 | 268.2 ms | 55.0 ms |
| The minimal app | 213.2 ms, from 208.9 to 215.6 | |

- **1.5.0 starts as fast as 1.4.0**: 0.4 ms later on average, with a deviation of 6.4 ms, and later in 7 rounds of 12. Each round's change ran from −13.9 to +10.5 ms.
- **The startup's gap was over its budget for both copies**, at 54.6 and 55.0 ms of 35. The release didn't cause this, because 1.4.0's own DMG was over it too. On 22 November, [1.4.0's DMG](startup-F5a-79.md) started in 249.2 ms with a gap of 20.8 ms. In this session the same DMG started 18.6 ms later, and the minimal app came 15 ms sooner, in 213.2 ms against 228.4.
- The gap has grown for both versions over the last days: 20.8 ms on 22 November, 34.6 and 34.9 ms at rest on 24 November of bundles that weren't notarized, [ab-F5b-20](ab-F5b-20.md#the-measures-of-lokyr-bench), and 55 ms in this session. The terminal that ran the session took 9 % of a core, and the antivirus's processes some 15 %.
