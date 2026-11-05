# The latency's tail and the startup of 0.5.0, bisected (F3-163)

Measured on 2026-11-05, between 14:47 and 15:45, in four sessions, of the scripts and the analysis in [latency-F3-163](latency-F3-163/), `build.sh`, `run.sh`, `startup.sh`, `startup2.sh`, `startup3.sh` and `analyze.py`, with their reports: what lengthens the latency's tail that [F3-40](ab-F3-40.md) found 0.5.0's 3.6 ms longer than 0.4.0's over 16 rounds, and its startup's first frame, a refresh later in two rounds of four.

## Method

- **The machine** of [F3-40](ab-F3-40.md#machine): the Dell P2721Q at 60 Hz the main display and the only one, F3-29's config.
- **The bundles**: `o`, the app of 0.4.0's DMG; `k`, 0acf7fd, the head that [F3-135](latency-F3-135.md)'s third session found of 0.3.0's tail, the slots in the windows, the live resize's pace and the review's fixes after F3-138 in it; `m2`, 8661a20, and `m3`, 7ae3d41, F3-135's midpoints within it, the slots in the windows and then the live resize's pace and a new size's wait for its own drawing; `m`, 282fdc7, `k` with the plugins' panels and their accessibility, the plugins' threads, their config and state, the distributions, the fake host and the conformance suite, built of `build.sh`; `h`, the app of 0.5.0's DMG, `m` with the window's motion's work and the fixes of the review after F3-153; `h0`, 0.5.0's head as a local release; and `l`, `h0` finding the interface's face as its first text needs it.
- **The latency**, `run.sh`: eight rounds of 200 keys, a Williams design of `o`, `k`, `m` and `h` twice, each bundle once in each place of four rounds and after each other once, after a warm-up of each.
- **The startup**: of `o`, `k`, `m` and `h`, `startup.sh`, and of `o`, `m2`, `m3` and `k`, `startup2.sh`, four rounds each of a Williams design; of `o`, `h0` and `l`, `startup3.sh`, six rounds of a Williams design of three; 10 runs after 2 a bundle a round, each round of the minimal app's floor.
- **The load average** was 0.9 to 5.2 in the latency's session, falling as it went, 2.0 to 9.3 in the startup's, of the user's other work.

## The latency

| Bundle | p50, mean of 8 | p99, mean of 8 | Its standard deviation | Its range | Samples in the third refresh or later, of 1592 | In the fourth or later |
|---|---|---|---|---|---|---|
| `o`, 0.4.0 | 20.1 ms | 45.6 ms | 5.7 ms | 35.9 to 55.1 ms | 59 | 7 |
| `k`, 0acf7fd | 20.4 ms | 42.6 ms | 6.4 ms | 37.0 to 56.4 ms | 54 | 6 |
| `m`, 282fdc7 | 20.1 ms | 44.7 ms | 2.4 ms | 40.2 to 48.5 ms | 61 | 6 |
| `h`, 0.5.0 | 20.1 ms | 44.5 ms | 3.6 ms | 39.3 to 49.9 ms | 65 | 6 |

- **0.5.0's tail is 0.4.0's here**, 44.5 against 45.6 ms, and no part of the work since 0.4.0 lengthens it: every bundle's p99 within the rounds' spread of the others', its samples past the third refresh alike. The place in a round moved the p99's mean by 1.7 ms, 44.9, 45.0, 43.3 and 44.1 ms from the first place to the fourth.
- **Over F3-40's two sessions and this one**, 24 rounds, 0.5.0's p99 is 2.1 ms longer than 0.4.0's, about 1.6 times the difference's standard error, of 31 samples of 4776 in the fourth refresh or later against 22: a difference that this session does not find, within the rounds' own spread of 3 to 6 ms.

## The startup

| Session | `o`, 0.4.0 | Its midpoints | The heads | The floor's range |
|---|---|---|---|---|
| `startup.sh`, the gap to the floor, mean of 4 | 2.1 ms | `k` 16.4 ms, `m` 14.6 ms | `h` 10.2 ms | 224 to 232 ms |
| `startup2.sh` | 2.8 ms | `m2` 7.2 ms, `m3` 7.1 ms, `k` 7.8 ms | | 221 to 232 ms |
| Its runs on the third step of a refresh, about 248 ms, of 40 | 4 | `m2` 14, `m3` 14, `k` 14 | | |
| `startup3.sh`, mean of 6 | 18.0 ms | | `h0` 25.6 ms, `l` 28.2 ms | 192 to 240 ms |

- **The startup is within its budget** in every session, a gap of 35 ms at most to the minimal app's first window: 0.5.0's 10 to 28 ms.
- **The refresh more comes with the slots in the windows**: of the first bisection, `k`, and the bundles after it, a refresh's 14 ms later than `o`; of the second, every bundle from `m2` on, the slots in the windows, of three times as many runs on the third step as `o`, the live resize's work after it adding none.
- **Not the interface's face**: the slots' work finds the interface's face as the app starts, a lookup of CoreText's on the start's way, which no view of the default distribution draws. `l`, which finds it as its first text needs it, starts as `h0` does, 245.2 against 242.6 ms, of a session too loaded to tell a few ms, its floor moving by 48 ms: the face's lookup is not the refresh, or less than this session tells. The change stays, as the face's own doc says, a window without the interface's text not paying for it.
- **Left as it is**, within the budget, as [B-445](../tasks/BACKLOG.md#low) of the backlog tells, a probe of the start's steps of the slots' work, of the windows' chrome and of the plugins' activation, the next measure.
