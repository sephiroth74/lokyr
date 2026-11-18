# The releases' benchmarks side by side

The last eight releases' measures, at a glance and in one table, each column of the A/B that measured it against the release before, at the conditions of [compare-F2-134](compare-F2-134.md): Menlo at 13 points, 120 × 40 cells, an opaque window, the main display at 60 Hz. Each release's notes tell the features that it brought and what they cost, as its session found. Each release's session adds its column and its notes here, the window's motion among its measures, and its values to [releases.json](releases.json), the data of the summary and the charts below, which `just release-charts` draws again; and takes the oldest release's column, notes and values away, which that release's own report keeps.

From 1.1.0 the sessions run of [a config of the least work](ab-F3b-20/config.toml) beside the terminal's own, no blinking nor ligatures, an opaque window, no quick tip, the same for every release from there, and 1.1.0's of the built-in display at 120 Hz: its column tells its latency, throughput and startup against 1.0.0's of its session, and the summary and the charts take its measures at rest alone, which neither the config nor the display moves. 1.2.0's session is of the display at 60 Hz again, of that config: its latency and its throughput join the charts, of a config that does less than the one before 1.1.0, as no blinking, which the releases before do not share; the startup's gap stays out of them, of a minimal app some 15 to 30 ms faster in the sessions from 1.1.0 on than before. 1.3.0's session is of a Dell U3417W at 60 Hz and a scale of 1, which halves the GPU's memory of both versions and takes away some of a live resize's missed refreshes, 1.2.0's 20 and 15 there against 33 and 31 in its own session; its minimal app started in 207 to 218 ms, as in the sessions before 1.1.0's, and its startup's gap joins the charts again.

Measures of different sessions compare within a few percent alone: a session's load and display move them, as each report tells; the A/B of a release against the one before, in one session of balanced orders, is what tells a cost. The window's motion, a drag by the titlebar and a live resize along a sine of 200 points, is measured as [motion-F3-152](motion-F3-152.md) measures it, of its config, Menlo at 20 points, 88 × 27 cells.

<!-- The summary and the charts, which tools/bench/releases.py writes from releases.json: edit those, not this. -->

## At a glance

