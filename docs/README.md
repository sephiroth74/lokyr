# Lokyr — project documentation

> **Lokyr** is the terminal, and **Sleip** its plugin package manager, as the user named them on 27 October 2026 ([ADR 0018](adr/0018-names-lokyr-and-sleip.md)). They replaced the working names Bolt and Crossbow ([ADR 0014](adr/0014-names-bolt-and-crossbow.md), [ADR 0016](adr/0016-new-names.md)), which had replaced the provisional Saetta and Faretra.

Lokyr is a terminal emulator for macOS. Version 1.0 targets macOS only ([ADR 0011](adr/0011-macos-only-1-0.md)). The architecture keeps the door open for Linux, planned after 1.0, and later for Windows. Goals:

- **Fast and light on memory**, with budgets measured in CI ([PERFORMANCE.md](PERFORMANCE.md)).
- **The best of existing terminals**: the speed and protocols of Ghostty and kitty; the tabs, tmux -CC and triggers of iTerm2; the restraint of Alacritty; the blocks of Warp.
- **Everything is a plugin**: tabs, splits, search and themes are all plugins. The terminal itself is a small kernel plus a list of plugins.
- **An open ecosystem**: third-party plugins are Luau scripts that run in a sandbox, installed by **Sleip**. Sleip is a separate program from the app, the same way cargo is separate from rustc.

**Status**: F1, the headless terminal core, done on 25 October 2026; F2, macOS with a single pane, is under way: the review of its breakdown with the user, on 26 October 2026, brought the MCP server and the AI agents into the plan ([ADR 0017](adr/0017-mcp-and-agents.md)), with phase F3b after F3. F0 built the workspace skeleton, CI, the dependency rules and the measurement tools; F1 has the parser, the paged grid with its scrollback and reflow, the terminal state, Unicode, the input encoders, the PTY, the session and the search, and the tools that check them: the recorded sessions and their replay, the headless harness with esctest and vttest, the differential tests against `alacritty_terminal`, fuzzing, the benchmarks and the Linux job; its exit criteria are met, and its known issues are listed in its tasks' file. This folder holds the architecture, the decisions and the roadmap.

## Documents

Start with [ARCHITECTURE.md](ARCHITECTURE.md), then read the [ROADMAP.md](ROADMAP.md). Each of the other documents covers one subsystem. The reasons behind each choice are in the [ADRs](adr/).

| Document | Contents |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Principles, layers, data model, threads, workspace layout, dependency rules, libraries |
| [VT-CORE.md](VT-CORE.md) | Parser, cells, pages, pins, Unicode, input, PTY, terminfo, supported protocols |
| [PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md) | Extension points, execution model, Rust and Luau front ends, the Luau API, capabilities, lifecycle, limits |
| [PACKAGE-MANAGER.md](PACKAGE-MANAGER.md) | Sleip, file formats, resolution, installation, index, registry |
| [UI-AND-FEATURES.md](UI-AND-FEATURES.md) | UI engine, tabs, splits, search, iTerm2 themes, command palette |
| [SECURITY.md](SECURITY.md) | Threat model, security defaults, IPC, packages, robustness, macOS environment |
| [PERFORMANCE.md](PERFORMANCE.md) | Budgets, how to measure them, benchmarks and CI |
| [CONFIGURATION.md](CONFIGURATION.md) | The user's guide to the config file: where it is, how it reloads, and every key, with its values and its default |
| [ROADMAP.md](ROADMAP.md) | Phases, exit criteria, first code files, open questions |
| [tasks/](tasks/README.md) | Each phase broken into tasks of one day or less, one file per phase |
| [bench/](bench/baseline-F0.md) | Measurements: of existing terminals, starting with the F0 baseline, of Lokyr's compatibility, starting with [esctest](bench/esctest-F1-44.md), [then](bench/esctest-F1-45.md) and [then](bench/esctest-F1-46.md), and of the VT core's [throughput](bench/throughput-F1-50.md) against `alacritty_terminal`'s, [then](bench/throughput-F1-51.md), of [search](bench/search-F1-54.md), and of the [instructions](bench/instructions-F1-52.md) that CI counts |
| [adr/](adr/) | Architecture decision records, one per file |

## Requirements → solution

The phases (F0–F7) are described in the [ROADMAP](ROADMAP.md).

