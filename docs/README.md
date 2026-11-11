# Lokyr — project documentation

> **Lokyr** is the terminal, and **Sleip** its plugin package manager, as the user named them on 27 October 2026 ([ADR 0018](adr/0018-names-lokyr-and-sleip.md)). They replaced the working names Bolt and Crossbow ([ADR 0014](adr/0014-names-bolt-and-crossbow.md), [ADR 0016](adr/0016-new-names.md)), which had replaced the provisional Saetta and Faretra.

Lokyr is a terminal emulator for macOS. Version 1.0 targets macOS only ([ADR 0011](adr/0011-macos-only-1-0.md)). The architecture keeps the door open for Linux, planned after 1.0, and later for Windows. Goals:

- **Fast and light on memory**, with budgets measured in CI ([PERFORMANCE.md](PERFORMANCE.md)).
- **The best of existing terminals**: the speed and protocols of Ghostty and kitty; the tabs, tmux -CC and triggers of iTerm2; the restraint of Alacritty; the blocks of Warp.
- **Everything is a plugin**: tabs, splits and search are all plugins; themes are the host's, as it draws them, of files that anyone writes ([ADR 0035](adr/0035-themes-are-the-hosts.md)). The terminal itself is a small kernel plus a list of plugins.
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
| [UI-TOOLKITS.md](UI-TOOLKITS.md) | A survey of Rust's UI frameworks and of the native toolkits, why Lokyr keeps its own, and the ideas taken from them |
| [SECURITY.md](SECURITY.md) | Threat model, security defaults, IPC, packages, robustness, macOS environment |
| [PERFORMANCE.md](PERFORMANCE.md) | Budgets, how to measure them, benchmarks and CI |
| [CONFIGURATION.md](CONFIGURATION.md) | The user's guide to the config file: where it is, how it reloads, and every key, with its values and its default |
| [plugins/](plugins/README.md) | The bundled plugins, a page each: their commands, keys, config and what turning one off takes away |
| [TRANSLATING.md](../TRANSLATING.md) | How to translate Lokyr: the catalogs, their format, and the steps of a new language |
| [ROADMAP.md](ROADMAP.md) | Phases, exit criteria, first code files, open questions |
| [LINUX-VM.md](LINUX-VM.md) | A proposal, not implemented: CI's Linux job in a VM on the Mac, of Apple's Virtualization framework |
| [tasks/](tasks/README.md) | Each phase broken into tasks of one day or less, one file per phase |
| [tasks/BACKLOG.md](tasks/BACKLOG.md) | The issues that no task of a phase's plan tracks, an entry each, by priority |
| [bench/](bench/baseline-F0.md) | Measurements: of existing terminals, starting with the F0 baseline, of Lokyr's compatibility, starting with [esctest](bench/esctest-F1-44.md), [then](bench/esctest-F1-45.md) and [then](bench/esctest-F1-46.md), and of the VT core's [throughput](bench/throughput-F1-50.md) against `alacritty_terminal`'s, [then](bench/throughput-F1-51.md), of [search](bench/search-F1-54.md), [then through the app's service](bench/search-F3-52.md), and of the [instructions](bench/instructions-F1-52.md) that CI counts |
| [adr/](adr/) | Architecture decision records, one per file |

## Requirements → solution

The phases (F0–F7) are described in the [ROADMAP](ROADMAP.md).

