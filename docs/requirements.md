# Requirements and their solutions

<sub>Internals: how Lokyr is built, for contributors. [The documentation's home](README.md) leads to the pages for using Lokyr.</sub>

What Lokyr sets out to be, and where each requirement is met: the goals, the project's state, each requirement's solution, and the projects worth studying.

> **Lokyr** is the terminal, and **Sleip** its plugin package manager, their names since 27 October 2026. They replaced the working names Bolt and Crossbow, which had replaced the provisional Saetta and Faretra.

## Goals

Lokyr is a terminal emulator for macOS. Version 1.0 targets macOS only. The architecture keeps the door open for Linux, planned after 1.0, and later for Windows. Goals:

- **Fast and light on memory**, with budgets measured in CI ([PERFORMANCE.md](PERFORMANCE.md)).
- **The best of existing terminals**: the speed and protocols of Ghostty and kitty; the tabs, tmux -CC and triggers of iTerm2; the restraint of Alacritty; the blocks of Warp.
- **Everything is a plugin**: tabs, splits and search are all plugins; themes are the host's, as it draws them, of files that anyone writes. The terminal itself is a small kernel plus a list of plugins.
- **An open ecosystem**: third-party plugins are Luau scripts that run in a sandbox, installed by **Sleip**. Sleip is a separate program from the app, the same way cargo is separate from rustc.

**Status**: F0 to F4 are done: the workspace, CI and the measurement tools; the headless terminal core, its parser, paged grid, scrollback and reflow, Unicode, input, PTY, session and search, with the recorded sessions, esctest and vttest, the differential tests, fuzzing and the benchmarks; and the macOS app of a single pane. F3, the macOS MVP, of tabs, splits, the plugins' API, themes, the command palette, the agents on their tabs and inline images, and F4, the plugin ecosystem, of Luau plugins, Sleip and the registry, came out in releases from 0.2.0 to 0.9.0, then in 1.0.0, the first public release, on 13 November 2026; F3b's settings window came out in 1.1.0 on 14 November 2026, and its Theme Editor, its themes made in the app and the Keys page in 1.2.0 on 16 November 2026; IPC and the MCP server are put off.

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
| iTerm2 theme compatibility | The host's: `.itermcolors`, the one format imported, read as iTerm2 reads it, its color spaces and its light/dark keys; a native format of TOML, a file a variant, its interface's colors of rules that name each other; the themes of permissive licenses held | [UI-AND-FEATURES](UI-AND-FEATURES.md#iterm2-themes) | F3 |
| Customizable iTerm2-style tabs | `tabs` plugin, drawn in the titlebar by our UI engine; native tabs disabled | [UI-AND-FEATURES](UI-AND-FEATURES.md#tabs) | F3 |
| Powerful plugin system | Native first-party plugins and sandboxed Luau third-party plugins share one API; the commands that a plugin runs are declared in its manifest, and checked against its capabilities, but those that a person runs through a plugin with `user-commands`, as the palette; the calls are messages, and the plugins' UI is data that the host draws; each plugin's keys in its section of the config file, of its JSON Schema | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md) | F3–F4 |
| Terminal built from its own plugins | Micro-kernel; the distributions, `dist/*.toml`, are the app's features, which `xtask gen-dist` writes; a plugin turned off by `enabled = false` in its section | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#distribution) | F3 |
| Sub-projects per OS and engine | `platform-*`, `render-*` and `font-*` crates behind thin traits; macOS backends first, Linux after 1.0 | [ARCHITECTURE](ARCHITECTURE.md#workspace-layout) | F2 / F7 |
| Central package manager and online registry | Sleip, plus a registry that starts static, a repository of the index on GitHub with publishing by pull request, its index and site on GitHub Pages at `pkg.lokyr.dev`, signed by keys that a root key in Sleip vouches for, and later becomes a service; Lokyr's own `plugin-manager` of Sleip's commands, through the app | [PACKAGE-MANAGER](PACKAGE-MANAGER.md) | F4 / F7 |
| Plugin fetching separate from the app | The `lokyr` binary contains no networking code of its own, enforced in CI; Sparkle's updater is its one network client; the plugins' network goes through the app, which checks it, to Sleip's helper, after 1.0.0 | [PACKAGE-MANAGER](PACKAGE-MANAGER.md#role) | F0+, the plugins' network F6 |
| Integration with AI agents, the common ones included | The programs' notifications; an agent's presence from its process, and its state from its title and its sequences, on its tab; `lokyr +notify` and the agents' hooks, put off with IPC | [UI-AND-FEATURES](UI-AND-FEATURES.md#agents) | F2–F3 |
| MCP: agents drive the terminal | `lokyr +mcp`, over stdio through the IPC socket; an agent's scope is its own pane and the panes it opened | [SECURITY](SECURITY.md#mcp) | Put off |
| Localized: menus, commands and notices in each person's language | Fluent catalogs, English and Italian first; the system's preferred language, or `[app] language`; the system's own texts made from the same catalogs | [TRANSLATING](../TRANSLATING.md) | F2 |
| The app installs from a DMG, or a Homebrew cask, and updates itself | A signed, notarized DMG of `xtask dmg`, and a cask of the project's Homebrew tap; Sparkle 2, from a signed feed at `lokyr.dev`; the versions `X.Y.Z` of releases, `X.Y.Z-rc.N` of candidates and `X.Y.Z-YYYYMMDD` of builds, a build's number growing with each release | [SECURITY](SECURITY.md#supply-chain-of-our-own-code), [ROADMAP](ROADMAP.md#versions), [releases](releases/README.md) | F3 |

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