| Requirement | Solution | Where | Phase |
|---|---|---|---|
| Fast and light on memory | 8-byte cells in pages; one GPU surface per window; native backends; no frames when idle | [VT-CORE](VT-CORE.md#paged-grid), [PERFORMANCE](PERFORMANCE.md), [ADR 0015](adr/0015-vt-core-throughput-exception.md) | F1–F2 |
| Split panes | `splits` plugin; the layout tree lives in the kernel | [UI-AND-FEATURES](UI-AND-FEATURES.md#splits) | F3 |
| Search the buffer | Kernel `search` service for the engine, `search` plugin for the UI | [UI-AND-FEATURES](UI-AND-FEATURES.md#search) | F3 |
| Configurable cursor: blink interval, smooth blinking, block shape, smooth movement (snappy, gliding) | `[cursor]` options, drawn by `render-metal`; the blinking and its fades on the cursor's own layer, which the window server animates, and the movement only while it is in progress | [UI-AND-FEATURES](UI-AND-FEATURES.md#cursor), [ADR 0021](adr/0021-drawables-and-the-cursor-layer.md) | F2–F3 |
| Widths and terminal type that match the programs': the reported `TERM`, Unicode 8 or current width tables, ambiguous-width characters | `[terminal]` options, which the width computation of `vt-core` and the PTY's environment follow | [VT-CORE](VT-CORE.md#settings) | F1–F2 |
| iTerm2's text options: Unicode normalization (NFC, NFD, HFS+), a separate font for non-ASCII text | The `normalization` option of `[terminal]`; the non-ASCII font in `[font]` | [VT-CORE](VT-CORE.md#unicode), [ROADMAP](ROADMAP.md#f2--macos-single-pane) | F1–F2 |
| Encodings other than UTF-8 | UTF-8 only in the MVP; other encodings per profile, as a conversion in the session | [VT-CORE](VT-CORE.md#session-session) | F5b |
| iTerm2 theme compatibility | `theme-iterm2` plugin: `.itermcolors`, light/dark variants, color spaces | [UI-AND-FEATURES](UI-AND-FEATURES.md#iterm2-themes) | F3 |
| Customizable iTerm2-style tabs | `tabs` plugin, drawn in the titlebar by our UI engine; native tabs disabled | [UI-AND-FEATURES](UI-AND-FEATURES.md#tabs) | F3 |
| Powerful plugin system | Native first-party plugins and sandboxed Luau third-party plugins share one API; the commands that a plugin runs are declared in its manifest, and checked against its capabilities | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md), [ADR 0019](adr/0019-commands-errors-and-permissions.md) | F3–F4 |
| Terminal built from its own plugins | Micro-kernel; the distribution is generated from `dist/default.toml` | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#distribution) | F3 |
| Sub-projects per OS and engine | `platform-*`, `render-*` and `font-*` crates behind thin traits; macOS backends first, Linux after 1.0 | [ARCHITECTURE](ARCHITECTURE.md#workspace-layout), [ADR 0011](adr/0011-macos-only-1-0.md) | F2 / F7 |
| Central package manager and online registry | Sleip, plus a registry that starts static and later becomes a service | [PACKAGE-MANAGER](PACKAGE-MANAGER.md) | F4 / F7 |
| Plugin fetching separate from the app | The `lokyr` binary contains no networking code, enforced in CI | [PACKAGE-MANAGER](PACKAGE-MANAGER.md#role) | F0+ |
| Integration with AI agents, the common ones included | The programs' notifications; an agent's presence from its process, and its state from its sequences and `lokyr +notify`, on its tab | [UI-AND-FEATURES](UI-AND-FEATURES.md#agents), [ADR 0017](adr/0017-mcp-and-agents.md) | F2–F3 |
| MCP: agents drive the terminal | `lokyr +mcp`, over stdio through the IPC socket; an agent's scope is its own pane and the panes it opened | [SECURITY](SECURITY.md#mcp), [ADR 0017](adr/0017-mcp-and-agents.md) | F3b |
| Localized: menus, commands and notices in the user's language | Fluent catalogs, English and Italian first; the system's preferred language, or `[app] language`; the system's own texts made from the same catalogs | [ADR 0020](adr/0020-localization.md) | F2 |

## Architecture decisions

| ADR | Decision | Status |
|---|---|---|
| [0001](adr/0001-language-rust.md) | Rust for the app, the CLI and the registry | Accepted; its wasmtime reasons outdated by 0012 |
| [0002](adr/0002-micro-kernel-and-plugins.md) | Micro-kernel: mechanism in the core, policy in plugins | Accepted |
| [0003](adr/0003-native-and-wasm-plugins.md) | Two tiers of plugins (native and Wasm) with one API | Accepted; third-party tier superseded by 0012 |
| [0004](adr/0004-native-shell-and-gpu-chrome.md) | Native shell per OS, our own GPU-drawn chrome | Accepted |
| [0005](adr/0005-native-renderers.md) | Native renderers (Metal, OpenGL), one surface per window | Accepted; OpenGL in F7, per 0011; the cursor's layer, per 0021; two drawables, per 0022 |
| [0006](adr/0006-text-pipeline.md) | Text pipeline: font backends, procedural sprites, UCD width tables | Accepted; shaping superseded by 0011 |
| [0007](adr/0007-own-vt-core.md) | Our own VT core: table-driven parser, paged grid | Accepted |
| [0008](adr/0008-separate-package-manager.md) | Sleip as a separate program; no networking code in the app | Accepted; plugin builds became checks, per 0012 |
| [0009](adr/0009-toml-config.md) | TOML config with schemas, all-or-nothing reload and migrations | Accepted |
| [0010](adr/0010-mit-license.md) | MIT license | Accepted; to be reconfirmed before 0.1 |
| [0011](adr/0011-macos-only-1-0.md) | Version 1.0 targets macOS only; Linux after 1.0; CoreText shaping on macOS | Accepted; Intel question settled by 0013 |
| [0012](adr/0012-luau-plugins.md) | Third-party plugins in Luau, with its native code generator | Accepted |
| [0013](adr/0013-platform-baseline.md) | Platform baseline: macOS 26 or later, Apple Silicon only | Accepted |
| [0014](adr/0014-names-bolt-and-crossbow.md) | Names: Bolt for the terminal, Crossbow for the package manager | Superseded by 0018 |
| [0015](adr/0015-vt-core-throughput-exception.md) | Throughput of the VT core: vtebench's bottom region, with its lines of one character, is an exception | Accepted |
| [0016](adr/0016-new-names.md) | New names: Shard for the package manager; the terminal's still to choose, by the end of F2 | Superseded by 0018, never accepted |
| [0017](adr/0017-mcp-and-agents.md) | An MCP server, and the AI agents that run in the terminal; phase F3b | Accepted |
| [0018](adr/0018-names-lokyr-and-sleip.md) | Names: Lokyr for the terminal, Sleip for the package manager | Accepted |
| [0019](adr/0019-commands-errors-and-permissions.md) | Commands: a flat schema of arguments, errors with codes and categories, and the commands that plugins run, declared and checked; four new capabilities | Accepted |
| [0020](adr/0020-localization.md) | Localization: every text that Lokyr shows, in Fluent catalogs, English and Italian first; the system's preferred language, or the config's | Accepted |
| [0021](adr/0021-drawables-and-the-cursor-layer.md) | Three drawables per window, of which a hidden window keeps one; the cursor's cell on a layer of its own, which the window server blinks | Accepted; amends 0005; its drawables superseded by 0022 |
| [0022](adr/0022-two-drawables.md) | Two drawables per window, as measured at 60 and 120 Hz; three as a build's feature | Accepted; supersedes 0021's drawables |

## Glossary

| Term | Meaning |
|---|---|
| **Kernel** | The core of the app. It owns the data model (windows, tabs, layouts, panes, sessions), commands, keymap, events, config and fast services. It decides nothing about how things look |
| **First-party plugin** | One of our own plugins, statically linked into the binary. It uses only the public SDK |
| **Third-party plugin** | A Luau plugin loaded from disk. It runs in its own sandboxed VM, with explicit capabilities |
| **Distribution** | The list of first-party plugins compiled into the app (`dist/*.toml`) |
| **Chrome** | Everything visible besides the terminal text: tab bar, dividers, find bar, palette |
| **Pane** | An area that shows one terminal. A tab holds one or more panes, arranged by the layout tree |
| **Session** | The PTY, the child process and the VT state of a pane |
| **Pin** | A stable reference to a position in the grid. It stays valid across scrolling and reflow |
| **Capability** | A permission that a third-party plugin must declare and the user must grant (e.g. `terminal-read`) |
| **Slot** | A place in the UI where a plugin can render a view tree (e.g. `tab-bar`) |
| **HostApi** | The Rust trait the kernel implements. Both front ends of the plugin API, Rust and Luau, call into it |

## References worth studying

- **Ghostty**: page list and compact cells, renderer, shell integration, terminfo, `libghostty-vt`.
- **Alacritty**: regex search over the grid, vtebench, PTY spawning.
- **kitty**: keyboard and graphics protocols, layouts, kittens.
- **WezTerm**: the closest existing project (Rust, self-drawn tabs, splits, multiplexer). Also useful for understanding its limits on memory and complexity.
- **iTerm2**: tabs, triggers, tmux -CC, the `.itermcolors` format, esctest.
- **Luau**: sandboxing, interrupting and limiting untrusted scripts, as Roblox does.
- **Zed**: its extensions repository, the model of registry v0.
- **Warp**: command + output blocks.
- **crates.io and pub.dev**: sparse index, verified publishers, package pages.
