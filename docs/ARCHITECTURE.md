# Architecture

This document describes Lokyr as a whole. Each subsystem has its own document with the details, and the reasons behind each choice are in the [ADRs](adr/).

## Principles

1. **Mechanism in the core, policy in plugins.** The kernel provides data models and fast services. Plugins decide *what* is shown and *how* users interact with it ([ADR 0002](adr/0002-micro-kernel-and-plugins.md)).
2. **One API for every plugin.** First-party plugins use the same interfaces as third-party ones, through a Rust front end instead of a Luau one. If one of our own features cannot be written with the public SDK, the SDK is what's missing something ([ADR 0003](adr/0003-native-and-wasm-plugins.md), [ADR 0012](adr/0012-luau-plugins.md)).
3. **No wasted work.** When nothing changes there are no frames, timers or wakeups. Anything that is not visible or not installed costs nothing, and that includes the Luau runtime.
4. **Native where it matters, our own where we need control.** Windows, menus, IME and accessibility are native on each OS. The chrome and the rendering are ours, and identical everywhere ([ADR 0004](adr/0004-native-shell-and-gpu-chrome.md)).
5. **No networking in the app.** Downloading and installing is Sleip's job ([ADR 0008](adr/0008-separate-package-manager.md)). The MCP server speaks over stdio, through the IPC socket ([ADR 0017](adr/0017-mcp-and-agents.md)).
6. **Budgets are requirements.** Timings and memory have numeric targets that CI checks ([PERFORMANCE.md](PERFORMANCE.md)).

## Layers

```
                ┌──────────── sleip (separate CLI) ────────────┐
                │ resolve · download · verify · install · publish │◀──▶ online registry
                └───────────────────────┬─────────────────────────┘
                                        │ writes to disk: store + plugins.lock
┌───────────────────────────────────────▼────────── lokyr (app) ────────────────────────────┐
│ First-party plugins (static)            │ Third-party plugins (Luau, sandboxed)          │
│ tabs · splits · search · themes ·       │ read from disk, activated only when needed     │
│ theme-iterm2 · core-actions · palette   │                                                │
├─────────────────────────────────────────┴────────────────────────────────────────────────┤
│ plugin-sdk ── HostApi (kernel, native)  │  plugin-host-luau (Luau VMs → same HostApi)   │
├──────────────────────────────────────────────────────────────────────────────────────────┤
│ KERNEL: workspace + layout tree · commands · keymap · event bus · config                 │
│         services: search · decorations · triggers                                        │
├─────────────────────────┬──────────────────────────────┬─────────────────────────────────┤
│ TERMINAL                │ UI ENGINE                    │ PER-OS BACKENDS (behind traits) │
│ vt-core · pty ·         │ ui-core (view tree, taffy,   │ platform: macos | wayland | x11 │
│ session · search        │ AccessKit) · text-core       │ render: metal | gl              │
│                         │ · render-core                │ font: coretext | freetype       │
└─────────────────────────┴──────────────────────────────┴─────────────────────────────────┘
```

| Layer | Crates | Responsibilities |
|---|---|---|
| Terminal | `vt-core`, `pty`, `session`, `search` | Parsing, state and grid; child processes; per-pane I/O; search engine. No UI |
| Kernel | `kernel` | Data model, commands, keymap, events, config, services, native plugin host, window composition |
| UI engine | `ui-core`, `text-core`, `render-core` | View tree, layout, widgets, text, display list. Platform-independent |
| Per-OS backends | `platform-*`, `render-*`, `font-*` | Windows and events, GPU, fonts. One crate per OS and engine, behind the traits of the `*-core` crates |
| Plugins | `plugin-sdk`, `plugin-host-luau`, `plugins/*` | API and Rust SDK, Luau host, first-party plugins |
| Packages | `pkg-model`, `sleip/*`, `registry/*` | Shared formats, package manager, registry |

## Kernel data model

```
Workspace
└── Window (id, screen, size, appearance)
    └── Tab (id, title, color, active pane)
        └── LayoutTree
            ├── Split { direction, ratio, a, b }
            └── Leaf(PaneId)
                └── Pane (id, profile, cwd) ──▶ Session (pty + process + VT state)
```