1.3.0 against 1.2.0, both measured in 1.3.0's session, [ab-F5a-17](ab-F5a-17.md), but a measure that names another: each measure's change, the share of its budget, the best of the other terminals, of [the comparison of 13 November 2026](compare-F3-268.md) and [the window's motion](motion-F3-152.md), with Lokyr's own value of that session where its conditions differ, and whether it is as before, better, or worse beyond its rounds' spread.

| Measure | 1.2.0 | 1.3.0 | Change | Budget | The best of the others | |
|---|---|---|---|---|---|---|
| Startup, its gap to the minimal app's first window | 28.6 ms | 27.2 ms | −1.5 ms, −5 % | ≤ 35 ms, 78 % of it | — | ✔ As before |
| Latency, a key to its echo, p50 | 17.3 ms | 17.3 ms | +0.0 ms, +0 % | ≤ 33.3 ms, 52 % of it | Terminal.app 18.4, Lokyr 18.5 then | ✔ As before |
| Latency, a key to its echo, p99 | 32.4 ms | 31.5 ms | −0.9 ms, −3 % | ≤ 75 ms, 42 % of it | Alacritty 46, Lokyr 48.2 then | ✔ As before |
| Memory at rest, without the GPU's | 27.4 MB | 27.6 MB | +0.2 MB, +1 % | ≤ 35 MB, 79 % of it | Alacritty 31.1, Lokyr 27.8 then | ✔ As before |
| Idle, interrupt wakeups | 0.36/s | 0.36/s | ±0.00/s, ±0 % | ≤ 0.5/s, 72 % of it | Alacritty 0.37, Lokyr 0.35 then | ✔ As before |
| Idle, package idle wakeups | 0.00/s | 0.00/s | ±0.00/s | ≤ 0/s | — | ✔ As before |
| `cat` of 1 GiB of ASCII on screen | 298.6 MiB/s | 308.6 MiB/s | +10.0 MiB/s, +3 % | ≥ 294.9 MiB/s | Ghostty 328, Lokyr 318 then | ✔ As before |
| `seq 1 10000000` | 220.2 MiB/s | 218.3 MiB/s | −1.9 MiB/s, −1 % | — | — | ✔ As before |
| Colored text, SGR | 279.0 MiB/s | 285.2 MiB/s | +6.2 MiB/s, +2 % | — | — | ✔ As before |
| CJK and emoji | 217.2 MiB/s | 213.2 MiB/s | −4.0 MiB/s, −2 % | — | — | ✔ As before |
| A frame on the CPU, ASCII, p50 | 0.281 ms | 0.277 ms | −0.005 ms, −2 % | — | — | ✔ As before |
| A frame on the CPU, ASCII, p99 | 0.429 ms | 0.418 ms | −0.012 ms, −3 % | ≤ 2 ms, 21 % of it | — | ✔ As before |
| A frame on the CPU, CJK and emoji, p50 | 0.73 ms | 0.73 ms | −0.00 ms, −1 % | — | — | ✔ As before |
| A drag of the window, opaque, frames a second | 58.1/s | 58.3/s | +0.2/s, +0 % | — | Ghostty 58.3 | ✔ As before |
| A drag of the window, translucent with a blur, frames a second | 58.6/s | 58.5/s | −0.1/s, −0 % | — | Ghostty 57.3 | ✔ As before |
| A live resize, opaque, frames a second | 57.4/s | 57.4/s | ±0.0/s, ±0 % | — | Ghostty 57 | ✔ As before |
| A live resize, translucent with a blur, frames a second | 57.6/s | 57.6/s | +0.0/s, +0 % | — | Ghostty 57.7 | ✔ As before |
| A live resize, opaque, refreshes missed of 600 | 20.0 | 15.0 | −5.0, −25 % | — | Ghostty 25 | ✔ As before |
| A live resize, translucent with a blur, refreshes missed of 600 | 15.3 | 13.7 | −1.7, −11 % | — | Ghostty 17.5 | ✔ As before |
| A live resize, opaque, the compositor's delay, p95 | 13.2 ms | 21.0 ms | +7.8 ms, +59 % | — | Ghostty 27.7 | ✔ As before |
| A live resize, translucent with a blur, the compositor's delay, p95 | 18.3 ms | 18.4 ms | +0.1 ms, +1 % | — | Ghostty 27.5 | ✔ As before |

The latency's budgets, 2 refreshes at the p50 and 4.5 at the p99, set of a display at 120 Hz, are of these sessions' 60 Hz here: 33.3 and 75 ms. The throughput's of `cat` is the faster of Ghostty and Alacritty less 10 %, of [the comparison of 13 November 2026](compare-F3-268.md): Ghostty's 328 MiB/s.

## Over the releases

Each measure over the last eight releases, a dot of each release, its budget a dashed line, and the best of the other terminals a dotted one, of [the comparison of 13 November 2026](compare-F3-268.md), and of [the window's motion](motion-F3-152.md) for the window's motion, measured once of their versions then, Lokyr's own value of that session beside it where its conditions differ from the releases' sessions, the latency's by Typometer, of the screen; of the columns of [the table](#every-measure): different sessions, which compare within a few percent alone. A panel without a dashed line has no budget, as `seq`'s, SGR's, CJK's and the window's motion; without a dotted one, no other terminal was measured of it: that comparison timed no minimal app beside the others' first windows, which their startup's gap needs, nor the four workloads but `cat`, nor a frame's CPU.

![Starting and typing: the startup's gap to a minimal app; a key to its echo, at 60 Hz; the others' by Typometer](releases/start.svg)

![At rest: a window idle, its memory without the GPU's and its wakeups](releases/rest.svg)

![Output on screen: the throughput of four workloads, MiB/s, higher is better](releases/output.svg)

![A frame on the CPU: the CPU's time of a frame of steady output, p50 and p99, ms](releases/frames.svg)

![The window's motion: a drag by the titlebar and a live resize along a sine, 60 Hz](releases/motion.svg)

<!-- The end of what tools/bench/releases.py writes. -->

## Every measure

| Measure | 0.5.1 | 0.7.0 | 0.8.0 | 0.9.0 | 1.0.0 | 1.1.0 | 1.2.0 | 1.3.0 | Budget |
|---|---|---|---|---|---|---|---|---|---|
| Report | [ab-F3-190](ab-F3-190.md) | [ab-F3-77](ab-F3-77.md) | [ab-F4-25](ab-F4-25.md) | [ab-F4-42](ab-F4-42.md) | [ab-F3-85](ab-F3-85.md) | [ab-F3b-20](ab-F3b-20.md) | [ab-F3b-29](ab-F3b-29.md) | [ab-F5a-17](ab-F5a-17.md) | |
| Startup, warm, first frame, median | 235.8, 234.8, 234.9, 235.8 ms | 269.6, 272.1, 271.0, 253.2 ms | 250.8, 248.4, 233.8, 232.9 ms | 251.4, 249.8, 247.1, 247.2 ms | 222.3, 240.5, 241.3, 234.0 ms | 245.5, 243.8, 252.2, 246.8 ms, of a new copy; 245.6 ms of eight rounds of copies started as often, 1.0.0's 242.9 | 254.1, 262.8, 237.3, 252.3 ms, of a new copy; 254.3 ms of eight rounds of copies started as often, 1.1.0's 248.9 | 239.3, 270.6, 263.2, 284.4 ms, of copies started 30 times; 238.6 ms of eight rounds of copies started 60 times, 1.2.0's 240.0 | |
| Its gap to the minimal app's first window | 26.6 ms | 18.8 ms at rest, [startup-F3-228](startup-F3-228.md); 46.9 ms in the A/B, of a new copy | 24.9 ms, of a new copy started once before; 20.4 ms at rest against 0.7.0's 26.9, within the rounds' spread, [rest-F4-58](rest-F4-58.md#the-startup-at-rest) | 25.4 ms, of a copy new out of its build started once before, against 0.8.0's 23.8 in the session | 26.7 ms, of a copy new out of its DMG started once before, against 0.9.0's 28.9 in the session | 52.2 ms of a new copy against 1.0.0's 42.5 in the session, of a minimal app 15 ms faster than before; the startup 2.7 ms longer than 1.0.0's of copies started as often, [ab-F3b-20](ab-F3b-20.md#the-startup) | 58.5 ms of eight rounds against 1.1.0's 53.2, of a minimal app of 179 to 210 ms; the startup 5.3 ms longer, within the rounds' spread, [ab-F3b-29](ab-F3b-29.md#results) | 27.2 ms of eight rounds against 1.2.0's 28.6, of a minimal app of 207 to 218 ms, as before 1.1.0's sessions | ≤ 35 ms |
| Latency, p50 | 18.9 ms | 20.2 ms | 19.6 ms | 20.6 ms | 19.2 ms | 13.9 ms at 120 Hz, of the new config, against 1.0.0's 14.4 in the session | 16.6 ms at 60 Hz, against 1.1.0's 17.7 in the session | 17.3 ms at 60 Hz, against 1.2.0's 17.3 in the session | ≤ 2 refreshes, 33.3 ms at 60 Hz |
| Latency, p99 | 48.6 ms | 44.0 ms | 47.3 ms | 42.0 ms | 48.7 ms | 32.0 ms at 120 Hz, against 1.0.0's 32.3 | 31.5 ms, against 1.1.0's 31.2 | 31.5 ms, against 1.2.0's 32.4 | ≤ 4.5 refreshes, 75 ms at 60 Hz |
| Its samples in the fourth refresh or later | 11 of 1592 | 7 of 1592 | 10 of 1592 | 3 of 1592 | 11 of 1592 | 46 of 1592, 1.0.0's 54 | 0 of 1591, 1.1.0's 0 | 0 of 1592, 1.2.0's 0 | |
| Idle, package idle wakeups | 0/s | 0/s | 0/s | 0/s | 0/s | 0/s | 0/s | 0, 0/s over 240 s; 0.017/s in two of four rounds of 60 s, of the same interrupt wakeups as 1.2.0's, [ab-F5a-17](ab-F5a-17.md#results) | 0/s |
| Idle, interrupt wakeups | 0.35/s | 0.32, 0.35/s | 0.33, 0.35/s | 0.38, 0.38/s | 0.35, 0.35/s | 0.35, 0.35/s | 0.35, 0.33/s | 0.35, 0.35, 0.38, 0.35/s | ≤ 0.5/s |
| Footprint without the GPU's, at rest | 26.1 MB | 26.6, 27.0 MB | 27.1, 27.3 MB | 30.4, 26.9 MB | 27.6, 27.7 MB | 28.3, 28.2 MB | 28.4 MB of four rounds, 1.1.0's 27.9 | 27.6 MB, the median of four rounds, one of 31.5; 1.2.0's 27.4 | ≤ 35 MB |
| GPU memory | 30.3 MB | 30.1 MB | 30.1 MB | 30.1 MB | 30.1 MB | 30.6 MB | 30.6 MB | 14.2 MB, of a display at a scale of 1, as 1.2.0's in the session | |
| `cat` of 1 GiB of ASCII on screen | 318.6 MiB/s | 314.3 MiB/s | 315.0 MiB/s | 311.9 MiB/s | 310.9 MiB/s | 271.1 MiB/s at 120 Hz, against 1.0.0's 268.4 | 314.9 MiB/s, 1.1.0's 315.6 | 308.6 MiB/s, 1.2.0's 298.6 | ≥ 295 MiB/s, Ghostty's of the comparison of 13 November 2026 less 10 %; 275 of the first comparison's before |
| `seq 1 10000000` | 219.6 MiB/s | 220.3 MiB/s | 222.1 MiB/s | 223.8 MiB/s | 220.9 MiB/s | 192.6 MiB/s, 1.0.0's 193.6 | 217.9 MiB/s, 1.1.0's 213.6 | 218.3 MiB/s, 1.2.0's 220.2 | |
| Colored text, SGR | 287.4 MiB/s | 285.5 MiB/s | 285.6 MiB/s | 288.9 MiB/s | 284.7 MiB/s | 234.7 MiB/s, 1.0.0's 245.6, within the runs' spread | 286.3 MiB/s, 1.1.0's 286.4 | 285.2 MiB/s, 1.2.0's 279.0 | |
| CJK and emoji | 213.7 MiB/s | 217.4 MiB/s | 215.3 MiB/s | 216.5 MiB/s | 214.9 MiB/s | 207.8 MiB/s, 1.0.0's 209.4 | 214.9 MiB/s, 1.1.0's 214.6 | 213.2 MiB/s, 1.2.0's 217.2, within the tolerance of 5 % | |
| A frame on the CPU, ASCII, p50 | 0.270 ms | 0.283 ms | 0.296 ms | 0.288 ms | 0.282 ms | 0.385 ms, 1.0.0's 0.367 | 0.298 ms, 1.1.0's 0.295 | 0.277 ms, 1.2.0's 0.281 | p99 ≤ 2 ms |
| A frame on the CPU, CJK and emoji, p50 | 0.72 ms | 0.71 ms | 0.72 ms | 0.73 ms | 0.73 ms | 0.79 ms, 1.0.0's 0.77 | 0.78 ms, 1.1.0's 0.76 | 0.73 ms, 1.2.0's 0.73 | |
| A drag of the window, frames a second shown, opaque and translucent with a blur | 58.0, 58.0 | 58.0, 58.0 | not measured on this Mac, Defender scanning the recordings; 0.7.0's on an M1 Max at 120 Hz, [rest-F4-58](rest-F4-58.md#the-windows-motion-on-the-m1) | not measured on this Mac, Defender scanning the recordings: 0.8.0's within that noise, [ab-F4-42](ab-F4-42.md#the-windows-motion) | not measured | 57.7, 58.1, of 1.2.0's session | 57.1, 58.0, 1.1.0's 57.7 and 58.1 in the session, on the built-in display at 60 Hz | 58.3, 58.5, 1.2.0's 58.1 and 58.6 in the session, on a Dell U3417W at 60 Hz | |
| Its refreshes missed of 600 | 15.5, 16.5 | 16.0, 16.5 |  | |  | 16.0, 15.0, of 1.2.0's session | 23.0, 16.0 | 7.5, 9.0 | |
| A live resize, frames a second shown, opaque and translucent with a blur | 56.2, 56.5 | 56.3, 55.6 | not measured on this Mac; 0.7.0's on the M1, as the drag | not measured on this Mac, as the drag | not measured | 56.2, 55.8, of 1.2.0's session | 55.7, 56.3, 1.1.0's 56.2 and 55.8 | 57.4, 57.6, 1.2.0's 57.4 and 57.6 | |
| Its refreshes missed of 600 | 31.5, 30.0 | 33.5, 39.5 |  | |  | 31.5, 36.5, of 1.2.0's session | 33.0, 31.0 | 15.0, 13.7 | |
| A live resize, the window server's CPU and the app's | 36 % and 18 %, 37 % and 18 % | 39 % and 19 %, 42 % and 20 % |  | |  | 35 % and 16 %, 31 % and 15 %, of 1.2.0's session | 34 % and 16 %, 34 % and 15 % | 35 % and 15 %, 38 % and 15 % | |
| A live resize, the compositor's delay at the 95th percentile, opaque and a blur | 20.8, 28.3 ms | 20.2, 19.7 ms |  | |  | 20.6, 20.8 ms, of 1.2.0's session | 28.3, 19.8 ms, of runs of 13 or 28 ms for both versions, [ab-F3b-29](ab-F3b-29.md#the-windows-motion) | 21.0, 18.4 ms, of runs of 13 or 28 ms for both versions, [ab-F5a-17](ab-F5a-17.md#the-windows-motion) | |

The window's motion was first measured of 0.5.0, in [a motion session of 6 November](ab-F3-49.md#the-windows-motion), of the means of two rounds; [motion-F3-152](motion-F3-152.md) measured it of the builds before 0.5.0, against Ghostty's; 1.3.0's of two rounds of the opaque look and three of the blur. The latency's p50 and p99 are the means of the session's eight rounds. In 0.5.1's session, 0.5.0 measured 18.9 ms at the p50 and 46.6 ms at the p99, 9 samples in the fourth refresh or later, a startup's gap of 13.4 ms, 25.7 MB at rest, of the sessions' 25.6 and 25.7, 330.0 MiB/s of `cat` and a CJK frame of 0.44 ms, against which 0.5.1 is read; its motion, 57.4 and 57.2 frames a second of a drag, 55.8 and 56.2 of a resize, as 0.5.1's. The releases before 0.5.1 and the builds between them are in their own reports, from [ab-F3-13](ab-F3-13.md) on.

## The releases' features and their costs

- **0.5.1**, the plugins' block so far, released before the themes: the keymap's plugin, the tab bar with its drags, marks and looks, the splits, the windows' numbers, quitting with the last window, and the find bar with its highlights. Against 0.5.0 in [0.5.1's session](ab-F3-190.md), every budget holds: the latency, idle, the memory and the throughput are 0.5.0's within their spread, the footprint 0.5 MB larger. **The startup is 13 ms longer** in each of four rounds, its gap to the minimal app 26.6 ms against 13.4, within the budget of 35, the plugins' block's start. **A frame of CJK and emoji takes 0.27 ms more on the CPU**, and the p99 of the colored workloads' frames 1 to 2.5 ms more, of the same throughput, as in the build of 6 November. **A live resize** shows as many frames, of fewer missed refreshes, and one frame in twenty may come a refresh later, the compositor's delay at the 95th percentile 28 ms in three of four resizes against 13 to 15 in each of 0.5.0's of the session, though 0.5.0 showed 28 ms in two of four in the session of 6 November; the drags are 0.5.0's.
- **0.7.0**, the plugins' block's end and the daily features: the themes and their picker, the command palette, the shells' integration with its marks, gutter, header and notifications, the copy mode, the scroll bar, the programs' progress, the cursor's slide, windows restored, the Finder's services, the default terminal, Float on Top and the color scheme's reports. Against 0.5.1 in [0.7.0's session](ab-F3-77.md), every budget holds but the startup's gap: the latency's p50, idle, the memory, the throughput, the frames and the window's motion are 0.5.1's within their spread, the latency's p99 5.6 ms longer, within 0.7.0's spread, 7 samples in the fourth refresh against 1. The startup 30 ms longer in the session, its gap 46.9 ms past the budget of 35, of a copy freshly out of its DMG, which Microsoft Defender scanned at its starts; at rest, each copy started before, [startup-F3-228](startup-F3-228.md) found its gap 18.8 ms against 0.5.1's 15.1, within the budget and the spread, which the summary and the charts take, and the default terminal's look, on a thread of its own since the first draft, taking nothing of the start. The first draft's window, started from a shell, opened a second late and behind the app in front, which the motion's session found and a fix took away before the release.
- **0.8.0**, the Luau host's block: third-party plugins in Luau, each of a VM of its own and its sandbox, limits and watchdog, installed in a store and reloaded at once, the consent to their capabilities, and Sleip in the bundle, of `new`, `check`, `test`, `dev`, `add` and `update`. Against 0.7.0 in [0.8.0's session](ab-F4-25.md), every budget holds: the latency's p50, idle, the memory, the throughput and the frames are 0.7.0's within their spread; the latency's p99 7.4 ms longer, in seven rounds of eight, and the startup's gap 7 ms, of its first two rounds, within their budgets. [rest-F4-58](rest-F4-58.md) measured both again at rest: the startup 0.7.0's within its rounds' spread, the tail longer in some sessions of the managed Mac and not in others, of no code between the two, and of `allow-jit`, the entitlement of Luau's native code, not at all on a Mac of no management, an M1 Max, where the window's motion is 0.7.0's too; `allow-jit` stays. A Luau plugin active costs 0.9 MB, none without one; the app's code 2.6 MB larger, Luau's runtime 1.1 MB of it, the bundle 35.3 MB of Sleip's 13.1 and the DMG 14.7 MB.
- **0.9.0**, the plugin ecosystem's block: the registry, pkg.lokyr.dev, of its signed index, Sleip's `search`, `info`, `add` and `publish` of it, `snap`, `awake` and the font picker published, the host's images, save panel, power assertion and titlebar's buttons, the notices as cards and the quick tips. Against 0.8.0 in [0.9.0's session](ab-F4-42.md), every budget holds and no measure is worse beyond its spread: the latency's p50 0.2 ms shorter and its p99 1.1 ms longer, within the rounds' deviations; the startup's gap 1.5 ms longer, of its first round; the interrupt wakeups 0.38 a second against 0.35, of the account's own home alone, 0.35 of a home of its own. `awake` active and `snap` installed cost 1.6 and 1.9 MB, none at rest; the quick tips no timer at a start of no tip due. The bundle 40.4 MB, Sleip's 16.1 of its registry's client, and the released DMG of 17.7 MB. The window's motion, measured while Microsoft Defender scanned Instruments' recordings, as in 0.8.0's session, showed both apps at half their frames, 0.9.0 0.8.0's within that noise.
- **1.0.0**, the first public release: the AI agents on their tabs, their notifications, the Dock's badge and Claude Code's integration, a path opened at its line in the editor, a command's output copied or selected, inline images of iTerm2's and kitty's protocols, and the Homebrew cask. Against 0.9.0 in [1.0.0's session](ab-F3-85.md), every budget holds and no measure is worse beyond its spread: the latency's p50 0.9.0's and its p99 1.1 ms longer, within the rounds' deviation; the startup's gap 2.1 ms shorter; the footprint 0.5 MB larger, of a binary 0.5 MB larger; idle and the throughputs 0.9.0's. The window's motion was not measured.
- **1.1.0**, the settings in a window of their own: every key of the config, the keys' bindings, the themes' previews and the plugins, without editing a file. Against 1.0.0 in [1.1.0's sessions](ab-F3b-20.md), of the new config and the built-in display at 120 Hz, no measure is worse beyond its spread: the latency's p50 0.5 ms shorter and its p99 1.0.0's; idle 1.0.0's; the footprint 0.4 MB larger, of a binary 1 MB larger; the throughputs within their runs' spread; a frame of ASCII 0.018 ms longer on the CPU, 5 %, of no throughput lost. The startup is 2.7 ms longer, of copies started as often, within the rounds' spread: the sessions found 6.6 and 9.7 ms of a new copy, which a binary that the system has seen little of starts slower, as 1.0.0 rebuilt of its tag did by 16 ms. The startup's gap to the minimal app and `cat`'s throughput held for neither version, of a minimal app 15 ms faster than in the sessions before and of the display at 120 Hz. The window's motion was not measured.
- **1.2.0**, themes made in the app: the Theme Editor of every color of a theme, its values edited of a field or the system's color panel and written into the theme's file, and its previews of a window and of a plugin's panel in the theme edited; themes made, duplicated, converted and imported in the app; the interface's colors of rules that follow the theme's own; faint text, the cursor guide and the HDR cursor; the settings' Keys page; the settings window of Lokyr's own themes. Against 1.1.0 in [1.2.0's sessions](ab-F3b-29.md), of the display at 60 Hz, every budget holds but the startup's gap, which holds for neither version, and no measure is worse beyond its spread: the latency's p50 1.1 ms shorter, in seven rounds of eight, and its p99 1.1.0's; idle, the throughputs and the frames 1.1.0's; the footprint 0.5 MB larger, in each of four rounds, of the app's heap, of a binary 0.7 MB larger. The startup is 5.3 ms longer, of copies started as often, within the rounds' spread. The window's motion, measured of both in its session, is 1.1.0's: as many frames a second of a drag and of a resize, as many refreshes missed, the same CPU; the compositor's delay at the 95th percentile a refresh longer in more of 1.2.0's runs, in a session whose load rose, its median 13 ms for both.
- **1.3.0**, the programs' status, the prompt's click and the tabs: a program's status of OSC 7501 on its tab, a session's status of OSC 21337 and the notifications of OSC 99; a click in the command line that moves the shell's cursor, and a selection there that typing replaces; ⌃Tab of the tabs used last, pinned tabs, tab groups and a closed tab reopened; hints of the screen's URLs, paths, hashes and addresses, labeled to open or copy; the quick terminal of a double-tapped modifier; Use Transparency; the settings' editor of the hints' patterns; and the text fields' keys and context menu. Against 1.2.0 in [1.3.0's session](ab-F5a-17.md), of a display at a scale of 1, every budget holds and no measure is worse beyond its spread: the latency's p50 1.2.0's and its p99 0.9 ms shorter; the startup 1.5 ms shorter, of copies started as often, its gap to the minimal app 27.2 ms, within the budget; the footprint 0.2 MB larger, of a binary 0.7 MB larger; idle 1.2.0's, of no package idle wakeup over four minutes of each, the two of 1.3.0's in the rounds of 60 s chance of the same interrupt wakeups; `cat` 3 % faster and CJK and emoji 1.8 % slower, within their tolerance of 5 %, of the same frames' CPU. The window's motion is 1.2.0's, as many frames a second of a drag and of a resize, the compositor's delay at the 95th percentile of runs of 13 or 28 ms for both.

## Every report

Every session's report, the newest first.

| Date | Report |
|---|---|
| 18 November 2026 | [1.3.0 against 1.2.0](ab-F5a-17.md) |
| 16 November 2026 | [1.2.0 against 1.1.0](ab-F3b-29.md) |
| 14 November 2026 | [1.1.0 against 1.0.0](ab-F3b-20.md) |
| 13 November 2026 | [Lokyr against the other terminals, again](compare-F3-268.md) |
| 13 November 2026 | [1.0.0 against 0.9.0](ab-F3-85.md) |
| 12 November 2026 | [The inline images' cost when none shows](ab-F3-249.md) |
| 11 November 2026 | [0.9.0 against 0.8.0](ab-F4-42.md) |
| 9 November 2026 | [0.8.0's latency tail, startup and motion again, and `allow-jit`](rest-F4-58.md) |
| 9 November 2026 | [0.8.0 against 0.7.0](ab-F4-25.md) |
| 8 November 2026 | [The startup's gap at rest](startup-F3-228.md) |
| 8 November 2026 | [The cursor's movement's cost](movement-F3-72.md) |
| 8 November 2026 | [Lokyr again, against the other terminals of the first comparison](compare-F3-72.md) |
| 8 November 2026 | [0.7.0 against 0.5.1](ab-F3-77.md) |
| 7 November 2026 | [The search of 100 000 lines through the app's service](search-F3-52.md) |
| 7 November 2026 | [0.5.1 against 0.5.0](ab-F3-190.md) |
| 6 November 2026 | [The build of 6 November against 0.5.0, and the single tab's title](ab-F3-49.md) |
| 5 November 2026 | [The window's motion, its resize and the cursor's fades](motion-F3-152.md) |
| 5 November 2026 | [The latency beside a plugin on its own thread that renders without pause](latency-F3-35.md) |
| 5 November 2026 | [The latency's tail and the startup of 0.5.0, bisected](latency-F3-163.md) |
| 5 November 2026 | [0.5.0 against 0.4.0](ab-F3-40.md) |
| 4 November 2026 | [The latency's tail of 0.4.0, a first session](latency-F3-135.md) |
| 4 November 2026 | [0.4.0 against 0.3.0](ab-F3-29.md) |
| 2 November 2026 | [The first frame a refresh later than 0.2.0's](startup-F3-105.md) |
| 2 November 2026 | [0.3.0 against 0.2.0](ab-F3-83.md) |
| 2 November 2026 | [The architecture of the MVP against the single pane's](ab-F3-13.md) |
| 31 October 2026 | [Startup](startup-F2-53.md) |
| 31 October 2026 | [CPU per frame and throughput on screen](onscreen-F2-56.md) |
| 31 October 2026 | [Latency](latency-F2-54.md) |
| 31 October 2026 | [A frame's new glyphs](glyphs-F2-146.md) |
| 31 October 2026 | [Lokyr against the other terminals](compare-F2-134.md) |
| 30 October 2026 | [Drawables at 60 Hz](drawables-F2-57.md) |
| 29 October 2026 | [Idle, memory and GPU memory](idle-memory-F2-55.md) |
| 25 October 2026 | [Throughput of the finished VT core](throughput-F1-56.md) |
| 23 October 2026 | [Throughput after the performance fixes](throughput-F1-51.md) |
| 23 October 2026 | [Throughput of the review fixes](throughput-F1-133.md) |
| 20 October 2026 | [Instruction counts in CI](instructions-F1-52.md) |
| 19 October 2026 | [Throughput of the VT core](throughput-F1-50.md) |
| 19 October 2026 | [Search over the scrollback](search-F1-54.md) |
| 18 October 2026 | [esctest, after the second compatibility fixes](esctest-F1-46.md) |
| 18 October 2026 | [esctest, after the first compatibility fixes](esctest-F1-45.md) |
| 18 October 2026 | [esctest, before the compatibility fixes](esctest-F1-44.md) |
| 12 October 2026 | [The baseline of the existing terminals](baseline-F0.md) |
