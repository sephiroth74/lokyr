# 1.4.0's startup of its release's DMG

Measured on 22 November 2026, from 08:51 to 09:02, run from a terminal with no other app open. [1.4's startup at rest](rest-F5a-123.md) found a copy that is not notarized slower to start than the same code notarized, so the releases' own disk images are compared here, both notarized and stapled. The session's script, [`run.sh`](startup-F5a-79/run.sh), its analysis, [`analyze.py`](startup-F5a-79/analyze.py), the reports and [the tables](startup-F5a-79/analysis.txt) are in [startup-F5a-79](startup-F5a-79/).

## Method

The copies were 1.3.0's app of its release's DMG and 1.4.0's app of its release's DMG, build 202611220721, both notarized, their tickets stapled. Each copy was started 40 times first. Then came twelve rounds of both startups, in the orders AB BA BA AB:

- warm, 10 runs after 2, to the first frame on the display;
- each round of the minimal AppKit app as its floor;
- the same pause of 2 s before each run.

The config was [the releases' config](ab-F3b-20/config.toml), with smooth scrolling and Blocks off. The display was a Dell P2721Q at 60 Hz and a scale of 2, and the load average was 1.8 to 2.8.

## Results

| Copy | Startup, the mean of twelve rounds' medians | Its gap to the minimal app, the mean of the rounds |
|---|---|---|
| 1.3.0 | 250.1 ms | 21.7 ms |
| 1.4.0 | 249.2 ms | 20.8 ms |
| The minimal app | 228.4 ms, from 221.5 to 243.2 | |

- **1.4.0 starts as fast as 1.3.0**: 0.8 ms quicker on average, of a deviation of 1.3 ms, and longer in 2 rounds of 12.
- **Both hold the startup's gap**, at 20.8 and 21.7 ms of 35.
- 1.4.0's A/B had found 1.4.0's bundle 7.5 ms slower than 1.3.0's app. The bundle was not notarized and the app was, and that was the whole difference.
