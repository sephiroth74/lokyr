# Performance and memory

## Principles

- **Budgets are requirements**: every metric has a numeric target, and a regression fails CI just like a failing test.
- **Measure first**: no optimization without a benchmark that justifies it and then protects it.
- **Idle means zero**: no frames, timers or wakeups when nothing changes.
- **What you can't see costs nothing**:
  - hidden panes are not drawn;
  - unused plugins are not loaded;
  - occluded windows draw nothing, and keep one drawable alone, the one that shows their last frame.
- **Memory is accounted per category**: heap, GPU and mappings are tracked separately, because a single total hides problems.

## Budgets

Figures are for Apple Silicon (M-series), release builds. They are targets to confirm with the F2 prototype. If one turns out to be unrealistic, changing it is an explicit decision, never a silent one. The [F0 baseline](bench/baseline-F0.md) of existing terminals revised the startup, footprint and idle budgets.

| Metric | Target | How it is measured |
|---|---|---|
| First presented frame | ≤ 200 ms warm; ≤ 1 s cold, at the 90th percentile, after `sudo purge`, as [F2-53](bench/startup-F2-53.md) set it | The process's start → `addPresentedHandler`, with `lokyr-bench startup --first-frame`; for other terminals, `lokyr-bench startup`, which times the first window on screen; `wp_presentation` on Linux |
| Key → presented frame, at 120 Hz | p50 ≤ 2 frames, p99 ≤ 3; one extra frame allowed next to a pane flooding output | `NSEvent.timestamp` → presentation time |
| Idle | 0 package idle wakeups/s and ≤ 0.5 interrupt wakeups/s, once the cursor stops blinking (15 s after the last key or output, by default); while it blinks, ≤ 2/s at the default interval, faded or not, and with blinking text, as the window server blinks the cursor's layer and the blinking text's without the app ([ADR 0021](adr/0021-drawables-and-the-cursor-layer.md), [ADR 0023](adr/0023-blinking-text-layer.md)), which F2-55 and F2-125 measure | `lokyr-bench idle`: package idle wakeups, which `top -stats idlew` shows, and interrupt wakeups; `powermetrics`; `voluntary_ctxt_switches` deltas on Linux |
| Footprint excluding GPU memory | ≤ 35 MB with 1 window and 1 pane; ≤ 1 MB per additional pane; ≤ 2 MB per active Luau plugin; back to baseline ±2 MB after closing tabs | `lokyr-bench memory`: `phys_footprint` minus the GPU categories of `footprint`; on Linux PSS and heaptrack, never RSS |
| Scrollback | ≤ configured limit + 5%; ≈ 8 B per cell | memory tests in CI |
| GPU memory (accounted separately) | 2 BGRA8 `framebufferOnly` drawables per window (≈ 31 MB each at 3456×2234), three with the build's feature `three-drawables` ([ADR 0022](adr/0022-two-drawables.md)), none per pane, of which a hidden window keeps one, the one that shows its last frame; Metal's own memory while frames are drawn, about 85 to 95 MB on an M5 Pro, whatever the window's size, which the driver keeps and the system can take back about two seconds after the last frame, is any Metal app's, not counted against the budget; the glyph atlas, shared by every window, at most 32 MB: a grayscale texture up to 4096² and a color one up to 2048² | The GPU categories of `lokyr-bench memory`; Instruments (Metal System Trace); DRM `fdinfo` on Linux |
| Throughput | Within 10% of the faster of Ghostty and Alacritty, but in vtebench's bottom region, with its lines of one character ([ADR 0015](adr/0015-vt-core-throughput-exception.md)) | vtebench; `cat` of a 1 GB file, headless and on screen |
| CPU per frame | ≤ 2 ms p99 (4K, 4 panes) | `tracing` spans + signposts |
| Luau plugin activation | ≤ 5 ms, compilation included | `tracing` spans |

## How to measure

**Startup**
- **Lokyr**: `lokyr --bench startup` records when the process started (`proc_pidinfo`) and when the drawable's first `addPresentedHandler` fires. It then exits and prints JSON. `lokyr-bench startup --app target/Lokyr.app --first-frame` runs it 10 times after 2 warmup runs, and summarizes it as below.
- **Any app**: `lokyr-bench startup --app <path> [--runs <n>] [--warmup <n>]`:
  - launches the app through LaunchServices, as the Dock does, 10 times after 2 warmup runs;
  - times its first window on screen from the process start, by polling the window server. This approximates the first presented frame, and works for any terminal;
  - prints the median, the 90th percentile and the spread: the range from the 10th to the 90th percentile, as a fraction of the median;
  - needs the app not to be running, and the app takes the focus at every launch.
- **Cold**: after `sudo purge` on macOS, or after dropping the page cache on Linux. `lokyr-bench startup --cold --warmup 0` runs `sudo purge` before each launch, after a `sudo -v`. Cold, a window shows before its first frame, as the font and the shaders come from the disk: the first window understates the startup.