- **Where it is**: `kernel::workspace`. A `Workspace` holds the windows, the tabs and the panes, each pane with its session; a tab's layout is a tree whose leaves are panes, a single leaf until splits come. The app keeps, for each window of the platform, what is the platform's and the drawing's (its surface, the input method's composition, the cursor's blinking, the mouse), and the ids of its window and pane in the workspace. The sessions' events, the titles, which the tabs of their active panes take, the bells, the writes to the clipboards, the ends of the output and the terminals' others, come in on the main thread, as `Workspace::collect` gathers them once a session woke it, and wait as the workspace's `Event`s until the app takes them.
- **Serializable**: the whole model can be serialized. Session restore, IPC, tmux -CC and the E2E tests all use the same model. Its types are plain data, apart from the sessions; they take serde's derives with the first that serializes them, IPC in F3b.
- **Changes go through commands**: every change is a kernel command, and each one emits an event (`tab-created`, `pane-focused`, `layout-changed`, …). Plugins never mutate state directly.
- **Identifiers** are opaque `u64` values, never reused during the life of the process: windows, tabs and panes take theirs from one counter of the process.
- **Diagnostics**: `kernel::diagnostics`. What goes wrong that the user is to learn of, a config that does not read, a font that is not installed, a plugin that is disabled or a command that fails, is a diagnostic of a code, whose hundreds tell its category ([ADR 0019](adr/0019-commands-errors-and-permissions.md)), and of a text. The sink writes each to the log, and keeps those that the user can act on, the config's and the font's, the failures and the faults, for the notice bar across the top of the windows, the latest first, until a fix or a click takes it away; later the audit log and MCP take them too.
- **Mechanism vs. policy for splits**: the kernel provides the tree operations (split, resize, zoom). The `splits` plugin decides **when and how** to use them: commands, dividers, mouse interaction. Without that plugin every tab has exactly one pane.

## Threads and data flow

| Thread | Does | Never does |
|---|---|---|
| Main (OS event loop) | Input, kernel, native plugins, UI layout, snapshot, shaping, frame submission | Block on I/O or on a Luau plugin |
| Reader, one per pane | `read` from the PTY, into a ring of four buffers of 64 KiB, ahead of the parse | Parse, or wait for the main thread |
| I/O, one per pane | Parsing the reader's batches, grid updates; ending synchronized updates at their deadline | Wait for the main thread, or for the program to read its input |
| Writer, one per pane | Writing input and replies to the PTY | Wait for the main thread |
| Glyphs, three beside the main thread, only while a frame draws glyphs that the atlas lacks | Rasterizing a frame's new glyphs with CoreText, 2 ms of them at most | Outlive the frame, or touch the atlas |
| Drawing warm-up, once, as the app launches | Making the GPU's pipelines and the font, and letting them go, while AppKit launches, so that the app's own take nearly nothing | Outlive the launch |
| Font warm-up, once, after the first frame | Readying the system's font fallback and its CJK and emoji faces, about 20 ms | Hold anything of the app's; a frame's first lookup of a CJK face, within those 20 ms, can wait for CoreText's cascade, which the thread fills |
| Plugin executor | Calls into third-party plugins, with a deadline per call; its watchdog runs only while a plugin executes | Sit on the output path |
| Workers | Search, reflow of very large scrollbacks | Hold the grid lock for long |

