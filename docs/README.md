# Saetta — project documentation

> **Saetta** (the terminal; Italian for "lightning bolt") and **Faretra** (the plugin package manager; Italian for "quiver") are provisional names.

Saetta is a terminal emulator for macOS. Version 1.0 targets macOS only ([ADR 0011](adr/0011-macos-only-1-0.md)). The architecture keeps the door open for Linux, planned after 1.0, and later for Windows. Goals:

- **Fast and light on memory**, with budgets measured in CI ([PERFORMANCE.md](PERFORMANCE.md)).
- **The best of existing terminals**: the speed and protocols of Ghostty and kitty; the tabs, tmux -CC and triggers of iTerm2; the restraint of Alacritty; the blocks of Warp.
- **Everything is a plugin**: tabs, splits, search and themes are all plugins. The terminal itself is a small kernel plus a list of plugins.
- **An open ecosystem**: third-party plugins are Luau scripts that run in a sandbox, installed by **Faretra**. Faretra is a separate program from the app, the same way cargo is separate from rustc.

**Status**: F0, the foundations. The workspace skeleton, CI, the dependency rules and the measurement tools exist; the terminal itself starts in F1. This folder holds the architecture, the decisions and the roadmap.

## Documents

Start with [ARCHITECTURE.md](ARCHITECTURE.md), then read the [ROADMAP.md](ROADMAP.md). Each of the other documents covers one subsystem. The reasons behind each choice are in the [ADRs](adr/).

| Document | Contents |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Principles, layers, data model, threads, workspace layout, dependency rules, libraries |
| [VT-CORE.md](VT-CORE.md) | Parser, cells, pages, pins, Unicode, input, PTY, terminfo, supported protocols |
| [PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md) | Extension points, execution model, Rust and Luau front ends, the Luau API, capabilities, lifecycle, limits |
| [PACKAGE-MANAGER.md](PACKAGE-MANAGER.md) | Faretra, file formats, resolution, installation, index, registry |
| [UI-AND-FEATURES.md](UI-AND-FEATURES.md) | UI engine, tabs, splits, search, iTerm2 themes, command palette |
| [SECURITY.md](SECURITY.md) | Threat model, security defaults, IPC, packages, robustness, macOS environment |
| [PERFORMANCE.md](PERFORMANCE.md) | Budgets, how to measure them, benchmarks and CI |
| [ROADMAP.md](ROADMAP.md) | Phases, exit criteria, first code files, open questions |
| [tasks/](tasks/README.md) | Each phase broken into tasks of one day or less, one file per phase |
| [bench/](bench/baseline-F0.md) | Measurements of existing terminals, starting with the F0 baseline |
| [adr/](adr/) | Architecture decision records, one per file |

## Requirements → solution

The phases (F0–F7) are described in the [ROADMAP](ROADMAP.md).

| Requirement | Solution | Where | Phase |
|---|---|---|---|
| Fast and light on memory | 8-byte cells in pages; one GPU surface per window; native backends; no frames when idle | [VT-CORE](VT-CORE.md#paged-grid), [PERFORMANCE](PERFORMANCE.md) | F1–F2 |
| Split panes | `splits` plugin; the layout tree lives in the kernel | [UI-AND-FEATURES](UI-AND-FEATURES.md#splits) | F3 |
| Search the buffer | Kernel `search` service for the engine, `search` plugin for the UI | [UI-AND-FEATURES](UI-AND-FEATURES.md#search) | F3 |
| iTerm2 theme compatibility | `theme-iterm2` plugin: `.itermcolors`, light/dark variants, color spaces | [UI-AND-FEATURES](UI-AND-FEATURES.md#iterm2-themes) | F3 |
| Customizable iTerm2-style tabs | `tabs` plugin, drawn in the titlebar by our UI engine; native tabs disabled | [UI-AND-FEATURES](UI-AND-FEATURES.md#tabs) | F3 |
| Powerful plugin system | Native first-party plugins and sandboxed Luau third-party plugins share one API | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md) | F3–F4 |
| Terminal built from its own plugins | Micro-kernel; the distribution is generated from `dist/default.toml` | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#distribution) | F3 |
| Sub-projects per OS and engine | `platform-*`, `render-*` and `font-*` crates behind thin traits; macOS backends first, Linux after 1.0 | [ARCHITECTURE](ARCHITECTURE.md#workspace-layout), [ADR 0011](adr/0011-macos-only-1-0.md) | F2 / F7 |
| Central package manager and online registry | Faretra, plus a registry that starts static and later becomes a service | [PACKAGE-MANAGER](PACKAGE-MANAGER.md) | F4 / F7 |
| Plugin fetching separate from the app | The `saetta` binary contains no networking code, enforced in CI | [PACKAGE-MANAGER](PACKAGE-MANAGER.md#role) | F0+ |

## Architecture decisions

| ADR | Decision | Status |
|---|---|---|
| [0001](adr/0001-language-rust.md) | Rust for the app, the CLI and the registry | Accepted; its wasmtime reasons outdated by 0012 |
| [0002](adr/0002-micro-kernel-and-plugins.md) | Micro-kernel: mechanism in the core, policy in plugins | Accepted |
| [0003](adr/0003-native-and-wasm-plugins.md) | Two tiers of plugins (native and Wasm) with one API | Accepted; third-party tier superseded by 0012 |
| [0004](adr/0004-native-shell-and-gpu-chrome.md) | Native shell per OS, our own GPU-drawn chrome | Accepted |
| [0005](adr/0005-native-renderers.md) | Native renderers (Metal, OpenGL), one surface per window | Accepted; OpenGL in F7, per 0011 |
| [0006](adr/0006-text-pipeline.md) | Text pipeline: font backends, procedural sprites, UCD width tables | Accepted; shaping superseded by 0011 |
| [0007](adr/0007-own-vt-core.md) | Our own VT core: table-driven parser, paged grid | Accepted |
| [0008](adr/0008-separate-package-manager.md) | Faretra as a separate program; no networking code in the app | Accepted; plugin builds became checks, per 0012 |
| [0009](adr/0009-toml-config.md) | TOML config with schemas, all-or-nothing reload and migrations | Accepted |
| [0010](adr/0010-mit-license.md) | MIT license | Accepted; to be reconfirmed before 0.1 |
| [0011](adr/0011-macos-only-1-0.md) | Version 1.0 targets macOS only; Linux after 1.0; CoreText shaping on macOS | Accepted |
| [0012](adr/0012-luau-plugins.md) | Third-party plugins in Luau, with its native code generator | Accepted |

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
