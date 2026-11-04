# The latency's tail of 0.4.0: a first session (F3-135)

Measured on 2026-11-04, between 14:50 and 14:58: whether the windows' accessibility trees, new in 0.4.0, lengthen the latency's tail that [F3-29](ab-F3-29.md) found, 0.4.0's p99 higher than 0.3.0's in 7 rounds of 8. The reports are in [latency-F3-135](latency-F3-135/), with the session's script, `run.sh`, its analysis, `analyze.py`, and the patches of the two variants, `b.patch` and `c.patch`, over 0.4.0's commit, ba5508c. The task is deferred, as the user chose the same day: a second session, in the regime that F3-29 measured, is left to do.

## Method

- **Three bundles**, each built of ba5508c in a worktree with `LOKYR_RELEASE=0.4.0` and signed with the Developer ID, not notarized, as [F3-105](startup-F3-105.md) found that the notarization does not move the measures: `a`, 0.4.0 as it is; `b`, the same but for the trees, which it gives a client that asks, once, and builds again as the screens change no more, `b.patch`; and `o`, the app of 0.3.0's DMG.
- **A probe** first, `c.patch`: 0.4.0 that writes to the standard error each tree it gives and its time, built and run once, 200 keys of `lokyr --bench latency`.
- **The session**: F3-29's config, 200 keys a run; a short run of each bundle first, as the system checks a new bundle at its first launch; then six rounds, whose orders put each bundle first, second and third twice, as F3-29 found the second of a pair the slower.

## The probe

An accessibility client asked for the window's tree as it opened, as the apps of the Accessibility permission on this Mac do when a window becomes key; then the app gave the tree 45 times in the 10 s of the run, once every 200 ms at most as the screen changed at each key. Each tree took 0.38 ms to build, of the pane's text, at the median, and 0.50 ms with the adapter's update, 0.66 ms at most, on the main thread. What the client asks after each tree's notifications is answered on the main thread too, outside that time.

## Results

| Bundle | p50, mean of 6 | p99, mean of 6 | Its range | Samples in the third refresh or later | In the fourth or later |
|---|---|---|---|---|---|
| `a`, 0.4.0 | 26.3 ms | 70.9 ms | 66.1 to 76.3 ms | 271 of 1192 | 87 |
| `b`, 0.4.0 without the trees' rebuilds | 26.6 ms | 71.3 ms | 66.9 to 76.9 ms | 289 of 1192 | 98 |
| `o`, 0.3.0 | 26.3 ms | 71.4 ms | 66.1 to 76.3 ms | 269 of 1191 | 73 |

- **The trees are not the cause.** Without their rebuilds, `b`'s tail is not shorter than `a`'s: its p99 is `a`'s within 0.4 ms, and it has more samples in the fourth refresh or later, not fewer.
- **The session ran in a slower regime than F3-29's.** Every bundle took about a refresh more: a p50 of 26 ms against 19, a p99 of 71 ms against 41 to 50, as [F3-13](ab-F3-13.md)'s rounds 3 to 5 did. The load average was 1.5 to 2.5, so the processors were not the cause; the system's composition, which takes a refresh more now and then ([F2-54](latency-F2-54.md)), when something else on the display draws, is the likelier one, which this session did not check. The probe's run, right after its build, was in the same regime.
- **In that regime, 0.4.0's p99 is 0.3.0's**, 70.9 against 71.4 ms, and its median too. A difference of a few samples of 199, as F3-29's, can hide under a refresh more for every key, so this session neither confirms F3-29's difference nor rules it out.

## Left to do

The same session in F3-29's regime, a p50 near 19 ms: started once a short run of 0.3.0 gives a p50 under 21 ms, with nothing else drawing on the display; and, if 0.4.0's tail is longer there, bundles of 0.4.0 that differ by its other parts on the way of a key to its frame, beside the trees, as the windows' surfaces and layers.

# The second session, in the faster regime

Measured on 2026-11-04, between 21:13 and 21:17, of the script and the analysis in [latency-F3-135/second](latency-F3-135/second/), `run.sh` and `analyze.py`, with their reports: whether 0.4.0's longer tail shows in the regime of [F3-29](ab-F3-29.md)'s rounds, a p50 near 19 ms, where the first session ran a refresh slower.

## Method

- **Three bundles**: `o`, the app of 0.3.0's DMG; `a`, 0.4.0, the first session's; `h`, the head of the work since 0.4.0, 0acf7fd, built as a release of the plain features, `0.4.0-20261104`, the slots and their input, the live resize's pace, and the review's fixes after F3-138 in it.
- **The regime first**: a short run of `o`, 50 keys, until its p50 is under 21 ms, ten tries at most; it was at the first, 19.1 ms.
- **Six rounds** of F3-29's config, 200 keys a run, whose orders put each bundle first, second and third twice.

## Results

| Bundle | p50, mean of 6 | p99, mean of 6 | Its range | Samples in the third refresh or later | In the fourth or later |
|---|---|---|---|---|---|
| `o`, 0.3.0 | 19.7 ms | 41.5 ms | 37.1 to 43.0 ms | 36 of 1194 | 2 |
| `a`, 0.4.0 | 20.0 ms | 42.6 ms | 38.2 to 52.8 ms | 48 of 1194 | 4 |
| `h`, the head | 19.8 ms | 46.0 ms | 42.0 to 50.9 ms | 45 of 1194 | 6 |

