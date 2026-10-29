# Idle, memory and GPU memory (F2-55)

Measured on 2026-10-29 with `lokyr-bench`, on the bundle of commit 146e8ee, against the [budgets](../PERFORMANCE.md#budgets). The raw reports are in [idle-memory-F2-55/](idle-memory-F2-55/).

## Method

- The bundle, `target/Lokyr.app`, started with `open -n` and a config of its own, through `XDG_CONFIG_HOME`, so that the user's config, and its performance overlay, take no part: the defaults, but the cursor's keys that each measure names.
- Its one window in front, as `lsappinfo front` told, with nobody at the keyboard, so that its cursor blinks.
- `lokyr-bench idle --duration 60` for the wakeups, after five seconds for each change of the config to settle; `lokyr-bench memory` for the footprint, once a second for 10 s, and every 20 ms from the start.
- `sample` for 10 s, for what wakes the app; `footprint` for the memory's categories.

## Machine

- Apple M5 Pro, 64 GB of memory, macOS 27.2, on AC power.
- Display: Samsung U28E590, 6016 × 3384 pixels, as 3008 × 1692 points at 60 Hz: a scale of 2.
- The window: 1412 × 848 points, 2824 × 1696 pixels.

## Wakeups

| The cursor | Package idle wakeups/s | Interrupt wakeups/s | Budget |
|---|---|---|---|
| Blinking, switched (`smooth-blink = "off"`) | 0 | 0.45 | ≤ 2 |
| Blinking, faded (`smooth-blink = "breathing"`) | 0 | 0.35 | ≤ 2 |
| Steady (`blink = false`), as after the timeout | 0 | 0.35 | ≤ 0.5 |

- **The blinking costs nothing more** than a steady cursor, faded or not: the window server blinks the cursor's layer ([ADR 0021](../adr/0021-drawables-and-the-cursor-layer.md)). Before F2-123, a blink took a frame, and 22 wakeups a second, as F2-20 measured, on the debug build.
- **What wakes it, about once every 3 s**, while nothing changes: in a sample of 10 s, the main thread woke once, for a query of an accessibility client's, `_XCopyAttributeValue`, from outside Lokyr; the other threads waited, the session's reader in `poll`, its writer on its condition, and the config's watch in its run loop. The minimal AppKit app of F0 took 0.02 a second, likely without that client; F2-06 found the same 0.33 a second.

## Memory

| One window | Footprint | GPU | Without the GPU | Budget |
|---|---|---|---|---|
| Three minutes after the start | 93.5 MB | 69.5 MB | 24.0 MB | ≤ 35 MB without the GPU |
| Four seconds after another start | 100.6 MB | 69.5 MB | 31.1 MB | |

- **The GPU's memory**: 56 MB of IOSurface, the three drawables of 2824 × 1696 pixels, 19.2 MB each, as F2-57 chose; about 6 MB of IOAccelerator, the atlas's textures and the frames' buffers; the cursor's layer, a cell of pixels.
- **A peak at the start**: the GPU's memory reaches 235 MB in the first 0.2 s, and stays there for 1.8 s, before it falls to 72 MB, then 69.5 MB. The footprint's peak, over the process's life, is 267 MB. At 0.5 s, `footprint` names the peak "Owned physical footprint (unmapped) (graphics)": 156 MB in 60 regions, memory of Metal's that the process owns and does not map; at 3.5 s, 4.4 MB of it is dirty, and 104 MB reclaimable, which the system can take back. The drawables' IOSurface stays 56 MB throughout. Its source, Metal's own allocations at the device's or the pipelines' making, or the first frames' buffers, which Metal keeps to use again, is for Instruments' Metal resource allocations to tell.

## Still to measure, with the user at the Mac

- The footprint with five windows, and the drawable that a hidden window keeps.
- The wakeups of the frames of output, and where each comes from.
- Blinking text, which still draws a frame at each blink.
- The window server's cost of a translucent window, with and without its blur, over a video that plays behind it.
