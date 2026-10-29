# Drawables at 60 Hz (F2-57)

Measured on 2026-10-30 by the user, with the performance overlay of F2-122, on two bundles of the same code but the drawables' count: two and three per window. The overlay's display wait is how long a frame waited in `nextDrawable`, and its CPU the rest of the frame.

## Machine

- Apple M5 Pro, macOS 27.2.
- Display: Samsung U28E590, 3008 × 1692 points at 60 Hz, a scale of 2. The MacBook's own display, of 120 Hz, was not in use: the measure at 120 Hz is still to make.

## Results

| | Two drawables | Three drawables |
|---|---|---|
| Output, `yes`, at 60 Hz | 60 frames a second; CPU 0.14 ms; display wait 1.50 ms | 60 frames a second; CPU 0.13 ms; display wait 1.25 ms |
| A live resize, during the output | display wait 15 ms | display wait 15 ms |

- **At 60 Hz, two drawables keep a frame waiting 1.5 ms**, and a third takes 0.25 ms of it away. The frame of 16.65 ms that the overlay's first version showed, on 29 October 2026, before it told the wait apart, is not seen again.
- **A live resize waits about a frame** with either count: the live resize presents each frame with its transaction, synchronously, so that the content follows the window's edge, which [F2-07](../tasks/F2-macos-single-pane.md#f2-07--live-resize) takes up.