The p99's difference from 0.3.0's, round by round: `a` −3.7, +4.8, −1.7, −1.6, +10.2, −1.2 ms; `h` +0.1, +13.8, +5.3, +0.2, +5.6, +2.5 ms. By the place in the round, of every bundle, the p99's mean is 44.9, 43.3 and 41.9 ms, first to third, the p50's 19.8 ms each.

- **The regime was F3-29's**: the medians of the three are 19.7 to 20.0 ms, as F3-29's 18.9 to 20.0.
- **0.4.0's tail is 0.3.0's within the rounds' spread**, its orders balanced: its p99 higher in two rounds of six, lower in four, its mean 1.1 ms higher, of twelve samples more in the third refresh of 1194. F3-29's difference, 0.4.0's p99 higher in seven rounds of eight, came of its orders, 0.4.0 second in five of them, as the second of a pair was the slower there.
- **The head's tail is longer**: its p99 higher than 0.3.0's in every round, though by 0.2 ms or less in two, its mean 4.6 ms higher, of nine samples more in the third refresh or later and four more in the fourth; its medians 0.3.0's. Within the budget still, it is the work since 0.4.0, on a key's way to its frame, that lengthens it, which bundles that differ by one part of it tell next.

# The third session, of the parts of the work since 0.4.0

Measured on 2026-11-04, between 21:51 and 21:59, of the scripts and the analysis in [latency-F3-135/third](latency-F3-135/third/), `build.sh`, `run.sh` and `analyze.py`, with their reports: which part of the work since 0.4.0 lengthens the head's tail that the second session found.

## Method

- **A probe first**, of the head, `second/probe.patch`, 200 keys: each turn of the main thread, of its kind and its time, the slots' layout in each frame, and the timers by their names. A key's turn took 17 µs at the median, a frame's 384 µs, as two frames a key; the slots' layout 5 µs a frame; the plugins' turn of their events never started; the accessibility trees 47, of 0.6 ms each, as 0.4.0's; the titles' look 48, of 29 µs. Nothing on the main thread costs more than before.
- **Six bundles**: `o`, 0.3.0; `a`, 0.4.0; `m2`, 8661a20, the slots in the windows; `m3`, 7ae3d41, the live resize's pace and the view's contents placement at its top left corner; `h`, the head, the second session's; `t`, the head but for that placement, as a frame's drawn at another size than the view's would have stretched; each a release of its commit, built in a worktree.
- **The faster regime first**, at the first try, 20.4 ms; then six rounds of a Williams design, each bundle once in each place of a round and after each other once.

## Results

| Bundle | p50, mean of 6 | p99, mean of 6 | Its range | Samples in the third refresh or later | In the fourth or later |
|---|---|---|---|---|---|
| `o`, 0.3.0 | 20.3 ms | 45.0 ms | 39.4 to 52.2 ms | 45 of 1194 | 4 |
| `a`, 0.4.0 | 20.2 ms | 48.4 ms | 41.4 to 57.1 ms | 54 of 1194 | 10 |
| `m2`, the slots | 20.1 ms | 45.8 ms | 41.8 to 53.1 ms | 45 of 1194 | 6 |
| `m3`, the resize's pace and the placement | 20.3 ms | 46.1 ms | 40.2 to 53.7 ms | 61 of 1194 | 6 |
| `h`, the head | 20.5 ms | 45.1 ms | 37.1 to 58.4 ms | 60 of 1194 | 8 |
| `t`, the head but for the placement | 20.1 ms | 44.1 ms | 35.4 to 55.8 ms | 53 of 1193 | 6 |

By the place in the round, of every bundle, the p99's mean is 41.9, 44.2, 47.3, 45.8, 44.6 and 50.7 ms, first to sixth; the p50's 19.9 to 20.5 ms. Round by round, the p99's differences from 0.3.0's run from −12.9 to +14.3 ms.

- **The head's tail is 0.3.0's** here, 45.1 against 45.0 ms, and so is every part's, within the rounds' spread: the slots, the resize's pace, the placement, which `t` without it leaves as it is.
- **The place in a round moves the p99 more than any bundle**, by 9 ms from the first to the last, as F3-29 found the second of a pair the slower: the measure's own noise, whatever its cause on the system, larger than the differences sought. A design whose bundle runs later more often finds it slower: F3-29's 0.4.0, second in five rounds of eight; the second session's head, of a design of three in which it differed from 0.3.0 by 0.2 ms or less in two rounds of six.
- **Over the second and third sessions**, twelve rounds, the share of samples in the third refresh or later is 3.4 % of 0.3.0's, 4.3 % of 0.4.0's and 4.4 % of the head's, a difference under two of its standard deviations, and no part of the work since 0.4.0 carries it.

## The outcome

No part of 0.4.0, nor of the work since, lengthens the latency's tail by a measure larger than the method's noise; the differences that F3-29 and the second session found came of the bundles' places in their rounds. The A/B measures from now, as F3-40's budgets, take balanced designs, each bundle in each place as often, and tell the places' means beside the bundles'.
