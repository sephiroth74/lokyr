# A hidden session's windows and their frames

Measured on 25 November 2026. A window that a session hides lets its drawables go and presents a frame of a pixel in place of its last one; it draws its frame of now before it shows again. This report measures what that gives back, of 1.5.0's tag built into a bundle against the change's bundle, both signed alike, and where the rest of a hidden session's memory goes. The sessions' reports, [at a scale of 2](hidden-session-F5b-47/scale-2/report.md) and [at a scale of 1](hidden-session-F5b-47/scale-1/report.md), are in [hidden-session-F5b-47](hidden-session-F5b-47/), of the scenario [`hidden-session-memory`](../../tools/bench/sessions/scenarios/hidden-session-memory.toml).

## The scenario's memory

Each copy started alone, and with a second session whose window hides, four times each in turned orders, its memory read ten seconds after the launch: the physical footprint of the terminal's processes, its shells too, as ttybench sums them.

| Display | 1.5.0, a window | and a hidden session | the change, a window | and a hidden session | The hidden session, 1.5.0 against the change |
|---|---|---|---|---|---|
| Built-in, Retina, a scale of 2 | 56.9 MiB | 96.3 MiB (+39.5) | 54.4 MiB | 85.2 MiB (+30.8) | **−11.1 MiB (−11.5 %)**, −12.5 … −9.8 |
| External, 3440 × 1440, a scale of 1 | 43.4 MiB | 60.3 MiB (+16.9) | 45.2 MiB | 57.8 MiB (+12.6) | **−2.4 MiB (−4.0 %)**, −3.2 … −2.1 |

- **The change gives back a drawable of the hidden window**: about 9.7 MB for a window of 1936 × 1320 pixels at a scale of 2, a quarter of that at a scale of 1. 1.5.0's +39.5 MiB at a scale of 2 is [1.5.0's session](ab-F5b-20.md)'s +38.7, which was at a scale of 2 too.
- **The peak footprint rises for a moment**, by 13.6 MiB at a scale of 2 and 6.6 at 1, as the window that shows draws a new drawable of its size while the hidden one's frame of a pixel is still in the GPU's hands. It lasts the switch.
- **A switch draws a frame of each window that it shows** before it shows it: a frame on the CPU, 0.45 ms at the p99 in [1.5.0's measures](ab-F5b-20.md#the-measures-of-lokyr-bench), well within a refresh.

## Where the rest goes

`footprint` of Lokyr's own process, of the change's bundle at a scale of 2, ten seconds after the launch, and of its drawables by their sizes:

| | Lokyr's footprint | IOSurface | Drawables of 1936 × 1320 |
|---|---|---|---|
| A window | 64 MB | 20 MB | 2 |
| A hidden session, 1.5.0 | 89 to 90 MB | 40 MB | 4: the hidden window keeps its two |
| A hidden session, the change | 78 MB | 30 MB | 3: the hidden window keeps one |
| A floating pane, the change | 68 MB | 20 MB | 2 |

- **Most of the rest is the hidden session's shell**, a process of its own, which ttybench counts and Lokyr's own footprint does not: Lokyr adds 14 MB for a hidden session, of which 10 MB is the drawable that the window still keeps and 3 MB its terminal.
- **The drawable that stays is the layer's**: the drawables are the pool of the window's view's layer, which Core Animation empties as it chooses. A frame of a pixel presented with Core Animation's transaction, as a trial, kept both of the hidden window's drawables, and was left out. Giving the last one back would take a new layer for the hidden window.
- **A floating pane holds no drawable of its own**, as it draws into its window's surface: it adds 4 MB to Lokyr, its terminal's, and its shell beside it, most of 1.5.0's +17.6 MiB.
