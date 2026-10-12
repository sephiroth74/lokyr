# Performance and memory

## Principles

- **Budgets are requirements**: every metric has a numeric target, and a regression fails CI just like a failing test.
- **Measure first**: no optimization without a benchmark that justifies it and then protects it.
- **Idle means zero**: no frames, timers or wakeups when nothing changes.
- **What you can't see costs nothing**:
  - hidden panes are not drawn;
  - unused plugins are not loaded;
  - occluded windows release their drawables.
- **Memory is accounted per category**: heap, GPU and mappings are tracked separately, because a single total hides problems.

## Budgets

Figures are for Apple Silicon (M-series), release builds. They are targets to confirm with the F2 prototype. If one turns out to be unrealistic, changing it is an explicit decision, never a silent one.

| Metric | Target | How it is measured |
|---|---|---|
| First presented frame | ≤ 100 ms warm, ≤ 250 ms cold | Launch mach time → `addPresentedHandler`; `wp_presentation` on Linux |
| Key → presented frame, at 120 Hz | p50 ≤ 2 frames, p99 ≤ 3; one extra frame allowed next to a pane flooding output | `NSEvent.timestamp` → presentation time |
| Idle | 0 wakeups/s; ≤ 2/s with a blinking cursor | `top -stats idlew`, `powermetrics`; `voluntary_ctxt_switches` deltas on Linux |
| Footprint excluding GPU memory | ≤ 30 MB with 1 window and 1 pane; ≤ 1 MB per additional pane; ≤ 2 MB per active Luau plugin; back to baseline ±2 MB after closing tabs | `saetta-bench memory`: `phys_footprint` minus the GPU categories of `footprint`; on Linux PSS and heaptrack, never RSS |
| Scrollback | ≤ configured limit + 5%; ≈ 8 B per cell | memory tests in CI |
| GPU memory (accounted separately) | 2 BGRA8 `framebufferOnly` drawables per window (≈ 31 MB each at 3456×2234), none per pane, released when the window is hidden | The GPU categories of `saetta-bench memory`; Instruments (Metal System Trace); DRM `fdinfo` on Linux |
| Throughput | Within 10% of the faster of Ghostty and Alacritty | vtebench; `cat` of a 1 GB file, headless and on screen |
| CPU per frame | ≤ 2 ms p99 (4K, 4 panes) | `tracing` spans + signposts |
| Luau plugin activation | ≤ 5 ms, compilation included | `tracing` spans |

## How to measure

**Startup**
- `saetta --bench startup` records when the process started (`proc_pidinfo`) and when the drawable's first `addPresentedHandler` fires. It then exits and prints JSON.
- Warm: `hyperfine`, with warmup runs.
- Cold: after `sudo purge` on macOS, or after dropping the page cache on Linux.

**Latency**
- **In the app**: for every key press, `NSEvent.timestamp` is compared with the presentation time (`MTLDrawable.presentedTime`). A p50/p99 histogram is exposed over IPC.
- **External check**: an occasional run with Typometer or a high-speed camera calibrates the internal measurement.

**Idle**
- macOS: `top -l 2 -stats pid,command,idlew -pid <pid>` and `powermetrics --samplers tasks`.
- Linux: the change in `voluntary_ctxt_switches` in `/proc/<pid>/status` over 60 s.

**Memory**
- **macOS**:
  - `saetta-bench memory --pid <pid> [--interval <ms>] [--duration <s>]` samples `phys_footprint` through `proc_pid_rusage`, and subtracts the GPU memory that `footprint -j` reports by category: IOSurface, IOAccelerator, and the graphics ledger (`… (graphics)`), which includes GPU memory the process owns without mapping it. It works on any process of the same user, so it measures other terminals too.
  - `footprint <pid>` and `vmmap -summary <pid>` give the details.
  - In the app, `task_info(TASK_VM_INFO)` → `phys_footprint`, shown in a debug overlay and over IPC.
- **Linux**:
  - PSS from `/proc/<pid>/smaps_rollup`. Never RSS, which counts shared pages more than once.
  - heaptrack for allocations.
  - DRM `fdinfo` (`drm-memory-*`) for the GPU.
- **GPU on macOS**: Instruments, Metal System Trace.

**Throughput**
- Alacritty's vtebench, with the standard scenarios.
- `saetta-replay --bench <file>`: parsing only, headless.
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
| Instruction counts for the same scenarios | `gungraun` (callgrind) | Linux CI (valgrind does not run on Apple Silicon) | +3% blocks |
| End-to-end wall-clock times | `tools/bench` harness | dedicated Mac (self-hosted runner), nightly | +10% opens an issue |
| Memory: opening and closing tabs, full scrollback | integration tests | macOS and Linux CI | the budgets above |
| Binary size | `cargo bloat` | CI | a budget per release, with and without the Luau runtime |

Results history is published (e.g. on bencher.dev, or as JSON in a branch of the repo), so trends are visible and not just jumps.

## Techniques and the budgets they serve

| Technique | Budget |
|---|---|
| 8-byte cells, deduplicated styles, pages recycled from a pool | Memory, throughput |
| SIMD fast path for printable ASCII | Throughput |
| Grid lock taken per slice and released fairly | Latency under load |
| Copying only the visible rows that changed | CPU per frame |
| One GPU surface per window; 2 drawables; release when occluded | GPU memory |
| Display link running only when there is work; immediate draw after idle | Idle, latency |
| Per-run shaping cache, shared glyph atlas | CPU per frame, memory |
| Luau plugins loaded only when needed; no VM before the first one | Startup, memory |
| A plugin watchdog that steps in only at the deadline, and runs only while a plugin executes | Idle, CPU per frame |
| No async runtime in the app | Memory, startup |
| System allocator; mimalloc only if measurements justify it | Memory |

## Comparing with other terminals

To compare Saetta fairly with Ghostty, Alacritty, kitty, WezTerm and iTerm2:
- same machine, on AC power;
- same font, same font size, same window size in pixels;
- release builds, with ligatures and blur disabled everywhere;
- same shell, without a complex prompt;
- multiple runs, reporting median and spread;
- terminal versions recorded alongside the results.