**From program output to the screen**
1. The reader thread reads the PTY into a batch of at most 64 KiB, while the I/O thread parses the one before it, so that the program never waits for the parse. A macOS terminal queues about 1 KiB of output, which a program that writes more fills again within microseconds of each read: a reader that parsed between its reads kept the program waiting through each parse, and waited itself through each refill, a KiB for two wakeups. Once a batch holds a full queue, the reader bridges the refills, with up to 16 reads that do not wait, then waits of 1 ms while the I/O thread has a batch to parse, for about 3 ms in all, so that a batch holds many refills; less than a full queue, as a shell's echo, goes to the I/O thread at once, and so does what the reader holds as soon as the I/O thread runs out of batches. Past four batches, the reader stops, and the terminal's queue holds the program back.
2. The I/O thread parses a batch while holding the pane's grid lock, then releases the lock fairly so the renderer never starves.
3. It marks the rows that changed and, unless it already has, wakes the main thread (a `CFRunLoopSource` on macOS, an `eventfd` on Linux), marking its pane as woken: the main thread looks at the woken panes alone, and asks for their windows' frames alone. A bell or a terminal's event wakes it too, once a frame's time at most: the main thread takes what a pane told no more often, and a timer brings the turn of one that told within it, so that a stream of bells wakes it 60 times a second, not 100 000.
4. On the display-link tick, the main thread takes the lock, copies only the visible rows that changed (8-byte cells plus the styles they use) and releases it. A frame whose display list is the one that the window shows already presents nothing. The copy carries the program's input modes, its keyboard's flags and the alternate screen, which the frame reads from it, and a key's contexts too, so that a frame takes a pane's lock once, and a key once more to write; a paste, the mouse and a selection that the app just changed read the terminal under the lock.
5. Shaping, display-list construction and GPU command encoding then run without the lock. The grid's layout collects the glyphs that the atlas lacks rather than drawing each as it comes: CoreText draws them together, on the main thread and three others, for 2 ms at most, and the layout is done again with them. The faces of characters that no frame met before, which the system's fallback gives, are looked up while the frame has spent less than 2 ms on them: a lookup that started goes on, and the system's fallback can take about 22 ms for each of the icons of a Nerd Font's private planes that the font lacks. What is left, of a screen of hundreds of new glyphs, shows as empty cells, and the frames after, which the frame asks for, draw it: a page of new Chinese shows whole in about three frames, where the first frame of it took about 25 ms, and a 4K screen of all-new glyphs in about 0.2 s, where it took 300. After the first frame, a thread readies the fallback, whose first looks take over a millisecond each, and the CJK and emoji faces: up to about 4 MB of the footprint, most of which a shell's prompt of symbols that the font lacks takes anyway, 1.1 MB more as measured.
6. If the pane has synchronized output (mode 2026) enabled, the frame waits for the update to finish or for a timeout.

**From a key press to the program**
1. The native event reaches the main thread. The IME gets it first (composition text, candidates).
2. The keymap resolves the key combination in the current context (`when`). If a binding matches, the command runs. Otherwise:
3. The `vt-core` encoder translates the key according to the pane's modes (legacy xterm or kitty keyboard) and queues it for the pane's writer thread, which writes it to the PTY.
4. If the terminal was idle, the next frame is drawn immediately instead of waiting for the display link. This is the lowest-latency path.

## Frame pacing

**macOS**
- **Pacing source**: `NSView.displayLink` (macOS 14+), registered in the *common run-loop modes* so it keeps running during live resize and while menus are open. `CVDisplayLink` is deprecated.
- **Idle**: the display link runs only when there is something to draw, and stops when idle (zero wakeups).
- **Live resize**: `presentsWithTransaction = true` plus synchronous drawing, so the content follows the window edge without tearing; once a refresh, as a second draw within one would wait a refresh for a drawable, which the display link's next frame draws instead.
- **Presentation**: the Metal layers present without the display's sync (`displaySyncEnabled = false`), so that a frame goes to the window server as soon as it is drawn, which composes it at its next refresh, where it waited for the layer's own refresh first: a key's echo shows about 20 ms sooner, at 60 Hz as at 120. The window server's composition keeps each frame whole, in a window and in full screen.
- **No partial present**: CAMetalLayer redraws the whole drawable every frame. Dirty-row tracking therefore saves CPU time (snapshot, shaping), not GPU time. GPU cost stays low thanks to instanced rendering.
- **Drawables**: 2 per window (`maximumDrawableCount = 2`), BGRA8, `framebufferOnly = true`, which keep no frame waiting at 60 Hz nor at 120 Hz; 3 with the build's feature `three-drawables` ([ADR 0022](adr/0022-two-drawables.md)); a hidden window keeps the one that shows its last frame.
- **The layers**: the cursor's cell goes into a layer of its own over the window's surface, which the window server blinks and fades without a frame of the app's ([ADR 0021](adr/0021-drawables-and-the-cursor-layer.md)); the text that blinks, into another, under the cursor's, over the rows that hold it, which the window server switches as the grid draws that text hidden ([ADR 0023](adr/0023-blinking-text-layer.md)).
- **Translucency**: the frame's colors are premultiplied by their opacity, and the cells of the default background are clear. Below an opacity of 1 `[window] background-opacity` the window, its content view and the view's layer turn non-opaque, and the window's own background takes the terminal's default background with that opacity, under its whole shape, the titlebar and the rounded corners too, while the frame clears to nothing over it and the titlebar turns transparent, without its separator; the blur behind the window takes `CGSSetWindowBackgroundBlurRadius`, a private call of the window server, the only one of the app, as macOS has no public one. The app ships with a Developer ID, outside the App Store, which private calls would keep it from.

