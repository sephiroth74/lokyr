# 1.4's startup and output at rest

Measured on 22 November 2026, from 06:26 to 06:45, run from a terminal with no other app open. [1.4.0's A/B](ab-F5a-78.md) had found two problems:

- the startup's gap to the minimal app and `cat`'s throughput past their budgets, for both versions;
- 1.4.0's startup 7.5 ms longer than 1.3.0's in one of its two steps.

The session's script, [`run.sh`](rest-F5a-123/run.sh), its analysis, [`analyze.py`](rest-F5a-123/analyze.py), the reports and [the tables](rest-F5a-123/analysis.txt) are in [rest-F5a-123](rest-F5a-123/).

## Method

Three copies:

- 1.3.0's app of its release's DMG, notarized, its ticket stapled.
- 1.3.0's tag, 8251e435, built into a bundle and signed as 1.4.0's, not notarized.
- 1.4.0's code, of commit f25e3ffb, of the same bundle and signature, not notarized.

The two bundles were built the same morning, by the same toolchain and signing identity. The tag's bundle tells the signature's cost apart from the cost of 1.4's code. Gatekeeper reads 1.3.0's app as a notarized Developer ID app and both bundles as unnotarized ones.

Each copy was started 40 times first. Then came twelve rounds of the three copies' startups:

- warm, 10 runs after 2, to the first frame on the display;
- in the orders of a Williams square, each copy as often before and after each other;
- each round of the minimal AppKit app as its floor.

Last came the four workloads on screen, of 1.3.0's app and 1.4.0's: `cat`'s in eight rounds, the others' in four, in the orders AB BA BA AB.

Every run had the same pause of 2 s before it, of [the releases' config](ab-F3b-20/config.toml), smooth scrolling and Blocks off. The display was a Dell P2721Q at 60 Hz and a scale of 2. The load average ran from 2.5 to 6.8. Microsoft Defender's daemon took a tenth of a core as the session started, and Spotlight's indexer most of a core as it ended.

## Results

| Copy | Startup, the mean of twelve rounds' medians | Its gap to the minimal app, the mean of the rounds |
|---|---|---|
| 1.3.0, notarized | 254.4 ms | 30.7 ms |
| 1.3.0's tag, not notarized | 267.0 ms | 43.2 ms |
| 1.4.0, not notarized | 267.2 ms | 43.5 ms |
| The minimal app | 223.7 ms, from 217.1 to 230.4 | |

| Copies | Their rounds' differences, the mean | The median | Rounds where the second is longer |
|---|---|---|---|
| 1.3.0's tag, then 1.4.0 | +0.2 ms, of a deviation of 11.6 | −0.3 ms | 5 of 12 |
| 1.3.0 notarized, then 1.3.0's tag | +12.5 ms, of a deviation of 7.4 | +14.5 ms | 11 of 12 |
| 1.3.0 notarized, then 1.4.0 | +12.7 ms, of a deviation of 7.8 | +13.2 ms | 11 of 12 |

| Workload on screen, the median of the runs | 1.3.0 | 1.4.0 | In 1.4.0's A/B, 1.3.0 and 1.4.0 | In 1.3.0's A/B, 1.3.0 |
|---|---|---|---|---|
| `cat` of 1 GiB of ASCII | 303.7 MiB/s | 304.4 MiB/s | 265.6 and 269.5 MiB/s | 308.6 MiB/s |
| `seq 1 10000000` | 218.4 MiB/s | 216.8 MiB/s | 159.4 and 185.9 MiB/s | 218.3 MiB/s |
| Colored text, SGR | 271.8 MiB/s | 281.1 MiB/s | 195.1 and 191.9 MiB/s | 285.2 MiB/s |
| CJK and emoji | 217.2 MiB/s | 216.1 MiB/s | 203.3 and 204.6 MiB/s | 213.2 MiB/s |

- **1.4.0's code starts as fast as 1.3.0's.** Built and signed the same way, the two are 0.2 ms apart, and 1.4.0 is longer in 5 rounds of 12. That is within the tolerance of 5 ms: nothing that 1.4 added at the start costs time.
- **A copy that is not notarized started 12.5 ms later in this session, of the same code.** 1.3.0's tag, built as a bundle, was longer than its release's notarized app in 11 rounds of 12. As far as their builds tell, the copies differ in the notarization alone: the same commit, toolchain, signing identity and hardened runtime. 1.4.0's A/B compared 1.3.0's notarized app with 1.4.0's bundle, which is not notarized, so its 7.5 ms were of the signature. The cost varies from session to session:
  - nothing in [1.3.0's A/B](ab-F5a-17.md), whose bundle started 1.4 ms quicker than 1.2.0's notarized app;
  - about 6 ms in [0.7.0's startup at rest](startup-F3-228.md);
  - 0.9 and 7.5 ms in 1.4.0's A/B's two steps;
  - 12.5 ms here.
- **The notarized app holds the startup's gap at rest**, at 30.7 ms of 35. The minimal app started in 217 to 230 ms, as before 1.4.0's A/B: 207 to 218 ms in 1.3.0's. That A/B's 170 to 195 ms had made both versions' gaps 53 to 61 ms. The bundles' gaps of 43 ms are past the budget because they are not notarized, 1.3.0's code as much as 1.4.0's.
- **`cat` holds at rest**, at 303.7 and 304.4 MiB/s, above the budget of 275, as in 1.3.0's own A/B. `seq`, SGR and CJK are back at that session's rates too. 1.4.0's output matches 1.3.0's within the tolerance of 5 %, SGR 3.4 % faster. The A/B's slower output was the session's, the same for both versions.
- **From here, an A/B compares the startup of copies signed the same way.** Either it builds the earlier release's tag into a bundle beside its notarized app, as this session did, or it notarizes both.