| Requirement | Solution | Where | Phase |
|---|---|---|---|
| Fast and light on memory | 8-byte cells in pages; one GPU surface per window; native backends; no frames when idle | [VT-CORE](VT-CORE.md#paged-grid), [PERFORMANCE](PERFORMANCE.md), [ADR 0015](adr/0015-vt-core-throughput-exception.md) | F1–F2 |
| Split panes | `splits` plugin; the layout tree lives in the kernel | [UI-AND-FEATURES](UI-AND-FEATURES.md#splits) | F3 |
| Search the buffer | Kernel `search` service for the engine, `search` plugin for the UI | [UI-AND-FEATURES](UI-AND-FEATURES.md#search) | F3 |
| Configurable cursor: blink interval, smooth blinking, block shape, smooth movement (snappy) | `[cursor]` options, drawn by `render-metal`; the blinking and its fades on the cursor's own layer, which the window server animates, as it blinks text on a layer of its own, and the movement only while it is in progress | [UI-AND-FEATURES](UI-AND-FEATURES.md#cursor), [ADR 0021](adr/0021-drawables-and-the-cursor-layer.md), [ADR 0023](adr/0023-blinking-text-layer.md) | F2–F3 |
| Widths and terminal type that match the programs': the reported `TERM`, Unicode 8 or current width tables, ambiguous-width characters | `[terminal]` options, which the width computation of `vt-core` and the PTY's environment follow | [VT-CORE](VT-CORE.md#settings) | F1–F2 |
| iTerm2's text options: Unicode normalization (NFC, NFD, HFS+), a separate font for non-ASCII text | The `normalization` option of `[terminal]`; the non-ASCII font in `[font]` | [VT-CORE](VT-CORE.md#unicode), [ROADMAP](ROADMAP.md#f2--macos-single-pane) | F1–F2 |
| The Nerd Fonts' icons, of prompts and tools, with any font | Symbols Nerd Font Mono in the binary, which draws the private use areas' characters that the configured font lacks, before the system's fallback | [ADR 0025](adr/0025-symbols-font.md) | F2 |
| Encodings other than UTF-8 | UTF-8 only in the MVP; other encodings per profile, as a conversion in the session | [VT-CORE](VT-CORE.md#session-session) | F5b |
| iTerm2 theme compatibility | The host's: `.itermcolors`, the one format imported, read as iTerm2 reads it, its color spaces and its light/dark keys; a native format of TOML, a file a variant; the themes of permissive licenses held | [UI-AND-FEATURES](UI-AND-FEATURES.md#iterm2-themes), [ADR 0034](adr/0034-themes-native-format-and-itermcolors.md), [ADR 0035](adr/0035-themes-are-the-hosts.md) | F3 |
| Customizable iTerm2-style tabs | `tabs` plugin, drawn in the titlebar by our UI engine; native tabs disabled | [UI-AND-FEATURES](UI-AND-FEATURES.md#tabs) | F3 |
| Powerful plugin system | Native first-party plugins and sandboxed Luau third-party plugins share one API; the commands that a plugin runs are declared in its manifest, and checked against its capabilities, but those that the user runs through a plugin with `user-commands`, as the palette; the calls are messages, and the plugins' UI is data that the host draws; each plugin's keys in its section of the config file, of its JSON Schema | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md), [ADR 0019](adr/0019-commands-errors-and-permissions.md), [ADR 0024](adr/0024-user-commands.md), [ADR 0028](adr/0028-plugin-calls-as-messages.md), [ADR 0032](adr/0032-plugins-config-and-state.md), [ADR 0039](adr/0039-workspace-view-decorate-pointer-capabilities.md), [ADR 0041](adr/0041-save-panel-of-no-capability.md), [ADR 0043](adr/0043-keep-awake-title-buttons-ticked-items.md), [ADR 0044](adr/0044-titlebar-buttons-icons-and-overflow.md) | F3–F4 |
| Terminal built from its own plugins | Micro-kernel; the distributions, `dist/*.toml`, are the app's features, which `xtask gen-dist` writes; a plugin turned off by `enabled = false` in its section | [PLUGIN-SYSTEM](PLUGIN-SYSTEM.md#distribution), [ADR 0033](adr/0033-plugins-turned-off-in-their-sections.md) | F3 |
| Sub-projects per OS and engine | `platform-*`, `render-*` and `font-*` crates behind thin traits; macOS backends first, Linux after 1.0 | [ARCHITECTURE](ARCHITECTURE.md#workspace-layout), [ADR 0011](adr/0011-macos-only-1-0.md) | F2 / F7 |
| Central package manager and online registry | Sleip, plus a registry that starts static, a repository of the index on GitHub with publishing by pull request, its index and site on GitHub Pages at `pkg.lokyr.dev`, signed by keys that a root key in Sleip vouches for, and later becomes a service; Lokyr's own `plugin-manager` of Sleip's commands, through the app | [PACKAGE-MANAGER](PACKAGE-MANAGER.md), [ADR 0031](adr/0031-registry-v0.md), [ADR 0045](adr/0045-packages-service.md) | F4 / F7 |
| Plugin fetching separate from the app | The `lokyr` binary contains no networking code of its own, enforced in CI; Sparkle's updater is its one network client ([ADR 0026](adr/0026-app-updates.md)); the plugins' network goes through the app, which checks it, to Sleip's helper, after 1.0.0 ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) | [PACKAGE-MANAGER](PACKAGE-MANAGER.md#role) | F0+, the plugins' network F6 |
| Integration with AI agents, the common ones included | The programs' notifications; an agent's presence from its process, and its state from its title and its sequences, on its tab; `lokyr +notify` and the agents' hooks with F3b's IPC | [UI-AND-FEATURES](UI-AND-FEATURES.md#agents), [ADR 0017](adr/0017-mcp-and-agents.md) | F2–F3b |
| MCP: agents drive the terminal | `lokyr +mcp`, over stdio through the IPC socket; an agent's scope is its own pane and the panes it opened | [SECURITY](SECURITY.md#mcp), [ADR 0017](adr/0017-mcp-and-agents.md) | F3b |
| Localized: menus, commands and notices in the user's language | Fluent catalogs, English and Italian first; the system's preferred language, or `[app] language`; the system's own texts made from the same catalogs | [ADR 0020](adr/0020-localization.md) | F2 |
| The app installs from a DMG and updates itself | A signed, notarized DMG of `xtask dmg`; Sparkle 2, from a signed feed at `lokyr.dev`; the versions of ADR 0027, a build's number growing with each release | [ADR 0026](adr/0026-app-updates.md), [ADR 0027](adr/0027-version-numbers.md), [ADR 0029](adr/0029-update-feed-and-releases.md), [ADR 0030](adr/0030-release-notes-in-english.md) | F3 |

## Architecture decisions

| ADR | Decision | Status |
|---|---|---|
| [0001](adr/0001-language-rust.md) | Rust for the app, the CLI and the registry | Accepted; its wasmtime reasons outdated by 0012 |
| [0002](adr/0002-micro-kernel-and-plugins.md) | Micro-kernel: mechanism in the core, policy in plugins | Accepted; themes the host's, per 0035 |
| [0003](adr/0003-native-and-wasm-plugins.md) | Two tiers of plugins (native and Wasm) with one API | Accepted; third-party tier superseded by 0012; the calls as messages, per 0028 |
| [0004](adr/0004-native-shell-and-gpu-chrome.md) | Native shell per OS, our own GPU-drawn chrome | Accepted; confirmed by a survey of UI toolkits on 3 November 2026 |
| [0005](adr/0005-native-renderers.md) | Native renderers (Metal, OpenGL), one surface per window | Accepted; OpenGL in F7, per 0011; the cursor's layer, per 0021; two drawables, per 0022; blinking text's layer, per 0023 |
| [0006](adr/0006-text-pipeline.md) | Text pipeline: font backends, procedural sprites, UCD width tables | Accepted; shaping superseded by 0011; a font of symbols before the system's fallback, per 0025 |
| [0007](adr/0007-own-vt-core.md) | Our own VT core: table-driven parser, paged grid | Accepted |
| [0008](adr/0008-separate-package-manager.md) | Sleip as a separate program; no networking code in the app | Accepted; plugin builds became checks, per 0012; the app's updates through Sparkle, its one network client, per 0026; registry v0 detailed by 0031; the plugins' network after 1.0.0, per 0038 |
| [0009](adr/0009-toml-config.md) | TOML config with schemas, all-or-nothing reload and migrations | Accepted |
| [0010](adr/0010-mit-license.md) | MIT license | Accepted; to be reconfirmed before the first public release, 1.0.0, per 0027 |
| [0011](adr/0011-macos-only-1-0.md) | Version 1.0 targets macOS only; Linux after 1.0; CoreText shaping on macOS | Accepted; Intel question settled by 0013 |
| [0012](adr/0012-luau-plugins.md) | Third-party plugins in Luau, with its native code generator | Accepted |
| [0013](adr/0013-platform-baseline.md) | Platform baseline: macOS 26 or later, Apple Silicon only | Accepted |
| [0014](adr/0014-names-bolt-and-crossbow.md) | Names: Bolt for the terminal, Crossbow for the package manager | Superseded by 0018 |
| [0015](adr/0015-vt-core-throughput-exception.md) | Throughput of the VT core: vtebench's bottom region, with its lines of one character, is an exception | Accepted |
| [0016](adr/0016-new-names.md) | New names: Shard for the package manager; the terminal's still to choose, by the end of F2 | Superseded by 0018, never accepted |
| [0017](adr/0017-mcp-and-agents.md) | An MCP server, and the AI agents that run in the terminal; phase F3b | Accepted |
| [0018](adr/0018-names-lokyr-and-sleip.md) | Names: Lokyr for the terminal, Sleip for the package manager | Accepted |
| [0019](adr/0019-commands-errors-and-permissions.md) | Commands: a flat schema of arguments, errors with codes and categories, and the commands that plugins run, declared and checked; four new capabilities | Accepted; the commands that the user runs through a plugin, per 0024; the capabilities of the workspace, the views, the decorations and the pointer, per 0039 |
| [0020](adr/0020-localization.md) | Localization: every text that Lokyr shows, in Fluent catalogs, English and Italian first; the system's preferred language, or the config's | Accepted |
| [0021](adr/0021-drawables-and-the-cursor-layer.md) | Three drawables per window, of which a hidden window keeps one; the cursor's cell on a layer of its own, which the window server blinks | Accepted; amends 0005; its drawables superseded by 0022; blinking text on a layer, per 0023 |
| [0022](adr/0022-two-drawables.md) | Two drawables per window, as measured at 60 and 120 Hz; three as a build's feature | Accepted; supersedes 0021's drawables |
| [0023](adr/0023-blinking-text-layer.md) | Blinking text on a layer of its own, which the window server blinks, as it blinks the cursor's | Accepted; amends 0021 |
| [0024](adr/0024-user-commands.md) | The commands that the user runs through a plugin, as the palette's: the capability `user-commands`, which also reads the whole registry | Accepted; amends 0019 |
| [0025](adr/0025-symbols-font.md) | A font of symbols in the binary, Symbols Nerd Font Mono, for the icons of the private use areas that the configured font lacks | Accepted; amends 0006 |
| [0026](adr/0026-app-updates.md) | The app updates itself through Sparkle 2, from a signed feed on the repository's GitHub releases | Accepted; amends 0008; its feed at the project's address, per 0029; its notes in English, per 0030 |
| [0027](adr/0027-version-numbers.md) | Version numbers: releases, builds of a date and candidates; F3 from 0.2 to the first public release, 1.0.0; the plugin API's own version | Accepted; replaces the roadmap's versions; F3's blocks' minors in their new order, per 0029; F4 before 1.0.0, per 0036 |
| [0028](adr/0028-plugin-calls-as-messages.md) | The plugins' calls as messages, with every plugin's changes queued, and their UI as data, whose widgets keep their state in the host | Accepted; amends 0003 |
| [0029](adr/0029-update-feed-and-releases.md) | The updates' feed at `lokyr.dev`, builds' numbers that grow with each release, Sparkle in the app, and the distribution as F3's second block | Accepted; amends 0026 and 0027; F4's blocks 0.8 and 0.9, per 0036; the plugins' network after 1.0.0, per 0038 |
| [0030](adr/0030-release-notes-in-english.md) | The release notes in English alone, and the feed of Sparkle's `generate_appcast` | Accepted; amends 0026 |
| [0031](adr/0031-registry-v0.md) | Registry v0: a repository of the index, publishing by pull request, the archives in its releases, the index and the site on GitHub Pages, and a root key in Sleip that vouches for the index's keys | Accepted; details 0008 |
| [0032](adr/0032-plugins-config-and-state.md) | The plugins' config in `[plugins.<namespace>]` sections of the config file, checked against their JSON Schemas of the keywords that the host checks, a wrong value refusing its section alone; the plugins' states in files of their own | Accepted; the host's key `enabled`, per 0033 |
| [0033](adr/0033-plugins-turned-off-in-their-sections.md) | A plugin turned off by `enabled = false` in its section, at once, and on again a new one of it | Accepted; amends 0032 |
| [0034](adr/0034-themes-native-format-and-itermcolors.md) | Themes: a native format of TOML, a file a variant, colors alone; `.itermcolors` the one format imported, read as iTerm2 reads it and never converted; the themes of permissive licenses held, converted as the app builds; the user's in a folder, installed from the Finder | Accepted; its collection with Sleip amended by 0042 |
| [0035](adr/0035-themes-are-the-hosts.md) | Themes are the host's: `[theme]` in the config, the themes' files and `.itermcolors` read by the app, no `themes` plugin; the palette's picker writes `[theme]` | Accepted; amends 0002 |
| [0036](adr/0036-plugin-ecosystem-before-1-0.md) | The plugin ecosystem, F4, whole, before 1.0.0, as 0.8 and 0.9, `snap` and `awake` published; semantic versioning from 1.0.0; the settings' panel in F3b; the agents on their tabs in the daily features | Accepted; amends 0027 and 0029; the agents on their tabs after F4, per 0037; F4's scope per 0038; its `plugin-manager`'s panel after 1.0.0, per 0046 |
| [0037](adr/0037-agents-on-their-tabs-before-1-0.md) | The agents on their tabs, F3-63, right after F4's third-party plugins, before 1.0.0, which shows them; 0.7 without them | Accepted; amends 0036 |
| [0038](adr/0038-plugin-ecosystem-scope.md) | F4's scope: no network nor processes for plugins before 1.0.0, plugins reloaded at once by a watch of their files, Sleip's commands of registry v0, the registry public from 0.9 | Accepted; amends 0008, 0029 and 0036; its packages of themes in F3b, per 0042 |
| [0039](adr/0039-workspace-view-decorate-pointer-capabilities.md) | The capabilities `workspace-write`, `pane-view`, `decorate` and `pointer-read`, of the requests that needed none; the capabilities of a parameter by what it covers; `net:` and `process-spawn` refused before 1.0.0 | Accepted; amends 0019 |
| [0040](adr/0040-appearance-write-capability.md) | The capability `appearance-write`, of a medium risk: the themes' and the font's preview and choice in the config, the themes' moved from `user-commands`; the font picker a third-party plugin | Accepted; amends 0024 and 0039 |
| [0041](adr/0041-save-panel-of-no-capability.md) | A file saved through the user's save panel, of no capability: the host shows the panel and writes the bytes, the plugin learning the path alone; `snap` without `fs-write` | Accepted |
| [0042](adr/0042-themes-in-the-registry.md) | A guide for theme authors before 0.9, of every key, a full example and the install; themes in the registry in F3b, as 1.1: packages of kind `themes` of the same registry and site, Sleip's add and publish, each page's preview drawn of the theme's colors | Accepted; amends 0034 and 0038 |
| [0043](adr/0043-keep-awake-title-buttons-ticked-items.md) | The capability `keep-awake`, of a high risk: the system, or the system and the display, kept from idle sleep, of one power assertion that the host holds and releases with its plugins; the slot `title-buttons`, right after the window's buttons; a plugin's menu items ticked | Accepted; its titlebar's buttons per 0044 |
| [0044](adr/0044-titlebar-buttons-icons-and-overflow.md) | The titlebar's buttons beside the window's: one button of an icon alone a plugin, of the host's size, three at most, of more two and an ellipsis whose menu holds the rest | Accepted; amends 0043 |
| [0045](adr/0045-packages-service.md) | The packages' service: the capability `packages`, of a very high risk, Lokyr's own plugins' alone; a job of the bundle's Sleip of `--json` a call, its lines and end as events to its plugin, and its cancel | Accepted |
| [0046](adr/0046-plugin-manager-after-1-0.md) | The `plugin-manager`'s panel after 1.0.0, in F3b beside the settings' panel; Sleip's command line alone before it; the packages' service kept | Accepted; amends 0036 |

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