**Linux** (F7): the same scheme, using Wayland frame callbacks (`wl_surface.frame`) or the Present extension on X11, and `wp_presentation` for measurement.

## Workspace layout

```
lokyr/
├── Cargo.toml · justfile · deny.toml · rust-toolchain.toml · .github/workflows/
├── crates/
│   ├── vt-core/           parser, state, paged grid, scrollback, reflow, input encoding (pure)
│   ├── pty/               fork+exec, resize; no RawFd in the API (ConPTY later)
│   ├── session/           pty + vt-core + I/O thread, snapshots for the renderer
│   ├── search/            literal and regex search over the grid
│   ├── kernel/            modules: workspace and layout tree, commands, keymap, events, config, texts (l10n), services; implements HostApi
│   ├── ui-core/           view tree, layout (taffy), widgets (IME text field, lists, menus), hit testing, AccessKit, display list
│   ├── text-core/         FontBackend trait (discovery, fallback, shaping, rasterization), shaping cache, glyph atlas, box-drawing sprites, UCD tables
│   ├── render-core/       Renderer trait, display-list types, shaders
│   ├── platform-core/     traits: app, windows, quick windows, global keys, events, IME, clipboard, menus, notifications, appearance
│   ├── platform-macos/    AppKit via objc2 (NSWindow, NSPanel, NSView + CAMetalLayer, NSTextInputClient, NSMenu), Carbon's hot keys
│   ├── platform-wayland/  (F7) smithay-client-toolkit + calloop, our own CSD
│   ├── platform-x11/      (F7) x11rb + xkbcommon + xim
│   ├── render-metal/      objc2-metal
│   ├── render-gl/         (F7) glow + khronos-egl (GL 3.3 / GLES 3)
│   ├── font-coretext/     objc2-core-text (discovery, fallback, shaping, rasterization)
│   ├── font-freetype/     (F7) fontconfig + freetype-rs + harfrust
│   ├── plugin-sdk/        the plugin API (HostApi and its types) and its Rust front end; no internal dependencies
│   ├── plugin-host-luau/  Luau host (mlua): one VM per plugin, the Luau front end, capabilities, limits (behind a feature)
│   ├── pkg-model/         plugin.toml, plugins.toml/.lock, index format, dist (shared by app, sleip, registry)
│   └── app/               the `lokyr` binary: kernel + backends for the target + distribution
├── plugins/               first-party: depend ONLY on plugin-sdk
│   ├── core-actions/ links/ tabs/ splits/ search/ themes/ theme-iterm2/ quick-terminal/
│   ├── keymap-macos/ command-palette/ plugin-manager/ keymap-linux/ (F7)
│   └── (F5) shell-integration/ blocks/ tmux-cc/ triggers/ images/ …
├── dist/                  default.toml, minimal.toml
├── sleip/              cli/ (`sleip` binary) · resolver/ (pubgrub) · index/ (sparse index, verification)
├── registry/              registry-server (v1) + index specification
├── templates/             templates for `sleip new` (Luau)
├── tools/                 replay/ (`lokyr-replay`, `lokyr-record`) · differential/ (the VT core against `alacritty_terminal`) · bench/ · xtask/ (bundle, signing, gen-dist, UCD tables, terminfo)
├── fuzz/                  cargo-fuzz: parser, terminal, differential (against `alacritty_terminal`); later .itermcolors, manifest, archives
└── docs/                  this documentation (later an mdBook)
```

