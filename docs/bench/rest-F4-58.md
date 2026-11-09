# 0.8.0's latency tail, startup and motion again, and `allow-jit` (F4-58)

Measured on 2026-11-09, from 18:07 to 19:57, after [F4-25's A/B](ab-F4-25.md), whose latency's p99 was 7.4 ms longer than 0.7.0's in seven rounds of eight and whose startup's gap was 7 ms longer in its first two rounds; then from 22:01 to 22:45 on a MacBook Pro of no management, [below](#on-a-mac-of-no-management). The sessions' scripts, reports and tables are in [rest-F4-58](rest-F4-58/); the micro-benchmarks' program in [`spikes/jit-entitlement`](../../spikes/jit-entitlement/).

Each session ran of F4-25's config, Menlo at 13 points, 120 × 40 cells, an opaque window, on the Dell P2721Q at 60 Hz, of balanced orders, the same pause of 2 s before each run, each copy started before its measured rounds. Microsoft Defender's enterprise inventory scripts, its endpoint extension and its data loss prevention agent ran through all of them, 20 % to 140 % of a core, as they do on this managed Mac; no session could wait for them to end.

## In short

- **The code between 0.7.0 and 0.8.0 does not lengthen the latency's tail**: six builds between them, release binaries run outside a bundle, are alike.
- **The startup at rest is 0.7.0's**: F4-25's 7 ms were a new copy's first starts.
- **0.8.0's bundle has a longer tail in some sessions and not in others**, and so does `allow-jit`, its one entitlement that 0.7.0's lacks: three sessions of six found it longer, by 6.5 to 10.9 ms of the p99, in most rounds; three found nothing, the same copies among them. No mechanism of the system's shows a cost of the entitlement: a context switch, a wake under load, a page fault and a system call are alike of `allow-jit` and of the hardened runtime alone.
- **On a Mac of no management, `allow-jit` costs nothing**: an M1 Max without Defender or Jamf, at 120 Hz, of the same four copies in 16 rounds, finds `allow-jit`'s p99 longer in 15 rounds of 32, its mean 1.5 ms shorter, its median the same; the managed Mac's lengthening was of that Mac's activity, or of chance.
- **The window's motion**, which Defender's scans of Instruments' recordings kept from the managed Mac, is 0.7.0's on the M1, at 85 to 88 frames a second of 120 in both apps; its startup 2.4 ms longer of its larger binary, within its budget.
- **`allow-jit` stays**, as the user chose on 9 November 2026: it costs nothing measurable, and Luau's native code runs arithmetic three times faster than the interpreter; its slower activation of a large plugin is B-648's.

## The sessions of 0.8.0 against 0.7.0 and of `allow-jit`

| Session | When | Compared | p99, without the entitlement | p99, of `allow-jit` | Rounds where it is longer |
|---|---|---|---|---|---|
| [F4-25's A/B](ab-F4-25.md) | 15:55 | 0.7.0's DMG and 0.8.0's | 39.9 ms | 47.3 ms | 7 of 8 |
| F4-25's second | 17:00 | the same, Defender scanning 36 GB of recordings | 68.3 ms | 70.4 ms | within the spread |
| [At rest](rest-F4-58/analysis.txt) | 18:07 | the same | 44.9 ms | 55.8 ms | 13 of 16 |
| [`allow-jit`](rest-F4-58/jit/analysis.txt) | 18:57 | 0.7.0 and 0.8.0, each also signed again, 0.7.0's with `allow-jit` and 0.8.0's without | 43.4 ms | 49.9 ms | both ways: 0.7.0 longer of it, 0.8.0 shorter without it |
| [The phases](rest-F4-58/phases/analysis.txt) | 19:37 | a build of HEAD as signed, without `allow-jit`, and of `allow-unsigned-executable-memory` | 50.0 ms | 47.2 ms | none |
| [`allow-jit` again](rest-F4-58/jit-again/analysis.txt) | 19:47 | the copies of 18:57, 12 rounds | 47.4 ms | 46.6 ms | none |
| [The M1](rest-F4-58/m1/results/analysis.txt) | 22:05 | the copies of 18:57, 16 rounds, on a Mac of no management at 120 Hz | 46.4 ms | 44.9 ms | none, 15 of 32 |

The p50 is alike in every session, 19.2 to 20.6 ms but the second, loaded one's. Where the tail is longer, it is in most rounds; where it is not, it is not even of the same copies. What tells the sessions apart is not known: not the load average, which was alike, nor what the sparse records of the busiest processes show.

A sign test of the rounds, the two-sided chance of as many rounds longer one way under even odds, weighs each: 7 of 8 in F4-25's A/B, p = 0.07; 13 of 16 at rest, p = 0.02; 11 of 16 of `allow-jit`'s copies against none at 18:57, 0.7.0's and 0.8.0's together, p = 0.21; 12 of 24 at 19:47, p = 1. The sessions that compared `allow-jit` alone, of the same code, never reach the at-rest session's weight, which compared 0.7.0's code with 0.8.0's too.

## The builds between them

[bisect](rest-F4-58/bisect/analysis.txt): six release binaries of `lokyr-app`, of 0.7.0, the fixes of the review after F3-207 (3bd0c75e), the host's hardening to F4-06 (d33fb27c), the Luau host, its review's fixes and the timers to F4-13 (64b1c1ec), the installed plugins, the consent and the reloads with their review's fixes to F4-56 (2104a617), and 0.8.0, each run outside a bundle, of no hardened runtime, in 12 rounds, each build twice in each place.

| Build | p50 | p99 | In the fourth refresh or later |
|---|---|---|---|
| 0.7.0 | 19.76 ms | 48.30 ms | 17 of 2388 |
| The review's fixes | 19.72 ms | 42.53 ms | 5 of 2388 |
| To F4-06 | 19.43 ms | 45.19 ms | 13 of 2388 |
| To F4-13 | 19.40 ms | 44.23 ms | 8 of 2387 |
| To F4-56 | 19.53 ms | 48.54 ms | 14 of 2388 |
| 0.8.0 | 19.66 ms | 47.34 ms | 28 of 2388 |

The binary grows from 11.5 to 15.5 MB at F4-56, as the installed plugins reach Luau; its latency does not move.

## The startup at rest

Four rounds of the floor and both apps, each started many times before: the gap to the minimal app's first window 26.9 ms of 0.7.0's, 20.4 ms of 0.8.0's, their medians 237.5 and 231.0 ms. F4-25's 7 ms longer of 0.8.0 were of a new copy, as [F3-228](startup-F3-228.md) found.

## What the entitlement changes, of a program of its own

[`spikes/jit-entitlement`](../../spikes/jit-entitlement/) signs the same binary five ways: ad hoc of no hardened runtime, the hardened runtime alone, of `allow-jit`, of `allow-unsigned-executable-memory`, and of both.

| Signature | Code of `MAP_JIT`, as Luau's code generator maps it | Code of a plain mapping |
|---|---|---|
| No hardened runtime | runs | runs |
| The hardened runtime alone | refused, `EINVAL` | killed, `SIGKILL` |
| `allow-jit` | runs | killed |
| `allow-unsigned-executable-memory` | runs | runs |
| Both | runs | runs |

Luau maps its code of `MAP_JIT` and turns it executable with `mprotect`, never of `pthread_jit_write_protect_np`; `allow-unsigned-executable-memory` lets it do so too, a wider exception.

The same program's measures, six rounds of each signature in turned orders ([micro](rest-F4-58/micro/analysis.txt)), and again with a region of `MAP_JIT` mapped first ([micro-region](rest-F4-58/micro-region/analysis.txt)):

| Measure, p99 | No hardened runtime | The hardened runtime | `allow-jit` | `allow-unsigned-executable-memory` |
|---|---|---|---|---|
| A wake between two threads and back | 7.10 µs | 7.81 µs | 7.79 µs | 7.21 µs |
| A sleep of 1 ms, late by, at rest | 280 µs | 279 µs | 281 µs | 279 µs |
| The same beside a busy thread on each core | 3.89 ms | 3.78 ms | 3.78 ms | 3.99 ms |
| A page's first touch | 2.75 µs | 2.79 µs | 2.71 µs | 2.75 µs |
| `getppid` | 0.11 µs | 0.10 µs | 0.10 µs | 0.10 µs |

The hardened runtime without `allow-unsigned-executable-memory`, alone or of `allow-jit`, has a wake between two threads 0.7 µs longer at its p99, 2.2 µs on the M1 [below](#on-a-mac-of-no-management), in each round and with the region alike: microseconds, against a tail of milliseconds, and the same of `allow-jit` as of the hardened runtime alone, which 0.7.0 had. The rest is alike of all five within their spread. The tables until this session's last summed the four signatures of the hardened runtime into its column, a pattern of the analysis that took `hr-*`, fixed.

## The latency's phases

The bench tells each key's phases since this task: the press, as the system stamped it, to the app's handling of its key; that to the frame that shows its echo, ready to present; and that to the frame's presentation. Of the session of 19:37, alike of the three signatures:

| Signature | Latency p99 | Handled p50 and p99 | Framed p50 and p99 | Presented p50 and p99 |
|---|---|---|---|---|
| `allow-jit` | 47.2 ms | 1.7 and 16.9 ms | 1.5 and 17.0 ms | 7.6 and 33.6 ms |
| None | 50.0 ms | 2.1 and 16.9 ms | 2.3 and 17.1 ms | 8.3 and 36.1 ms |
| `allow-unsigned-executable-memory` | 50.8 ms | 2.1 and 16.9 ms | 2.3 and 17.0 ms | 8.4 and 36.4 ms |

The tail's keys wait a refresh in a phase, the most of them in the presentation's; which holds the wait for a drawable and the frame's encoding too, as the phase `framed` ends before the renderer draws, the review after F4-59 found ([B-651](../tasks/BACKLOG.md)), so that a wait of the main thread for a drawable counts there.

## Luau's interpreter against its native code

As the choice of the entitlement weighs it: a plugin's work of each kind, on the SDK's fake host, of a release build, the median of 30 calls, in both executions, of Luau 0.740.

| Work | Interpreted | Native |
|---|---|---|
| A view of 200 rows rendered | 0.43 ms | 0.42 to 0.68 ms |
| 2000 lines matched of `string`'s patterns | 0.49 ms | 0.65 ms |
| A loop of `string.byte` over 100 KB | 3.2 ms | 3.3 ms |
| A loop of `buffer.readu8` over 100 KB | 2.7 ms | 2.8 ms |
| Arithmetic over a table, a million operations | 6.9 ms | 2.3 ms |
| The template's activation | 0.17 ms | 0.27 ms |
| The activation of 2017 lines | 1.27 ms | 4.27 ms |

The interpreter is native code's speed but for arithmetic in Luau's own loops, three times slower, and activates faster, as it compiles nothing; F0-22's byte loops, two to five times slower interpreted of an older Luau, are alike in 0.740.

## The window's motion

Stopped after its first window, as F4-25's: Microsoft Defender scanned each recording of Instruments as it grew, about 80 % of a core, and 0.7.0's drag showed 29 frames a second and its resize 24, against 58 and 56 in [F3-77's session](ab-F3-77.md#the-windows-motion). A session of the motion needs Defender quiet, or recordings in a place that it does not scan; the M1's measured it [below](#the-windows-motion-on-the-m1).

## On a Mac of no management

A MacBook Pro of Apple's M1 Max, 64 GB, macOS 26.7.3, just installed again, of no management, Microsoft Defender, Jamf nor any system extension, lent for the evening: its built-in display alone, at 120 Hz through ProMotion, on AC power, its screen awake and unlocked, driven over SSH from the managed Mac, its apps opened by `open` in its desktop's session and the motion run in Terminal, which Accessibility allowed ([m1](rest-F4-58/m1/), its [setup](rest-F4-58/m1/SETUP.md) and [scripts](rest-F4-58/m1/all.sh)). The session began once Spotlight's indexing and the photos' analysis had ended, the busiest process under 1.5 % of a core over 10 s; XProtect looked at each new copy at its first start, the warm-ups', and the window server alone worked through the rounds. The same copies as the managed Mac's: 0.7.0's and 0.8.0's apps of their DMGs, notarized, and each signed again, 0.7.0's with `allow-jit` and 0.8.0's without, of the same config, Menlo at 13 points, 120 × 40 cells; the results and their tables in [m1/results](rest-F4-58/m1/results/analysis.txt).

### The latency

16 rounds of 200 keys, the four apps in orders that turn, the second eight reversed, each in each place four times:

| App | p50, mean | p99, mean | p99, median | In the fourth refresh or later |
|---|---|---|---|---|
| 0.7.0 | 21.52 ms | 45.85 ms (sd 3.35) | 45.69 ms | 739 of 3181 |
| 0.7.0 of `allow-jit` | 21.40 ms | 44.41 ms (sd 4.81) | 44.57 ms | 708 of 3178 |
| 0.8.0, of `allow-jit` | 21.63 ms | 45.34 ms (sd 2.66) | 45.72 ms | 733 of 3178 |
| 0.8.0 without it | 21.22 ms | 46.90 ms (sd 3.51) | 45.73 ms | 734 of 3182 |

`allow-jit`'s copy has the longer p99 in 8 rounds of 16 of 0.7.0 and 7 of 16 of 0.8.0, 15 of 32, p = 0.86; its p99 1.5 ms shorter on the mean, the same on the median. At 120 Hz a refresh is 8.33 ms, so that the fourth refresh begins at 25 ms, which a quarter of every app's keys reach.

### The entitlement's micro-benchmarks

| Measure, p99 | No hardened runtime | The hardened runtime | `allow-jit` | `allow-unsigned-executable-memory` | Both |
|---|---|---|---|---|---|
| A wake between two threads and back | 4.79 µs | 6.54 µs | 6.65 µs | 4.35 µs | 4.38 µs |
| A sleep of 1 ms, late by, at rest | 306 µs | 321 µs | 310 µs | 320 µs | 314 µs |
| The same beside a busy thread on each core | 9.17 ms | 9.19 ms | 9.18 ms | 9.17 ms | 9.23 ms |
| A page's first touch | 3.65 µs | 3.81 µs | 3.90 µs | 3.98 µs | 3.75 µs |
| `getppid` | 0.11 µs | 0.11 µs | 0.11 µs | 0.11 µs | 0.11 µs |

The managed Mac's pattern, larger: the wake 2.2 µs longer of the hardened runtime without `allow-unsigned-executable-memory`, 6.3 to 6.8 µs in every round against 4.0 to 5.5, `allow-jit` the hardened runtime's.

### The startup

Four rounds, both apps started before, the minimal app's first window each round:

| Round | The minimal app | 0.7.0 | 0.8.0 | 0.7.0's gap | 0.8.0's gap |
|---|---|---|---|---|---|
| 1 | 229.3 ms | 283.4 ms | 285.6 ms | 54.1 ms | 56.2 ms |
| 2 | 228.0 ms | 283.9 ms | 287.2 ms | 55.9 ms | 59.2 ms |
| 3 | 226.2 ms | 282.1 ms | 284.8 ms | 56.0 ms | 58.6 ms |
| 4 | 226.4 ms | 286.4 ms | 288.1 ms | 59.9 ms | 61.7 ms |
| Mean | | 284.0 ms | 286.4 ms | 56.5 ms | 58.9 ms |

0.8.0 starts 2.4 ms later in each round, 4 % of the gap, as its binary grows from 11.5 to 15.5 MB; the managed Mac's rounds, of a spread of 4 to 43 ms, could not tell so little. The M1's gap is twice the managed Mac's, of a slower machine; the budget, 35 ms, is of the M5 Pro's sessions.

### The window's motion on the M1

As [F3-152](motion-F3-152.md) measures it, of its config, two rounds, the second reversed, each recording's figures taken and its trace removed before the next ([motion.sh](rest-F4-58/m1/motion.sh)): frames a second shown of 120, the mean of the two rounds.

| Motion | 0.7.0 | 0.8.0 |
|---|---|---|
| A drag, opaque | 83.7, its second round alone | 85.5 |
| A drag, translucent with a blur | 82.6 | 84.6 |
| A live resize, opaque | 87.0 | 87.6 |
| A live resize, translucent with a blur | 85.3 | 86.6 |

0.7.0's first drag ran as ProMotion held the display at 60 Hz, 59.2 frames a second of 60, left out. A resize misses 306 to 333 refreshes of about 1150 in both, the compositor's delay 13.2 to 13.5 ms at its p50 and 14.3 to 14.6 at its p95; the window server 43 to 47 % of a core and the app 31 to 36 %. 0.8.0 moves its window as 0.7.0 does.
