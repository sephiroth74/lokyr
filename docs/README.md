# Lokyr — project documentation

> **Lokyr** is the terminal, and **Sleip** its plugin package manager, their names since 27 October 2026. They replaced the working names Bolt and Crossbow, which had replaced the provisional Saetta and Faretra.

Lokyr is a terminal emulator for macOS. Version 1.0 targets macOS only. The architecture keeps the door open for Linux, planned after 1.0, and later for Windows. Goals:

- **Fast and light on memory**, with budgets measured in CI ([PERFORMANCE.md](PERFORMANCE.md)).
- **The best of existing terminals**: the speed and protocols of Ghostty and kitty; the tabs, tmux -CC and triggers of iTerm2; the restraint of Alacritty; the blocks of Warp.
- **Everything is a plugin**: tabs, splits and search are all plugins; themes are the host's, as it draws them, of files that anyone writes. The terminal itself is a small kernel plus a list of plugins.
- **An open ecosystem**: third-party plugins are Luau scripts that run in a sandbox, installed by **Sleip**. Sleip is a separate program from the app, the same way cargo is separate from rustc.

**Status**: F0 to F2 are done: the workspace, CI and the measurement tools; the headless terminal core, its parser, paged grid, scrollback and reflow, Unicode, input, PTY, session and search, with the recorded sessions, esctest and vttest, the differential tests, fuzzing and the benchmarks; and the macOS app of a single pane. F3, the macOS MVP, of tabs, splits, the plugins' API, themes, the command palette, the agents on their tabs and inline images, and F4, the plugin ecosystem, of Luau plugins, Sleip and the registry, came out in releases from 0.2.0 to 0.9.0; 1.0.0, the first public release, comes next, then F3b, the MCP server. This folder holds the architecture, the roadmap and the guides.

## Documents

Start with [ARCHITECTURE.md](ARCHITECTURE.md), then read the [ROADMAP.md](ROADMAP.md). Each of the other documents covers one subsystem.

| Document | Contents |
|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | Principles, layers, data model, threads, workspace layout, dependency rules, libraries |
| [VT-CORE.md](VT-CORE.md) | Parser, cells, pages, pins, Unicode, input, PTY, terminfo, supported protocols |
| [PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md) | Extension points, execution model, Rust and Luau front ends, the Luau API, capabilities, lifecycle, limits |
| [PACKAGE-MANAGER.md](PACKAGE-MANAGER.md) | Sleip, file formats, resolution, installation, index, registry |
| [UI-AND-FEATURES.md](UI-AND-FEATURES.md) | UI engine, tabs, splits, search, iTerm2 themes, command palette |
| [SECURITY.md](SECURITY.md) | Threat model, security defaults, IPC, packages, robustness, macOS environment |
| [PERFORMANCE.md](PERFORMANCE.md) | Budgets, how to measure them, benchmarks and CI |
| [CONFIGURATION.md](CONFIGURATION.md) | The guide to the config file: where it is, how it reloads, and every key, with its values and its default |
| [plugins/](plugins/README.md) | The bundled plugins, a page each: their commands, keys, config and what turning one off takes away |
| [plugins/authoring.md](plugins/authoring.md) | Writing a plugin: from `sleip new` to its tests, `sleip dev` and its publishing, and the API at a glance, for plugins' authors |
| [themes/authoring.md](themes/authoring.md) | Making a theme: the format, every key and color token, `extends` and `.itermcolors`, for themes' authors |
| [TRANSLATING.md](../TRANSLATING.md) | How to translate Lokyr: the catalogs, their format, and the steps of a new language |
| [ROADMAP.md](ROADMAP.md) | Phases, exit criteria, first code files, open questions |
| [bench/](bench/baseline-F0.md) | Measurements: of existing terminals, starting with the F0 baseline, of Lokyr's compatibility, starting with [esctest](bench/esctest-F1-44.md), [then](bench/esctest-F1-45.md) and [then](bench/esctest-F1-46.md), and of the VT core's [throughput](bench/throughput-F1-50.md) against `alacritty_terminal`'s, [then](bench/throughput-F1-51.md), of [search](bench/search-F1-54.md), [then through the app's service](bench/search-F3-52.md), and of the [instructions](bench/instructions-F1-52.md) that CI counts |

## Requirements → solution

The phases (F0–F7) are described in the [ROADMAP](ROADMAP.md).

