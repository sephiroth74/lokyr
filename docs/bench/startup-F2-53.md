# Startup (F2-53)

Measured on 2026-10-31 with `lokyr-bench startup`, on the bundle of commit fd89e06, against the [budgets](../PERFORMANCE.md#budgets). The raw reports are in [startup-F2-53/](startup-F2-53/).

## Method

- **Lokyr's first frame**: `lokyr-bench startup --app target/Lokyr.app --first-frame` starts the bundle's executable with `--bench startup`, which tells, as JSON, the time from the process's start, as the system recorded it, to the first frame of its first window on the display, as the drawable's presented handler tells it, and quits. It runs outside LaunchServices, which adds about 20 ms before the process starts and nothing after.
- **The first window**: `lokyr-bench startup --app <app>` launches the app through LaunchServices, as the Dock does, and polls the window server for its first window on screen, as the [F0 baseline](baseline-F0.md) did: for Lokyr, Ghostty and the minimal AppKit app of F0, which gives the floor that macOS sets on this machine.
- **Warm**: 10 runs after 2 warmup runs. **Cold**: 10 runs, `sudo purge` before each, which empties the file cache, without warmup runs.
- Lokyr with the user's config: its font, NotoSansM NFM Condensed SemiBold at 20 points, a window of 88 × 27 cells, translucent and blurred. Ghostty with the user's config. Terminal.app and iTerm2 were open with the user's sessions, and kitty, WezTerm and Alacritty are no longer installed: their figures are F0's.

## Machine

- Apple M5 Pro, 64 GB of memory, macOS 27.2, **on battery**, normal power mode; F0 measured on AC power.
- Display: the built-in one, 3456 × 2234 pixels at a scale of 2.

## Results

| App | Measure | Warm, median | Warm, p90 | Warm spread | Cold, median | Cold, p90 | Cold spread |
|---|---|---|---|---|---|---|---|
| Lokyr | First frame | 246.9 ms | 271.3 ms | 14.0% | 882.0 ms | 905.5 ms | 21.5% |
| Lokyr | First window | 245.7 ms | 252.3 ms | 9.8% | 616.3 ms | 681.6 ms | 19.4% |
| Ghostty | First window | 284.3 ms | 293.1 ms | 5.0% | 801.8 ms | 836.8 ms | 27.8% |
| Minimal AppKit app | First window | 192.6 ms | 197.1 ms | 4.5% | | | |

- **Warm**, Lokyr's first frame takes 246.9 ms, past the budget of 200 ms by 47 ms; the minimal AppKit app's first window takes 192.6 ms on this machine today, 176 ms in F0 on AC power, so that Lokyr's own work is about 54 ms. Lokyr's first window and its first frame come together, warm: the window shows with its frame.
- **Cold**, the first frame takes 882 ms, and the first window 616 ms: the window shows before its first frame, as the font's files and the shaders come from the disk, so that the first window understates the startup, cold, for Lokyr as for the other terminals. Ghostty's first window takes 802 ms cold, 284 ms warm, 331 ms in F0.
- **The spread** of the cold runs, 19 to 28%, is the file cache's: what `purge` leaves, and what the system reads again meanwhile. The spreads are of the central runs, the 2nd to the 9th of ten, as the review after F2-148 had `lokyr-bench` take them, where its 10th percentile was the fastest run, and a single fast run, of 532 ms, made Lokyr's first frame's cold spread 42.3%.

## Where the warm startup goes

A build marked at each phase, from the bundle, outside the measures, five runs, from the process's start:

| Phase | From the start | Its time |
|---|---|---|
| `main` | 11 ms | 11 ms: the executable and its frameworks loaded |
| The drawing made: the GPU, its pipelines and the font | 55 ms | 44 ms |
| AppKit's launch, to `applicationDidFinishLaunching` | 112 ms | 55 ms |
| The app's start: languages, commands, plugins, menus | 115 ms | 3 ms |
| The window made, its view and its layers | 150 ms | 35 ms |
| The pane's shell started, and the first frame drawn | 157 ms | 7 ms |
| The first frame on the display | 227 ms | 70 ms |

- **The drawing is made before AppKit's launch**, on the main thread, one after the other: making it on a thread of its own as AppKit launches would take most of its 44 ms off, which [F2-143](../tasks/F2-macos-single-pane.md#f2-143--the-drawing-made-beside-appkits-launch) does.
- **The rest is the system's** but the window's making: AppKit's launch, and the 70 ms from the first frame drawn to the display, which the minimal app's first window takes too.
- **Outside the bundle**, the app's start takes 19 ms more, as the icon's file is read and shown, where the bundle's own icon shows already.

## After F2-143

[F2-143](../tasks/F2-macos-single-pane.md#f2-143--the-drawing-made-beside-appkits-launch) makes the drawing on a thread of its own as AppKit launches, and the app its own after the launch, which then takes next to nothing. The raw reports are in [startup-F2-53/after-F2-143/](startup-F2-53/after-F2-143/).

| Power | Minimal AppKit app, first window | Lokyr, first frame | The gap |
|---|---|---|---|
| Battery | 192.6 ms | 225.2 ms, from 246.9 ms | 32.6 ms |
| AC | 225.8 ms | 255.4 ms | 29.5 ms |

- **About 20 ms of the 44 came off**: AppKit's launch takes about 105 ms beside the warmup, where it takes 55 ms alone, as both load frameworks at once.
- **On AC power**, both took longer, with Microsoft Defender scanning each launch at about a fifth of a core, and a load of 4.4: the gap to the minimal app holds, 30 to 33 ms, where the times move by 30 ms.

## The budgets

- **Warm**: ≤ 200 ms, missed by 25 to 55 ms as the machine's state moves the floor; the user chose, on 31 October 2026, a budget of ≤ 35 ms past the minimal AppKit app's first window, measured in the same session, which Lokyr meets, at 30 to 33 ms.
- **Cold**: ≤ 1 s for the first frame at the 90th percentile, set from these measures: 906 ms today, on battery, against Ghostty's first window, 837 ms, which understates its first frame.
