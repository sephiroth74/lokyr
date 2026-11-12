# The first frame a refresh later than 0.2.0's

[0.3.0's A/B](ab-F3-83.md) found 0.3.0's warm startup a refresh at 60 Hz later than 0.2.0's, its runs on the next step of 16.7 ms. Measured on 2026-11-02 in two sessions of warm startups, in turns, three rounds of 10 runs after 2 each, `lokyr-bench startup --first-frame`, with that A/B's config, on [its Mac](ab-F3-83.md#machine), on AC power. The scripts are in [startup-F3-105](startup-F3-105/), `run.sh` and `verify.sh`, the reports in `four` and `fixed`.

## Which part: four bundles

At 16:47, four bundles that differ by one part each:

| Bundle | Median of 30 | Mean | The runs' steps, mostly | Gap to the minimal app, by round |
|---|---|---|---|---|
| A, 0.2.0 | 223.1 ms | 226.7 ms | the first | −5.6, −4.2, 4.6 ms |
| B, 0.3.0's DMG's app, notarized | 236.7 ms | 242.6 ms | the second | 8.2, 17.6, 9.2 ms |
| C, 0.3.0's code in a bundle of 0.2.0's contents, without Sparkle and the crates' licenses, signed | 236.5 ms | 238.8 ms | the second | 10.6, 10.3, 6.6 ms |
| D, 0.3.0's code in its whole bundle, signed, not notarized | 239.8 ms | 246.4 ms | the second | 19.8, 8.6, 9.8 ms |

C takes B's step: neither Sparkle, nor the licenses, nor the notarization, but the code.

## Where: the start's phases

The start of 0.2.0 and of 0.3.0, traced for the purpose, in milliseconds from `main`, the median of five runs: the same to the window's opening, 105 and 104.5 ms to `start`, 142.4 and 145.6 ms to the window's open, its first frame drawn within it, of the window's own; then 0.2.0's second frame came at the display's next turn, 155.9 ms, and 0.3.0's at once, 145.8 ms. Since the plugins' events, the turn that ends the start delivers the workspace's events to the plugins, after which the app asked every window for a frame, whether the plugins wrote or not; the platform drew it at once, beside the window's first: two frames in a row, and the window shown a refresh later.

## The fix

The plugins' host tells whether its calls wrote to the kernel's state, and the app asks for frames after a turn of the plugins only then. At 20:56, a second session, the machine slower, of 0.2.0, C again, and E, 0.3.0's code fixed, in its whole bundle:

| Bundle | Median of 30 | Mean | The runs' steps, mostly | Gap to the minimal app, by round |
|---|---|---|---|---|
| A, 0.2.0 | 223.4 ms | 229.3 ms | the first and the second | −9.2, −2.7, −2.6 ms |
| C, 0.3.0 unfixed | 267.5 ms | 258.2 ms | the fourth | 34.5, 35.5, 22.0 ms |
| E, 0.3.0 fixed | 235.3 ms | 230.6 ms | the first and the second | 3.7, 3.1, −2.0 ms |

- **E takes 0.2.0's steps**: its mean is A's within 1.3 ms, its runs on the first and the second steps as A's; the medians differ by how ten runs split between two steps.
- **C, that evening, took two refreshes more**, at the budget's 35 ms over the minimal app in two rounds: the second frame at once costs more as the machine is busier.
