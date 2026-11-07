# Roadmap

**Version 1.0 targets macOS only** ([ADR 0011](adr/0011-macos-only-1-0.md)). Linux comes after 1.0, in F7. To keep that door open, CI builds and tests the platform-independent crates on Linux from F0 onward.

Estimates are rough and assume one full-time developer experienced in Rust. The macOS MVP (F0–F3) takes about 6–9 months.

Each phase is broken into tasks of one day or less in [tasks/](tasks/README.md). A phase's breakdown is written near the end of the previous phase.

## Overview

| Phase | Contents | Exit criteria | Duration |
|---|---|---|---|
| F0 Foundations | Workspace, CI, dependency rules, ADRs, budget harness, notarized plugin-runtime spikes | CI green; ADRs approved; plugin runtime decided | 2–3 weeks |
| F1 Headless VT core | `vt-core`, `pty`, `session`, `search` engine, `lokyr-replay`, fuzzing and differential tests | Compatibility and throughput measured (see below) | 8–12 weeks |
| F2 macOS, single pane | macOS platform, CoreText fonts, Metal rendering, minimal kernel, `core-actions` | Used as the main terminal for a week | 8–12 weeks |
| F3 **macOS MVP** | Full kernel, native SDK, `ui-core` with the plugins' widgets and floating panels, `tabs`, `splits`, `search` plugins, iTerm2 themes, palette, the agents' state on the tabs; copy mode, the scrollbar, shell integration with prompt jumping and notifications, the command's sticky header, the programs' progress, windows restored with their directories, the Finder's service; a DMG, updates through Sparkle; after F4, the first public release, 1.0.0 | Every must-have requirement in place | 12–16 weeks |
| F4 Plugin ecosystem | Before 1.0.0 ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)): Luau host, Luau API, Sleip, the `plugin-manager`, registry v0, `snap` and `awake` published | Publishing and installing work end to end; `snap` and `awake` in the registry; then 1.0.0 | 8–10 weeks |
| F3b MCP and agents | After 1.0.0: IPC and the remote CLI, the MCP server `lokyr +mcp` and its tools, the agents' hooks; the settings' panel | Agents drive panes over MCP, within their scope; the settings' panel edits the config | 5–6 weeks |
| F5a Shell integration and blocks | Warp-style blocks, the last command's output, ssh integration | Verified in zsh, bash and fish, including over ssh | 3–5 weeks |
| F5b iTerm2 power features | tmux -CC, triggers, profiles, broadcast, the scrollback restored | tmux -CC workflows on par with iTerm2 | 8–11 weeks |
| F5c Graphics and kitty protocols | kitty graphics, sixel, iTerm2 images, hints and URLs | `kitten icat`, `imgcat` and `chafa` work | 6–8 weeks |
| F6 Plugin API 1.0 and hardening | Hardening, docs, stable plugin API 1.0 | The plugin API 1.0's criteria (see [Versions](#versions)) | — |
| F7 After 1.0 | Linux, registry v1, Windows, mux daemon, lz4, custom shaders, an AI assistant | — | — |

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
- **`lokyr-replay`** plus the test battery:
  - golden tests from recorded sessions;
  - esctest and vttest;
  - differential tests against `alacritty_terminal`;
  - fuzzing, reflow property tests, benchmarks.

**Exit**:
- ≥ 90% of the applicable esctest tests pass;
- vttest goldens;
- recorded sessions of vim, htop, tmux, less and fzf replay identically;
- no unexplained divergence from `alacritty_terminal`;
- 24 h of fuzzing without crashes: each target 24 hours by the clock on the code that F1 ends with, as `just fuzz-all` runs them;
- throughput ≥ `alacritty_terminal`, but in vtebench's bottom region, with its lines of one character, which keeps 0.6 of it at least ([ADR 0015](adr/0015-vt-core-throughput-exception.md));
- the same tests also pass on Linux CI.

### F2 — macOS, single pane

- **`platform-macos`**:
  - app and window, with the titlebar prototype;
  - input and IME (`NSTextInputClient`);
  - live resize, clipboard, menus;
  - the pane's context menu, after macOS's Terminal: Open Link, Look Up, the man page, the clipboard and the editing ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu));
  - the programs' notifications (OSC 9, 777), which the agents that run in the terminal send ([ADR 0017](adr/0017-mcp-and-agents.md));
  - the quick terminal, as Ghostty's: a window that a global hotkey slides in from an edge of the screen and out again, as the user asked on 30 October 2026, moved from F5b so that the week of use tries it; its policy goes into the `quick-terminal` plugin in F3.