| Requirement | Solution | Where | Phase |
|---|---|---|---|
| Fast and light on memory | 8-byte cells in pages; one GPU surface per window; native backends; no frames when idle | [VT-CORE](VT-CORE.md#paged-grid), [PERFORMANCE](PERFORMANCE.md) | F1–F2 |
| Split panes | `splits` plugin; the layout tree lives in the kernel | [UI-AND-FEATURES](UI-AND-FEATURES.md#splits) | F3 |
| Search the buffer | Kernel `search` service for the engine, `search` plugin for the UI | [UI-AND-FEATURES](UI-AND-FEATURES.md#search) | F3 |
| Configurable cursor: blink interval, smooth blinking, block shape, smooth movement (snappy) | `[cursor]` options, drawn by `render-metal`; the blinking and its fades on the cursor's own layer, which the window server animates, as it blinks text on a layer of its own, and the movement only while it is in progress | [UI-AND-FEATURES](UI-AND-FEATURES.md#cursor) | F2–F3 |
| Widths and terminal type that match the programs': the reported `TERM`, Unicode 8 or current width tables, ambiguous-width characters | `[terminal]` options, which the width computation of `vt-core` and the PTY's environment follow | [VT-CORE](VT-CORE.md#settings) | F1–F2 |
| iTerm2's text options: Unicode normalization (NFC, NFD, HFS+), a separate font for non-ASCII text | The `normalization` option of `[terminal]`; the non-ASCII font in `[font]` | [VT-CORE](VT-CORE.md#unicode), [ROADMAP](ROADMAP.md#f2--macos-single-pane) | F1–F2 |
| The Nerd Fonts' icons, of prompts and tools, with any font | Symbols Nerd Font Mono in the binary, which draws the private use areas' characters that the configured font lacks, before the system's fallback | [CONFIGURATION](CONFIGURATION.md#font) | F2 |
| Encodings other than UTF-8 | UTF-8 only in the MVP; other encodings per profile, as a conversion in the session | [VT-CORE](VT-CORE.md#session-session) | F5b |
| iTerm2 theme compatibility | The host's: `.itermcolors`, the one format imported, read as iTerm2 reads it, its color spaces and its light/dark keys; a native format of TOML, a file a variant; the themes of permissive licenses held | [UI-AND-FEATURES](UI-AND-FEATURES.md#iterm2-themes) | F3 |
| Customizable iTerm2-style tabs | `tabs` plugin, drawn in the titlebar by our UI engine; native tabs disabled | [UI-AND-FEATURES](UI-AND-FEATURES.md#tabs) | F3 |
| Powerful plugin system | Native first-party plugins and sandboxed Luau third-party plugins share one API; the commands that a plugin runs are declared in its manifest, and checked against its capabilities, but those that a person runs through a plugin with `user-commands`, as the palette; the calls are messages, and the plugins' UI is data that the host draws; each plugin's keys in its section of the config file, of its JSON Schema | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md) | F3–F4 |
| Terminal built from its own plugins | Micro-kernel; the distributions, `dist/*.toml`, are the app's features, which `xtask gen-dist` writes; a plugin turned off by `enabled = false` in its section | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#distribution) | F3 |
| Sub-projects per OS and engine | `platform-*`, `render-*` and `font-*` crates behind thin traits; macOS backends first, Linux after 1.0 | [ARCHITECTURE](ARCHITECTURE.md#workspace-layout) | F2 / F7 |
| Central package manager and online registry | Sleip, plus a registry that starts static, a repository of the index on GitHub with publishing by pull request, its index and site on GitHub Pages at `pkg.lokyr.dev`, signed by keys that a root key in Sleip vouches for, and later becomes a service; Lokyr's own `plugin-manager` of Sleip's commands, through the app | [PACKAGE-MANAGER](PACKAGE-MANAGER.md) | F4 / F7 |
| Plugin fetching separate from the app | The `lokyr` binary contains no networking code of its own, enforced in CI; Sparkle's updater is its one network client; the plugins' network goes through the app, which checks it, to Sleip's helper, after 1.0.0 | [PACKAGE-MANAGER](PACKAGE-MANAGER.md#role) | F0+, the plugins' network F6 |
| Integration with AI agents, the common ones included | The programs' notifications; an agent's presence from its process, and its state from its title and its sequences, on its tab; `lokyr +notify` and the agents' hooks with F3b's IPC | [UI-AND-FEATURES](UI-AND-FEATURES.md#agents) | F2–F3b |
| MCP: agents drive the terminal | `lokyr +mcp`, over stdio through the IPC socket; an agent's scope is its own pane and the panes it opened | [SECURITY](SECURITY.md#mcp) | F3b |
| Localized: menus, commands and notices in each person's language | Fluent catalogs, English and Italian first; the system's preferred language, or `[app] language`; the system's own texts made from the same catalogs | [TRANSLATING](../TRANSLATING.md) | F2 |
| The app installs from a DMG, or a Homebrew cask, and updates itself | A signed, notarized DMG of `xtask dmg`, and a cask of the project's Homebrew tap; Sparkle 2, from a signed feed at `lokyr.dev`; the versions `X.Y.Z` of releases, `X.Y.Z-rc.N` of candidates and `X.Y.Z-YYYYMMDD` of builds, a build's number growing with each release | [SECURITY](SECURITY.md#supply-chain-of-our-own-code), [ROADMAP](ROADMAP.md#versions), [releases](releases/README.md) | F3 |

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
| **Capability** | A permission that a third-party plugin must declare and that must be granted to it (e.g. `terminal-read`) |
| **Slot** | A place in the UI where a plugin can render a view tree (e.g. `tab-bar`) |
| **HostApi** | The plugin API: the requests, responses and events of `plugin-sdk`'s `message` module, which the kernel serves through `host::Host`. Both front ends, Rust's `Cx` and Luau's bindings, make its requests |

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
