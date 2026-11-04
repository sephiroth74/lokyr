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