- **`font-coretext` and `text-core`**:
  - font discovery and fallback, and an optional separate font for non-ASCII text, like iTerm2's;
  - CoreText shaping with ligatures;
  - glyph atlas, box-drawing and powerline sprites.
- **`render-metal`**: instanced cells, cursor (block, bar and underline, blinking as DECSCUSR asks), selection, decorations.
- **Minimal kernel**: commands, keymap, config with hot reload, plus the `core-actions` plugin.
- **Terminal settings**: the reported `TERM`, the width tables and the width of ambiguous characters ([VT-CORE](VT-CORE.md#settings)).
- **Environment and bundle**: locale, login shell, TCC strings, signed bundle; the bundle's `lokyr` on the panes' `PATH`, and Settings…, which opens the config file in the text editor; a performance overlay, frames a second, a frame's times, wakeups, memory and output.

**Exit**: used as the main terminal for a week; Japanese IME works; budgets measured; zero wakeups when idle.

### F3 — macOS MVP

- **Full kernel**: workspace, layout tree, events, search and decoration services.
- **The window's controller, portable**: `app-core`, a crate between the kernel and the app, takes from the app the windows' and the panes' state, input's routing, the gestures, the confirmations, the layout's geometry and the frame's composition, generic over the renderer and the fonts' backend, as F2's [architecture review](tasks/F2-macos-single-pane.md#the-architecture-review-for-f3) asked; the app keeps `main` and the backends.
- **Plugin plumbing**: the `HostApi` draft, as a protocol of messages whose changes the host queues for every plugin, and the plugins' UI as data ([ADR 0028](adr/0028-plugin-calls-as-messages.md)), `plugin-sdk` with the Rust front end, `export_plugin!`, `xtask gen-dist`. Native plugins run on the main thread, or on a thread of their own as their manifest asks, `thread = "worker"`; the Luau plugins of F4 on the plugin executor ([PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#execution-model)).
- **`ui-core`**: view tree, layout, AccessKit, with the panes' text and cursor for VoiceOver, which came from F2 as the user chose on 30 October 2026; the widgets that F3's plugins need, as the user chose on 1 November 2026: the IME text field, buttons, toggles, virtualized lists, scroll areas, tooltips, separators and a `progress` element, whose indeterminate state the window server animates; floating panels anchored to a cell, the cursor or the pointer, with the keyboard's focus. A gallery of the widgets, `tools/ui-gallery`, a binary apart from the bundle, shows each of them working, as the user asked on 2 November 2026. A plugin describes its UI and the host draws it: no plugin draws pixels or opens a native window. The menus stay native; a canvas of drawing primitives comes with F4's Luau API.
- **Plugins**: `tabs`, `splits`, `search`, `keymap-macos`, `command-palette`, `quick-terminal`, `progress`; the themes the host's, `[theme]` and the importer of `.itermcolors` ([ADR 0035](adr/0035-themes-are-the-hosts.md)); `links`, which comes in F2, underlines the link under the pointer while ⌘ is held, with the API's events and decorations. `settings`, the panel of the config, moved from F5b on 2 November 2026, moved on to F3b on 7 November 2026 ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)).
- **Basic OSC 7 and 133**: cwd inheritance for splits, and marks.
- **The context menu's items of the plugins**: the splits', the tabs', the man page index, a web search, a link's address ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).
- **Agents**: the presence and the state of the AI agents that run in a pane, working, waiting or done, on its tab ([ADR 0017](adr/0017-mcp-and-agents.md)); after F4, right before 1.0.0, as the user chose on 7 November 2026 ([ADR 0037](adr/0037-agents-on-their-tabs-before-1-0.md)).
- **The programs' progress** (OSC 9;4), as the user asked on 1 November 2026: the `progress` plugin, in a strip under the titlebar, and on the tab with the `tabs` plugin, in four states, normal, error, paused and indeterminate.
- **The command's sticky header**, moved from F5a as the user asked on 1 November 2026, iTerm2's: the command whose output the view shows stays at the pane's top, and gives way to the one before it as the scroll reaches it; a click scrolls to the command. A plugin, over OSC 133's marks; the actions on a command's output stay F5a's.
- **Distribution**, moved from F6 as the user asked on 1 November 2026: a DMG, signed, notarized and stapled, of `xtask dmg`; updates through Sparkle 2, from a signed feed at `lokyr.dev` ([ADR 0026](adr/0026-app-updates.md), [ADR 0029](adr/0029-update-feed-and-releases.md)); `just release`; a panic's backtrace in the log, the releases' dSYMs, Help › Report an Issue, and the third-party licenses' notices; the versions of [ADR 0027](adr/0027-version-numbers.md). It is F3's second block, after the architecture, so that each later block ends with a notarized DMG that updates itself through Sparkle. F3's last tasks, the repository made public and the first public release, 1.0.0, come after F4 ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)).
- **Cursor options**: smooth movement, `snappy` or `gliding` ([UI-AND-FEATURES](UI-AND-FEATURES.md#cursor)); the shape, the blink interval and smooth blinking come in F2.

- **What every terminal's users expect**, as a search of the major terminals' trackers found on 30 October 2026, which the user took:
  - copy mode: a cursor of the keyboard's, to move and select in the scrollback and the screen, a first-party plugin, with the HostApi's selection by range, a scroll to a row and a cursor of the plugin's own;
  - the scrollbar, an overlay that the theme styles, with the search's marks;
  - shell integration, moved from F5a: the scripts injected for zsh, bash and fish, without touching the user's files; jumping between prompts, OSC 133's marks; a notice when a long command ends, of its exit code and its duration, timed by its marks, without a timer;
  - the windows, tabs and splits restored as Lokyr starts again, with their directories, moved from F5b; their scrollback stays F5b's;
  - the Finder's service, New Lokyr Window Here, and a folder dropped on the Dock's icon;
  - CSI 2031, which tells programs that the color scheme changed;
  - the quick terminal's policy, out of the app into the `quick-terminal` plugin, once the HostApi has windows and hotkeys.

**Exit**:
- tabs: reorder, rename, drag;
- splits: resize, zoom;
- search over 100k lines in ≤ 150 ms;
- the whole iTerm2-Color-Schemes collection imports, Light/Dark keys and color spaces included;
- IME works in the find bar;
- every cursor option applies on hot reload, and with smooth blinking and movement on, the latency and idle budgets still hold;
- a plugin's floating panel, the find bar with its options, works with the keyboard, the input method and VoiceOver;
- Gradle's progress shows under the titlebar and on its tab;
- the sticky header follows the commands through a scroll;
- a plugin on a thread of its own slows no frame;
- the notarized DMG installs, and Sparkle updates one release to the next;
- budgets green.

### F4 — Plugin ecosystem

Before 1.0.0, after F3's daily features, as 0.8 and 0.9, as the user chose on 7 November 2026, so that the first public release has third-party plugins that anyone installs from the registry ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)). Its breakdown is a task of F3's plan, after 0.7; its exit joins F3's at the exit review before 1.0.0.

- **Luau host**:
  - API 0.1;
  - `plugin-host-luau`, with one VM per plugin, capabilities, the memory limit and the watchdog;
  - the Luau front end and its type definitions, generated from the Rust types;
  - the conversion of Luau's tables to the view's `Node`, which bounds its depth itself, as `Node`'s drop, clone and comparison recurse before the tree's check, and takes a node's `role` and `label`, as the reviews of F3's `ui-core` asked; and of Luau's tables to the SDK's `Data`, of the events and the states, within `Data::DEEPEST` levels as it converts them, as the review after F3-153 asked;
  - the conformance suite, and the hostile plugins in CI.
- **Sleip**:
  - `new`, `check`, `test`, `dev`;
  - `add` from a path, from git, and then from the registry;
  - the shared `pkg-model` formats.
- **`plugin-manager` plugin**: the in-app UI for Sleip.
- **`snap`**, a [reference third-party plugin](PLUGIN-SYSTEM.md#reference-third-party-plugins), as VS Code's CodeSnap: an image of the selection, of the screen or of a command's output, in a window's frame; written in Luau as a third party would, published to the registry, with its own config, its capabilities, and the host's `image` service.
- **`awake`**, a [reference third-party plugin](PLUGIN-SYSTEM.md#awake), in Luau, as the user asked on 6 November 2026: the Mac kept awake, always or while programs run, from a button beside the window's buttons; the host's request of a power assertion, its capability `keep-awake`, of an ADR, and a slot of the titlebar's buttons.
- **Registry v0** (static), ed25519 signatures, templates, a guide for plugin authors: a repository of the index, publishing by pull request, the archives in its releases, the index and the site on GitHub Pages at `pkg.lokyr.dev`, and the keys that Sleip trusts ([ADR 0031](adr/0031-registry-v0.md)).

**Exit**:
- the `sleip new → check → dev` loop works;
- a published plugin installs on another Mac and activates without a restart;
- the hostile test plugins are stopped.

### F3b — MCP and agents

After 1.0.0, as 1.1 ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)).

- **IPC and the remote CLI** (`lokyr cli …`), moved from F5b: the socket and its checks of [SECURITY.md](SECURITY.md#ipc).
- **The MCP server**: `lokyr +mcp`, over stdio, which the agent starts and which talks to the app through the socket; the app opens no port ([ADR 0017](adr/0017-mcp-and-agents.md)).
- **Its tools**: the windows, tabs and panes; a pane's screen and scrollback; a tab or a split that runs a command; the command's end and its exit code (OSC 133); text and keys sent to a pane; the working directory.
- **An agent's scope**: its own pane and the panes that it opened; another pane after the user's confirmation; the `[mcp]` section of the config ([SECURITY.md](SECURITY.md#mcp)).
- **The agents' hooks**: `lokyr +notify`, which they call for an exact state, and `lokyr +agent-hooks <agent>`, which prints the lines that their config takes.
- **The settings' panel**, `settings`, a first-party plugin, generated from the config's keys and the guide's text, with the fonts, the colors, the keys' bindings and a file with an error, moved from F3 on 7 November 2026 ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)): F3-103 and F3-104 of F3's plan, which F3b's breakdown takes.

**Exit**:
- Claude Code and Codex open a pane over MCP, run a command in it, and read its output and its exit code;
- an agent that reaches outside its scope gets the user's confirmation first;
- `lokyr cli` lists and drives the panes;
- budgets green.

### F5a — Shell integration and blocks

- **OSC 133 marks**, past F3's jumps and notices: select the output of the last command.
- **Warp-style blocks**:
  - command and output grouped together, past F3's sticky header;
  - actions: copy, select, the command's details, rerun, collapse, share, from the sticky header's menu and the context menu, with the lines' marks ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).
- **ssh integration**: installs the terminfo on the remote host, or falls back to `xterm-256color` automatically.

### F5b — iTerm2 power features

- **tmux -CC**: tmux windows and panes become native tabs and splits.
- **Windows and sessions**: input broadcast; the scrollback restored with the windows, which F3 restores. The quick terminal came in F2.
- **Automation**: triggers; profiles with automatic switching (by host or directory).
- **Search**: filter mode.
- **The context menu**: broadcast, locked panes, annotations, triggers and snippets ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).
- **Settings**: the panel generated from the config came in F3.
- **Encodings**: the encodings other than UTF-8, per profile, as a conversion in the session ([VT-CORE](VT-CORE.md#session-session)).

### F5c — Graphics and kitty protocols

- **Images**: kitty graphics (with Unicode placeholders for tmux), sixel and iTerm2 images (OSC 1337 `File=`), all under a memory quota.
- **Choosing a font**, as the user asked on 28 October 2026: `lokyr +list-fonts`, beside `+show-config` and `+validate-config`, which since [F2-149](tasks/F2-macos-single-pane.md#f2-149--list-fonts) lists the system's families and their styles as `[font]` names them, as text, with `--monospace` for the fixed-pitch families and `--family` for a family's styles alone, draws each family in its own face with a line of samples, as images that the terminal shows.
- **Links and selection**, in the `links` plugin, which opens links, URLs and paths with ⌘-click since F2:
  - hints;
  - semantic history: ⌘-click on `file:line` opens the editor;
  - smart selection, with its actions in the context menu, and Quick Look for a link ([UI-AND-FEATURES](UI-AND-FEATURES.md#the-panes-context-menu)).

### F6 — Plugin API 1.0 and hardening

- Stabilize the plugin API as 1.0, in both front ends.
- Complete the docs: user guide, plugin author guide, reference.
- Fix bugs and harden; reconfirm the license ([ADR 0010](adr/0010-mit-license.md)) and the final names.
- **Distribution**, as the user asked on 29 October 2026: the menu's Install Command Line Tool…, which links `lokyr` to the bundle's executable in a directory of the `PATH`; and, if Lokyr goes through Homebrew, a cask whose `binary` stanza links it in Homebrew's `bin`. The DMG and the updates came in F3.

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
- **Extensibility**: custom shaders; AI plugins, an assistant with an LLM among them ([ADR 0017](adr/0017-mcp-and-agents.md)), and a Warp-style input editor.

## Versions

As the user chose on 1 November 2026 ([ADR 0027](adr/0027-version-numbers.md)): releases are `X.Y.Z`, nightly and beta builds `X.Y.Z-YYYYMMDD`, release candidates `X.Y.Z-rc.N`.

| Version | Contents |
|---|---|
| 0.2–0.7 | F3's blocks, on its branch: 0.2 the architecture, 0.3 the distribution, 0.4 `ui-core`, 0.5 the plugins' plumbing, 0.6 the plugins, 0.7 the daily features ([ADR 0029](adr/0029-update-feed-and-releases.md)); a fix takes the patch |
| 0.8–0.9 | F4's blocks, before 1.0.0: 0.8 the Luau host and Sleip's own loop, 0.9 the registry, the `plugin-manager`, and `snap` and `awake` published ([ADR 0036](adr/0036-plugin-ecosystem-before-1-0.md)) |
| 1.0.0 | macOS MVP (F3) and the plugin ecosystem (F4), the first public release, the repository made public, with the agents on their tabs, which come after F4 ([ADR 0037](adr/0037-agents-on-their-tabs-before-1-0.md)); semantic versioning from here: patch, minor and major |
| 1.1 | MCP and agents, and the settings' panel (F3b) |
| 1.x | F5a, F5b, F5c, and the plugin API 1.0 (F6): stable plugin API in both front ends, complete docs, no open critical bug for three months; a major only for a change that breaks what users have |
| later | Linux (F7), registry v1 |

The plugin API has its own version, `plugin-sdk`'s, experimental until its 1.0 ([PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#versioning)).

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
| `crates/vt-core/src/parser/{mod,table,scan}.rs` | State machine, tables, fast path |
| `crates/vt-core/src/page/{cell,row,page,list,style,pin}.rs` | Paged grid |
| `crates/vt-core/src/terminal/{mod,modes,csi,sgr,osc,reports}.rs` | State and sequences |
| `crates/vt-core/src/unicode/{width,grapheme}.rs` | Tables generated from the UCD |
| `crates/vt-core/src/input/{keyboard,kitty,mouse}.rs` | Input encoding |
| `crates/pty/src/{lib,unix}.rs` | PTY and fork+exec |
| `crates/session/src/{lib,io,snapshot}.rs` | I/O thread and snapshots |
| `crates/search/src/{lib,literal,regex}.rs` | Search engine |
| `tools/replay/src/main.rs` | `lokyr-replay` |
| `fuzz/fuzz_targets/parser.rs` | First fuzz target |
| `crates/vt-core/tests/recordings/` | Recorded sessions and their `insta` snapshots |

## Open questions

| Question | When to decide |
|---|---|
| How `sleip check` type-checks Luau: Luau's analyzer is C++, to embed or to ship as `luau-analyze` ([ADR 0012](adr/0012-luau-plugins.md)) | F4 |
| An atomic store for the watchdog's interrupt in the vendored Luau, or Luau's guarantee that the race is harmless ([ADR 0012](adr/0012-luau-plugins.md)) | F4 |
| The buffers for bulk data between the host and Luau plugins, and how the Luau type definitions are generated from the Rust types ([ADR 0012](adr/0012-luau-plugins.md)) | F4 |
| The threads of the Luau plugins' executor: one, a fixed pool, or a pool that grows, which the user asked on 5 November 2026 to consider for third-party plugins: a thread at the start, one more as all of them hold calls under way, up to a bound, and an idle one ending after a while; each plugin one call at a time, its VM on one thread at a time. First-party plugins on a thread of their own have one each, as the user chose the same day ([PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#execution-model)) | F4, with the executor |
| Buying the domains of [ADR 0018](adr/0018-names-lokyr-and-sleip.md) that are left, `lokyr.sh` and `lokyr.app`; `lokyr.dev`, of the updates' feed and of the registry's `pkg.lokyr.dev`, the user bought on 1 November 2026 | Before F4 |
| Lokyr's and Sleip's marks, in the US, Europe and elsewhere, checked by hand ([ADR 0018](adr/0018-names-lokyr-and-sleip.md)) | Before the first public release, 1.0.0 |
| The names in Homebrew and other registries | Before the first public release, 1.0.0 |
| Branch protection on `main`: a ruleset that requires the `macos` and `linux` CI checks | When the repository goes public |
| License: MIT or dual MIT/Apache-2.0 ([ADR 0010](adr/0010-mit-license.md)) | Before the first public release, 1.0.0 |
| Flatpak for Linux, where the sandbox makes launching host shells awkward | F7 |
