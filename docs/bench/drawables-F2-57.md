# Drawables at 60 Hz

Measured on 2026-10-30 by hand, with the performance overlay, on two bundles of the same code but the drawables' count: two and three per window. The overlay's display wait is how long a frame waited in `nextDrawable`, and its CPU the rest of the frame.

## Machine

- Apple M5 Pro, macOS 27.2.
- Display: Samsung U28E590, 3008 × 1692 points at 60 Hz, a scale of 2. The MacBook's own display, of 120 Hz, was not in use: the measure at 120 Hz is still to make.

## At 120 Hz, from the log

On 2026-10-30, the MacBook's own display, ProMotion, at 120 Hz: each bundle started with the overlay on, which logs its numbers each second, and 30 s of `yes` written into its pane's terminal. The logs are in [drawables-F2-57/](drawables-F2-57/).

| Under output, over 30 s | Two drawables | Three drawables |
|---|---|---|
| Frames a second, the median | 120 | 120 |
| Display wait, the median of the seconds' medians | 0.34 ms, from 0.33 to 0.35 | 0.34 ms, from 0.32 to 0.36 |
| The longest wait, the median of the seconds' | 0.52 ms | 0.53 ms |
| The longest wait | 0.64 ms | 3.88 ms, once |
| Output | 108 MB/s | 107 MB/s |

**At 120 Hz, two drawables never keep a frame waiting**: the third changes nothing.

## At 60 Hz, as read on the overlay

| | Two drawables | Three drawables |
|---|---|---|
| Output, `yes`, at 60 Hz | 60 frames a second; CPU 0.14 ms; display wait 1.50 ms | 60 frames a second; CPU 0.13 ms; display wait 1.25 ms |
| A live resize, during the output | display wait 15 ms | display wait 15 ms |

- **At 60 Hz, two drawables keep a frame waiting 1.5 ms**, and a third takes 0.25 ms of it away. The frame of 16.65 ms that the overlay's first version showed, on 29 October 2026, before it told the wait apart, is not seen again.
- **A live resize waits about a frame** with either count, of 60 Hz as of 120 Hz, as read on the overlay: 15 ms and 6 to 7 ms: the live resize presents each frame with its transaction, synchronously, so that the content follows the window's edge, which later work on the live resize takes up.