### Crate dependencies

```
app ─┬─▶ kernel ─┬─▶ session ─┬─▶ vt-core
     │           │            └─▶ pty
     │           ├─▶ search ───▶ vt-core
     │           ├─▶ ui-core ──┬─▶ text-core
     │           │             └─▶ render-core
     │           ├─▶ plugin-sdk                      (the kernel implements HostApi)
     │           └─▶ pkg-model
     ├─▶ platform-{macos|wayland|x11} ─▶ platform-core
     ├─▶ render-{metal|gl} ─▶ render-core
     ├─▶ font-{coretext|freetype} ─▶ text-core
     ├─▶ plugin-host-luau ─▶ plugin-sdk              (feature `luau-plugins`)
     └─▶ plugins/* ─▶ plugin-sdk                     (selected by dist/*.toml)

sleip/* ─▶ pkg-model           registry/* ─▶ pkg-model
```

### Dependency rules

| Rule | Why | Enforced by |
|---|---|---|
| `vt-core` and `search` do not depend on UI, platform or plugins | Headless tests, fuzzing, reuse | `xtask check-deps` |
| `plugin-sdk` depends on no other workspace crate | The SDK must be publishable and usable on its own | `xtask check-deps` |
| Only `app` depends on `kernel` | Plugins must not be able to bypass the SDK | `cargo-deny` (`wrappers`) |
| `plugins/*` depend only on `plugin-sdk`, plus a whitelist of utility crates | Proves the public API is sufficient | `xtask check-deps` |
| First-party plugins use `#![forbid(unsafe_code)]`; clippy `disallowed-methods` forbids threads, blocking I/O and clocks | They follow the same rules as Luau plugins | clippy in CI |
| Luau (`mlua`) only in `plugin-host-luau` | The C++ runtime stays behind one crate and one feature | `cargo-deny` (`wrappers`) |
| HTTP/TLS (`hyper`, `reqwest`, `rustls`, …) only in `sleip/*` | No networking code in the app | `cargo-deny` (`bans` + `wrappers`) |
| No async runtime (`tokio`, `async-std`, …) in the app | Fewer threads, less memory, fewer dependencies | `cargo-deny` |
| Only `app` picks the backends, via `cfg(target_os)` | Core crates stay portable | review and `xtask check-deps` |
| Platform-independent crates (`vt-core`, `pty`, `session`, `search`, `kernel`, `*-core`) build and pass their tests on Linux, and expose no macOS types | Keeps Linux possible after 1.0 ([ADR 0011](adr/0011-macos-only-1-0.md)) | Linux CI job |

The platform, render and font traits are deliberately thin. They describe what the macOS backend needs rather than a lowest common denominator, and the Linux backends will extend them in F7.

## Main libraries

Versions and maturity must be rechecked when coding starts.

| Subsystem | Crates | Notes |
|---|---|---|
| macOS | `objc2`, `objc2-app-kit`, `objc2-metal`, `objc2-quartz-core`, `objc2-core-text`, `objc2-user-notifications` | The `metal` crate is deprecated. objc2 changes often, and AccessKit's macOS adapter can trail it: it then builds on its own versions of `objc2`, in one module of `platform-macos` that hands it the view as a raw pointer, until it catches up |
| Wayland / X11 (F7) | `smithay-client-toolkit` + `calloop`; `x11rb` + `xkbcommon` + `xim` | |
| GPU on Linux (F7) | `glow` + `khronos-egl` | |
| Text | CoreText on macOS (discovery, fallback, shaping, rasterization); `harfrust` + `fontconfig` + `freetype-rs` on Linux (F7) | HarfRust is young: benchmark it against `harfbuzz-sys` before adopting it |
| Core | `memchr`, `regex-automata`, `lz4_flex`, `unicode-normalization`. Scans like the parser's fast path use arithmetic on 64-bit words (SWAR) in safe code; `core::arch` only where a measurement shows it pays (`std::simd` is not stable) | `unicode-normalization` may trail the pinned Unicode version: 17 while the tables are 18 |
| UI | `taffy`, `accesskit` | |
| Formats and config | `plist`, `toml_edit` (errors with line and column), `serde`, `ciborium` (CBOR), `notify` | |
| Plugins | `mlua` with the `luau` and `luau-jit` features, from vendored sources | Luau ships every week or two; pin it and upgrade deliberately |
| Sleip | `pubgrub`, `reqwest` + `rustls`, `ed25519-dalek`, `zstd`, `tar` | ed25519 signatures before Sigstore |
| Tests and benchmarks | `insta`, `proptest`, `cargo-fuzz`, `criterion`, `gungraun` (formerly iai-callgrind) | |

