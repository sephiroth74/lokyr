# The start's gap at rest (F3-228)

Measured on 2026-11-08 between 17:26 and 17:33, the user away, as [0.7.0's A/B](ab-F3-77.md) found its startup 30 ms longer than 0.5.1's, its gap to the minimal app 46.9 ms past the budget of 35. The script, [`run.sh`](startup-F3-228/run.sh), its analysis, [`analyze.py`](startup-F3-228/analyze.py), and the reports are in [startup-F3-228](startup-F3-228/).

## Method

The apps of 0.5.1's and 0.7.0's DMGs, notarized and stapled, and two builds of the same commit, 576b221, and the same signature, not notarized: one as 0.7.0, one whose look at the default terminal waits a second, so that its thread, which starts with the app since F3-227, is out of the start's way. First a round of each, unmeasured, so that every copy has started before: Microsoft Defender's real-time protection, on this Mac, scans a new executable at its first starts, and took about half a core as the session began, scanning the two builds just made. Then four rounds of rotated orders, warm, 10 runs after 2, to the first frame on the display, each of the minimal AppKit app of F0 as its floor, of 0.7.0's A/B's config, the same pause of 2 s before each run. The load average 4 to 7.

## Results

| App | Startup, the mean of the four rounds' medians | Its gap to the minimal app, their mean | Each round's gap |
|---|---|---|---|
| 0.5.1 | 246.7 ms | 15.1 ms | 19.7, 12.7, 19.8 and 8.3 ms |
| 0.7.0 | 250.3 ms | 18.8 ms | 30.4, 10.8, 21.2 and 12.7 ms |
| The build | 256.7 ms | 25.1 ms | 20.6, 35.2, 32.1 and 12.7 ms |
| The build, its look a second later | 255.3 ms | 23.7 ms | 20.5, 34.5, 36.5 and 3.4 ms |

- **0.7.0's gap is within its budget at rest**, 18.8 ms of 35, 3.7 ms over 0.5.1's, within the rounds' spread, of 8 to 30 ms each. The A/B's 46.9 ms, and its second session's 35.2, were of a copy freshly out of its DMG, which the antivirus scanned at its starts, as 0.5.1's copy, started for days, no longer was; the first draft's copy had told the same, 20 ms longer in its A/B and 0.8 ms once it had started about a hundred times.
- **The default terminal's look takes nothing of the start**: the build whose look waits a second starts as the one whose look goes at once, 1.4 ms apart, within the spread.
- The two builds start about 6 ms later than 0.7.0's app, of their signatures without notarization, which the system checks at each start.
- From here, an A/B starts each new copy before its measured rounds, as this session did.