**Latency**
- **In the app**: for every key press, `NSEvent.timestamp` is compared with the presentation time (`MTLDrawable.presentedTime`). A p50/p99 histogram is exposed over IPC.
- **External check**: an occasional run with Typometer or a high-speed camera calibrates the internal measurement.

**Idle**
- macOS:
  - `lokyr-bench idle --pid <pid> [--interval <ms>] [--duration <s>]` reads the wakeup counters of `proc_pid_rusage` every second, over 60 s by default.
  - The budget limits two counters: package idle wakeups, which bring the CPU out of idle and which `top -stats idlew` shows, and interrupt wakeups, which count every wakeup caused by an interrupt, such as a timer, whether the CPU was idle or not.
  - `powermetrics --samplers tasks` gives the picture of the whole system.
- Linux: the change in `voluntary_ctxt_switches` in `/proc/<pid>/status` over 60 s.

**Memory**
- **macOS**:
  - `lokyr-bench memory --pid <pid> [--interval <ms>] [--duration <s>]` samples `phys_footprint` through `proc_pid_rusage`, and subtracts the GPU memory that `footprint -j` reports by category: IOSurface, IOAccelerator, and the graphics ledger (`… (graphics)`), which includes GPU memory the process owns without mapping it. It works on any process of the same user, so it measures other terminals too.
  - `footprint <pid>` and `vmmap -summary <pid>` give the details.
  - In the app, `task_info(TASK_VM_INFO)` → `phys_footprint`, shown in a debug overlay and over IPC.
- **Linux**:
  - PSS from `/proc/<pid>/smaps_rollup`. Never RSS, which counts shared pages more than once.
  - heaptrack for allocations.
  - DRM `fdinfo` (`drm-memory-*`) for the GPU.
- **GPU on macOS**: Instruments, Metal System Trace.

**Throughput**
- Alacritty's vtebench, with the standard scenarios.
- `lokyr-replay --bench <file>`: parsing only, headless.
- On screen:
  - `cat` of a 1 GB file;
  - `seq 1 10000000`;
  - dense colored (SGR) output;
  - CJK text and emoji.

**CPU per frame**: `tracing` spans and `os_signpost` (Instruments *Points of Interest*) for each phase: snapshot, shaping, display list, encoding, present.

## Benchmarks and CI

| Level | Tool | Where | Threshold |
|---|---|---|---|
| Micro: parser per scenario, cell writes, style dedup, reflow, search over 100k and 1M lines, shaping cache | `criterion` | locally and on the dedicated Mac | report |
| Instruction counts of vtebench's scenarios and of text as `cat` prints it, against those of the commit that a push or a pull request starts from | `gungraun` (callgrind), `just instructions` | Linux CI (valgrind does not run on Apple Silicon) | +3% blocks |
| End-to-end wall-clock times | `tools/bench` harness | dedicated Mac (self-hosted runner), nightly | +10% opens an issue |
| Memory: opening and closing tabs, full scrollback | integration tests | macOS and Linux CI | the budgets above |
| Binary size | `cargo bloat` | CI | a budget per release, with and without the Luau runtime |

Results history is published (e.g. on bencher.dev, or as JSON in a branch of the repo), so trends are visible and not just jumps.

## Techniques and the budgets they serve

| Technique | Budget |
|---|---|
| 8-byte cells, deduplicated styles, pages recycled from a pool | Memory, throughput |
| Fast path for printable ASCII, written straight into the cells | Throughput |
| Unicode properties in one table lookup | Throughput |
| Grid lock taken per slice and released fairly | Latency under load |
| Copying only the visible rows that changed | CPU per frame |
| One GPU surface per window, and the cursor's layer, and the blinking text's while it shows; 2 drawables; one kept when occluded | GPU memory |
| Display link running only when there is work; immediate draw after idle | Idle, latency |
| Per-run shaping cache, shared glyph atlas | CPU per frame, memory |
| Luau plugins loaded only when needed; no VM before the first one | Startup, memory |
| A plugin watchdog that steps in only at the deadline, and runs only while a plugin executes | Idle, CPU per frame |
| No async runtime in the app | Memory, startup |
| System allocator; mimalloc only if measurements justify it | Memory |

## Comparing with other terminals

To compare Lokyr fairly with Ghostty, Alacritty, kitty, WezTerm and iTerm2:
- same machine, on AC power;
- same font, same font size, same window size in pixels;
- release builds, with ligatures and blur disabled everywhere;
- same shell, without a complex prompt;
- multiple runs, reporting median and spread;
- terminal versions recorded alongside the results.

[F2-134](tasks/F2-macos-single-pane.md#f2-134--lokyr-against-the-other-terminals) takes these conditions, and more, with the tools of each measure.
