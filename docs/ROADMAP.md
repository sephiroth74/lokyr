# Roadmap

**Version 1.0 targets macOS only** ([ADR 0011](adr/0011-macos-only-1-0.md)). Linux comes after 1.0, in F7. To keep that door open, CI builds and tests the platform-independent crates on Linux from F0 onward.

Estimates are rough and assume one full-time developer experienced in Rust. The macOS MVP (F0–F3) takes about 6–9 months.

Each phase is broken into tasks of one day or less in [tasks/](tasks/README.md). A phase's breakdown is written near the end of the previous phase.

## Overview

| Phase | Contents | Exit criteria | Duration |
|---|---|---|---|
| F0 Foundations | Workspace, CI, dependency rules, ADRs, budget harness, notarized plugin-runtime spikes | CI green; ADRs approved; plugin runtime decided | 2–3 weeks |
| F1 Headless VT core | `vt-core`, `pty`, `session`, `search` engine, `saetta-replay`, fuzzing and differential tests | Compatibility and throughput measured (see below) | 8–12 weeks |
| F2 macOS, single pane | macOS platform, CoreText fonts, Metal rendering, minimal kernel, `core-actions` | Used as the main terminal for a week | 8–12 weeks |
| F3 **macOS MVP** | Full kernel, native SDK, `ui-core`, `tabs`, `splits`, `search` plugins, iTerm2 themes, palette | Every must-have requirement in place | 10–14 weeks |
| F4 Plugin ecosystem | Luau host, Luau API, Faretra, registry v0 | Publishing and installing work end to end | 8–10 weeks |
| F5a Shell integration and blocks | OSC 133, prompt jumping, Warp-style blocks, ssh integration | Verified in zsh, bash and fish, including over ssh | 4–6 weeks |
| F5b iTerm2 power features | tmux -CC, hotkey window, triggers, profiles, broadcast, IPC | tmux -CC workflows on par with iTerm2 | 8–12 weeks |
| F5c Graphics and kitty protocols | kitty graphics, sixel, iTerm2 images, hints and URLs | `kitten icat`, `imgcat` and `chafa` work | 6–8 weeks |
| F6 Release 1.0 | Hardening, docs, stable plugin API 1.0 | The 1.0 criteria (see [Versions](#versions)) | — |
| F7 After 1.0 | Linux, registry v1, Windows, mux daemon, lz4, custom shaders, AI | — | — |

## Phases in detail

### F0 — Foundations

- **Workspace**: a Cargo workspace with empty crates, with the dependency rules already enforced (`cargo-deny`, `xtask check-deps`).
- **CI**:
  - on macOS arm64: fmt, clippy, tests, deny, check-deps;
  - on Linux x86_64, for the platform-independent crates only: build and tests.
- **Tooling**: `justfile`, `rust-toolchain.toml`, workspace lints.
- **ADRs** reviewed and approved (the ones in [adr/](adr/)).
- **Budget harness** (`tools/bench`) for startup, idle and memory.
- **Notarized plugin-runtime spikes**:
  - A minimal signed and notarized app runs plugin-like work in wasmtime (Cranelift and Pulley), LuaJIT and Luau. The Lua runtimes also face hostile plugins.
  - Measure time and memory, then decide. The decision is [ADR 0012](adr/0012-luau-plugins.md): Luau with its native code generator.

**Exit**: CI green; ADRs approved; plugin runtime decided.

### F1 — Headless VT core

- **`vt-core`**: parser, state, paged grid, scrollback, reflow, Unicode, input encoding.
- **Other crates**: `pty`, `session` and the `search` engine.
- **`saetta-replay`** plus the test battery:
  - golden tests from recorded sessions;
  - esctest and vttest;
  - differential tests against `alacritty_terminal`;
  - fuzzing, reflow property tests, benchmarks.

**Exit**:
- ≥ 90% of the applicable esctest tests pass;
- vttest goldens;
- recorded sessions of vim, htop, tmux, less and fzf replay identically;
- no unexplained divergence from `alacritty_terminal`;
- 24 h of fuzzing without crashes;
- throughput ≥ `alacritty_terminal`;
- the same tests also pass on Linux CI.

### F2 — macOS, single pane

- **`platform-macos`**:
  - app and window, with the titlebar prototype;
  - input and IME (`NSTextInputClient`);
  - live resize, clipboard, menus;
  - an accessibility skeleton.
- **`font-coretext` and `text-core`**:
  - font discovery and fallback, and an optional separate font for non-ASCII text, like iTerm2's;
  - CoreText shaping with ligatures;
  - glyph atlas, box-drawing and powerline sprites.
- **`render-metal`**: instanced cells, cursor (block, bar and underline, blinking as DECSCUSR asks), selection, decorations.
- **Minimal kernel**: commands, keymap, config with hot reload, plus the `core-actions` plugin.
- **Terminal settings**: the reported `TERM`, the width tables and the width of ambiguous characters ([VT-CORE](VT-CORE.md#settings)).
- **Environment and bundle**: locale, login shell, TCC strings, signed bundle.

**Exit**: used as the main terminal for a week; Japanese IME works; budgets measured; zero wakeups when idle.

### F3 — macOS MVP

- **Full kernel**: workspace, layout tree, events, search and decoration services.
- **Plugin plumbing**: the `HostApi` draft, `plugin-sdk` with the Rust front end, `export_plugin!`, `xtask gen-dist`.
- **`ui-core`**: view tree, layout, IME text field, lists, menus, AccessKit.
- **Plugins**: `tabs`, `splits`, `search`, `themes`, `theme-iterm2`, `keymap-macos`, `command-palette`.
- **Basic OSC 7 and 133**: cwd inheritance for splits, and marks.
- **Cursor options**: blink interval, smooth blinking, default shape and smooth movement, `snappy` or `gliding` ([UI-AND-FEATURES](UI-AND-FEATURES.md#cursor)).

**Exit**:
- tabs: reorder, rename, drag;
- splits: resize, zoom;
- search over 100k lines in ≤ 150 ms;
- the whole iTerm2-Color-Schemes collection imports, Light/Dark keys and color spaces included;
- IME works in the find bar;
- every cursor option applies on hot reload, and with smooth blinking and movement on, the latency and idle budgets still hold;
- budgets green.

### F4 — Plugin ecosystem

- **Luau host**:
  - API 0.1;
  - `plugin-host-luau`, with one VM per plugin, capabilities, the memory limit and the watchdog;
  - the Luau front end and its type definitions, generated from the Rust types;
  - the conformance suite, and the hostile plugins in CI.
- **Faretra**:
  - `new`, `check`, `test`, `dev`;
  - `add` from a path, from git, and then from the registry;
  - the shared `pkg-model` formats.
- **`plugin-manager` plugin**: the in-app UI for Faretra.
- **Registry v0** (static), ed25519 signatures, templates, a guide for plugin authors.

**Exit**:
- the `faretra new → check → dev` loop works;
- a published plugin installs on another Mac and activates without a restart;
- the hostile test plugins are stopped.

### F5a — Shell integration and blocks

- **Automatic injection** of shell integration for zsh, bash and fish, without touching the user's files.
- **OSC 133 marks** with exit codes:
  - jump between prompts;
  - select the output of the last command;
  - notify when long-running commands finish.
- **Warp-style blocks**:
  - command and output grouped together;
  - a sticky header while scrolling;
  - actions: copy, rerun, collapse.
- **ssh integration**: installs the terminfo on the remote host, or falls back to `xterm-256color` automatically.

### F5b — iTerm2 power features

- **tmux -CC**: tmux windows and panes become native tabs and splits.
- **Windows and sessions**: hotkey window and quick terminal, input broadcast, session restore.
- **Automation**: triggers; profiles with automatic switching (by host or directory).
- **IPC and remote CLI** (`saetta cli …`).
- **Search**: filter mode.
- **Settings**: a panel generated from the config schemas.
- **Encodings**: the encodings other than UTF-8, per profile, as a conversion in the session ([VT-CORE](VT-CORE.md#session-session)).

### F5c — Graphics and kitty protocols

- **Images**: kitty graphics (with Unicode placeholders for tmux), sixel and iTerm2 images (OSC 1337 `File=`), all under a memory quota.
- **Links and selection**:
  - hints and clickable URLs;
  - semantic history: ⌘-click on `file:line` opens the editor;
  - smart selection.

### F6 — Release 1.0

- Stabilize the plugin API as 1.0, in both front ends.
- Complete the docs: user guide, plugin author guide, reference.
- Fix bugs and harden; reconfirm the license ([ADR 0010](adr/0010-mit-license.md)) and the final names.

### F7 — After 1.0

**Linux** (about 11–16 weeks):
- **A spike first**: a single-pane Wayland + GL window, used to extend the thin traits for Linux.
- **`platform-wayland`** with full CSD:
  - resize edges, cursors, window menu;
  - tiled states, fractional scaling;
  - text-input-v3, clipboard and primary selection.
- **Other backends**:
  - `platform-x11` with XIM;
  - `render-gl`;
  - `font-freetype` (fontconfig + FreeType + HarfRust);
  - `keymap-linux`.
- **Packages**: deb, rpm, AppImage, AUR, Nix.
- **Exit**: the macOS E2E suite is also green on GNOME, KDE, sway and X11.

**Everything else**:
- **Registry v1**, with a web UI and Sigstore.
- **Windows**: ConPTY, Win32, DirectWrite, D3D11.
- **Multiplexing**: a mux daemon and SSH domains.
- **Memory**: lz4 compression of the scrollback.
- **Extensibility**: custom shaders; AI plugins and a Warp-style input editor.

## Versions

| Version | Contents |
|---|---|
| 0.1 | macOS MVP (F3) |
| 0.2 | Luau plugins and Faretra (F4) |
| 0.3–0.5 | F5a, F5b, F5c |
| 1.0 | macOS only. Stable plugin API 1.0, complete docs, no open critical bug for three months |
| 1.x | Linux (F7), registry v1 |

## First code files

**F0**

| File | Contents |
|---|---|
| `Cargo.toml` | Workspace, `[workspace.dependencies]`, `[workspace.lints]`, profiles (`panic = "unwind"`, LTO in release) |
| `rust-toolchain.toml` | Pinned stable toolchain and targets (`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` for CI) |
| `deny.toml` | Licenses, advisories, bans (HTTP/TLS and async runtimes only where allowed, via `wrappers`) |
| `justfile` | `build`, `test`, `lint`, `bench`, `fuzz`, `bundle` |
| `.github/workflows/ci.yml` | macOS arm64 (everything) and Linux x86_64 (platform-independent crates) |
| `tools/xtask/src/main.rs` | `check-deps`, later `gen-dist`, `ucd`, `terminfo`, `bundle` |
| `tools/bench/` | Harness for startup, idle and memory |
| `spikes/wasmtime-notarized/` | The F0 plugin-runtime spikes (wasmtime, LuaJIT, Luau), outside the workspace |

**F1**

| File | Contents |
|---|---|
| `crates/vt-core/src/parser/{mod,table,simd}.rs` | State machine, tables, fast path |
| `crates/vt-core/src/page/{cell,row,page,list,style,pin}.rs` | Paged grid |
| `crates/vt-core/src/terminal/{mod,modes,csi,sgr,osc,reports}.rs` | State and sequences |
| `crates/vt-core/src/unicode/{width,grapheme}.rs` | Tables generated from the UCD |
| `crates/vt-core/src/input/{keyboard,kitty,mouse}.rs` | Input encoding |
| `crates/pty/src/{lib,unix}.rs` | PTY and fork+exec |
| `crates/session/src/{lib,io,snapshot}.rs` | I/O thread and snapshots |
| `crates/search/src/{lib,literal,regex}.rs` | Search engine |
| `tools/replay/src/main.rs` | `saetta-replay` |
| `fuzz/fuzz_targets/parser.rs` | First fuzz target |
| `crates/vt-core/tests/recordings/` | Recorded sessions and their `insta` snapshots |

## Open questions

| Question | When to decide |
|---|---|
| Default font bundled with the app (e.g. JetBrains Mono, OFL license) | F2 |
| Which themes to bundle, after checking their licenses | F3 |
| Hosting for registry v0 (GitHub Pages/Releases or object storage) | F4 |
| How `faretra check` type-checks Luau: Luau's analyzer is C++, to embed or to ship as `luau-analyze` ([ADR 0012](adr/0012-luau-plugins.md)) | F4 |
| An atomic store for the watchdog's interrupt in the vendored Luau, or Luau's guarantee that the race is harmless ([ADR 0012](adr/0012-luau-plugins.md)) | F4 |
| The buffers for bulk data between the host and Luau plugins, and how the Luau type definitions are generated from the Rust types ([ADR 0012](adr/0012-luau-plugins.md)) | F4 |
| Final names (Saetta and Faretra are provisional) and the registry domain | Before the public 0.1 |
| Branch protection on `main`: a ruleset that requires the `macos` and `linux` CI checks | When the repository goes public |
| License: MIT or dual MIT/Apache-2.0 ([ADR 0010](adr/0010-mit-license.md)) | Before the public 0.1 |
| Flatpak for Linux, where the sandbox makes launching host shells awkward | F7 |
