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
| Steady, with blinking text in the window (SGR 5) | 0.13 | 22.3 | ≤ 2 |
| Output, `yes`, at 60 Hz | 0 | 130.6 | none: output draws frames |

- **The blinking costs nothing more** than a steady cursor, faded or not: the window server blinks the cursor's layer ([ADR 0021](../adr/0021-drawables-and-the-cursor-layer.md)). Before F2-123, a blink took a frame, and 22 wakeups a second, as F2-20 measured, on the debug build.
- **Blinking text takes a frame at each blink**, about 11 wakeups each, as the cursor did: past the budget, which [F2-125](../tasks/F2-macos-single-pane.md#f2-125--blinking-text-on-a-layer-of-its-own) brings within it.
- **Output**: about two wakeups a frame; the sample shows the work where it belongs, the parser and the grid's scrolling.
- **What wakes it, about once every 3 s**, while nothing changes: in a sample of 10 s, the main thread woke once, for a query of an accessibility client's, `_XCopyAttributeValue`, from outside Lokyr; the other threads waited, the session's reader in `poll`, its writer on its condition, and the config's watch in its run loop. The minimal AppKit app of F0 took 0.02 a second, likely without that client; F2-06 found the same 0.33 a second.

## Memory

| One window | Footprint | GPU | Without the GPU | Budget |
|---|---|---|---|---|
| Three minutes after the start | 93.5 MB | 69.5 MB | 24.0 MB | ≤ 35 MB without the GPU |
| Four seconds after another start | 100.6 MB | 69.5 MB | 31.1 MB | |
| Four windows, one of a full scrollback of 10 MB, after the output | 141.3 MB | 92.3 MB | 49.0 MB | ≤ 38 MB for four panes, but the scrollback, ≤ 10.5 MB |

- **The GPU's memory**: 56 MB of IOSurface, the three drawables of 2824 × 1696 pixels, 19.2 MB each, of the bundle measured, which [ADR 0022](../adr/0022-two-drawables.md) brought back to two, 38 MB; about 6 MB of IOAccelerator, the atlas's textures and the frames' buffers; the cursor's layer, a cell of pixels.
- **More windows**: without the GPU, the footprint grows by about 3 to 5 MB a window, AppKit's window, its views and its layers with the pane's; a pane of its own, in a split, comes with F3, which measures it against its budget of 1 MB. The scrollback that `yes` filled took its limit, 10 MB, as the budget allows.
- **A window in the Dock** lets all three of its drawables go: the IOSurface fell by 20 MB, three drawables of 1716 × 1010 pixels, where a covered window keeps one.
- **Metal's memory while frames are drawn**: the same unmapped memory of Metal's comes back whenever the app draws, 93 to 95 MB dirty while output flows or text blinks, and becomes reclaimable about 1.5 s after the last frame. It is Metal's own, the driver's, as [F2-126](../tasks/F2-macos-single-pane.md#f2-126--the-frames-buffers-used-again) found: it stays 90 to 93 MB with buffers that the frames use again, with a font of 40 points, and in a window of 40 by 10 cells as of 200 by 60; and a loop that clears a surface of 64 by 32 pixels, with nothing of Lokyr's, takes 85 MB of it while it draws, 56 MB of which is reclaimable 3 s after.
- **A peak at the start**: the GPU's memory reaches 235 MB in the first 0.2 s, and stays there for 1.8 s, before it falls to 72 MB, then 69.5 MB. The footprint's peak, over the process's life, is 267 MB. At 0.5 s, `footprint` names the peak "Owned physical footprint (unmapped) (graphics)": 156 MB in 60 regions, memory of Metal's that the process owns and does not map; at 3.5 s, 4.4 MB of it is dirty, and 104 MB reclaimable, which the system can take back. The drawables' IOSurface stays 56 MB throughout. It is Metal's own memory, the driver's, of the frames that the start draws, which F2-126 found.

## The window server, over a video

A video plays in a browser behind the window, which covers part of it, and nothing changes in the window: the window server's CPU, over 30 s, as `top` gives it, in a core's percent.

| The window | Window server's CPU |
|---|---|
| Opaque | 33.5 % |
| Translucent, `background-opacity = 0.85`, without blur | 32.9 % |
| Translucent, with `background-blur = 20` | 37.0 % |

Translucency alone costs nothing that one run tells apart; the blur, about 4 % of a core more, while what is behind the window changes. The window server's GPU is `powermetrics`'s, which needs the administrator, and is not measured.