## Conventions

- **Toolchain**: stable Rust, pinned in `rust-toolchain.toml`; edition 2024. Targets:
  - `aarch64-apple-darwin` only, with macOS 26 as the deployment target ([ADR 0013](adr/0013-platform-baseline.md));
  - `x86_64-unknown-linux-gnu`, for the CI job that builds the platform-independent crates.
- **Fuzzing** alone uses nightly Rust, which cargo-fuzz needs. That toolchain is pinned too, in the `justfile`, and `fuzz/` is a workspace of its own so that the main one never needs nightly.
- **Logging and errors**: `tracing` for logs and spans (with signposts on macOS); `thiserror` in libraries, `anyhow` only in binaries.
- **Child processes** mark the descriptors that they inherit closed on exec, as the `pty` crate's do: on macOS, a child that does not can hold the pipe of a spawn made at the same moment until its program ends, and that spawn waits as long ([VT-CORE.md](VT-CORE.md#pty-and-processes-pty)). A program in a terminal starts through the `pty` crate; another child marks its descriptors between fork and exec, with `std::process::Command`'s `pre_exec`, which makes `std` fork and exec rather than spawn, or, on macOS, through a direct call of `posix_spawn` with Apple's `POSIX_SPAWN_CLOEXEC_DEFAULT`, which `std::process::Command` does not set. A dependency that spawns processes, or C code that calls `popen` or `system`, is looked at for the same before it comes in.
- **Workspace config**: `[workspace.dependencies]` and `[workspace.lints]` are centralized; clippy runs with `-D warnings`.
- **`unsafe`**: allowed only in the platform, render, font and `pty` crates, in `plugin-host-luau`, where the watchdog and the bulk readers use Lua's C API, in `tools/bench`, which reads other processes' statistics from the kernel, and in the test allocators that count allocations, outside library code. Always with a `// SAFETY:` comment. Every other crate uses `#![forbid(unsafe_code)]`; `plugin-host-luau` and `tools/bench` use `#![deny(unsafe_code)]` and allow their call sites one by one.
- **Comments**: code comments and doc comments describe the code itself. They never reference the plan (roadmap phases, task IDs, ADR numbers, sections of these docs). Links to external specifications such as xterm, kitty or Unicode are fine.
- **Common tasks** live in the `justfile`: `just ci` runs every check that CI runs, and `just --list` shows the other recipes.
- **Language**: all documentation, comments and commit messages are in English.
- **License**: MIT ([ADR 0010](adr/0010-mit-license.md)).

## On-disk paths

| What | macOS | Linux (F7) |
|---|---|---|
| Config (`config.toml`, `plugins.toml`, `plugins.lock`, `permissions.toml`, `themes/`) | `~/.config/lokyr/` (or `$XDG_CONFIG_HOME/lokyr/` if set) | `$XDG_CONFIG_HOME/lokyr/` |
| Installed packages | `~/Library/Application Support/lokyr/store/` | `$XDG_DATA_HOME/lokyr/store/` |
| Caches (fonts, compiled plugins) | `~/Library/Caches/lokyr/` | `$XDG_CACHE_HOME/lokyr/` |
| State (the last window's directory; later, saved sessions and search history) | `~/Library/Application Support/lokyr/state/` | `$XDG_STATE_HOME/lokyr/` |
| IPC socket | `$TMPDIR/lokyr-<uid>/` | `$XDG_RUNTIME_DIR/lokyr/` |

On macOS too the config lives in `~/.config`, so it can be versioned with dotfiles, as Ghostty, Alacritty and kitty do.
