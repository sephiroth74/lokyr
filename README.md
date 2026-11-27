<p align="center">
  <img src="crates/app/resources/icons/lokyr-jormungandr.svg" alt="Lokyr's icon" width="128">
</p>

<h1 align="center">Lokyr</h1>

<p align="center">
  <b>A fast terminal for macOS, written in Rust, where every visible feature is a plugin.</b>
  <br>
  Third-party plugins are sandboxed Luau scripts, installed by <b>Sleip</b>, its package manager.
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#why-lokyr">Why Lokyr</a> ·
  <a href="#status">Status</a> ·
  <a href="#performance">Performance</a> ·
  <a href="#bundled-plugins">Plugins</a> ·
  <a href="#building">Building</a> ·
  <a href="docs/README.md">Documentation</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="#supporting-lokyr">Support</a>
</p>

<p align="center">
  <a href="https://github.com/sephiroth74/lokyr/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/sephiroth74/lokyr/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="macOS 26 or later, Apple Silicon" src="https://img.shields.io/badge/macOS-26%2B%20·%20Apple%20Silicon-lightgrey">
  <img alt="Rust 1.99" src="https://img.shields.io/badge/Rust-1.99-orange">
  <a href="LICENSE"><img alt="MIT license" src="https://img.shields.io/badge/license-MIT-blue"></a>
</p>

<p align="center">
  <a href="https://www.buymeacoffee.com/sephiroth74"><img alt="Buy me a coffee" src="https://img.buymeacoffee.com/button-api/?text=Buy%20me%20a%20coffee&emoji=&slug=sephiroth74&button_colour=BD5FFF&font_colour=ffffff&font_family=Cookie&outline_colour=000000&coffee_colour=FFDD00"></a>
</p>

> [!NOTE]
> **Lokyr 1.5.0 is out**, of sessions and layouts, input broadcast to every pane, floating panes and popups, and the scrollback kept when you quit: [install it](#install), and Lokyr updates itself. See the [status](#status) for what comes next.

<p align="center">
  <img src="docs/images/lokyr-floating-pane.png" alt="A Lokyr window of two panes, Rust source by bat beside a system monitor, and a floating pane over them running lazygit" width="760">
</p>

## Install

Lokyr runs on macOS 26 or later, on Apple Silicon. Download the DMG of the latest release from [the releases](https://github.com/sephiroth74/lokyr/releases), or install it with Homebrew:

```sh
brew install --cask sephiroth74/tap/lokyr
```

The cask links `lokyr` and `sleip` into Homebrew's `bin`, and leaves Lokyr's updates to Lokyr, which takes them through Sparkle. `brew uninstall --zap --cask lokyr` takes away the app, its config, its plugins and its caches.

## Why Lokyr

- **A small core, and plugins for the rest.** The tab bar, splits, search and the command palette are plugins, on the same API that third parties get. The core keeps the terminal itself: the screen, the input, the rendering. Leave a plugin out, and Lokyr is a terminal without it.
- **Plugins you can trust.** A third-party plugin is Luau source, in a VM of its own, with a memory limit and a deadline. It reaches the screen, the clipboard, files or the network only through the capabilities that it declares, which Sleip shows at install and Lokyr asks for on first use. The app itself contains no networking code.
- **Fast where you feel it.** Its own VT core, with 8-byte cells and a paged grid; a Metal renderer, one surface per window; a frame of steady output in about a quarter of a millisecond at 4K. It is measured against six other terminals, each change against the builds before it, and the numbers are [in the repository](docs/bench/).
- **Native on macOS.** An AppKit window, CoreText shaping and font fallback, input methods, Secure Keyboard Entry, a quick terminal on a global key.
- **Ready for agents.** A tab tells what the AI agent in it is doing, as Claude Code, Codex or Gemini CLI: at work, waiting for you, or done, of no setup, as [`[agents]`](docs/CONFIGURATION.md#agents) tells, with a notification and the Dock's badge while you look elsewhere, and ⌥⌘A to the next one that waits.

## Status

Lokyr is built in phases, each with exit criteria that are measured, not assumed ([roadmap](docs/ROADMAP.md)):

| Phase | What it brings | Status |
|---|---|---|
| F0 | Foundations: the workspace, CI, the measuring tools, the plugin runtime chosen | ✔ |
| F1 | The headless terminal core, checked against xterm, `alacritty_terminal` and fuzzers | ✔ |
| F2 | The macOS app, a window of one pane | ✔ |
| F3 | The macOS MVP: tabs, splits, search, themes, the command palette, the agents' state on the tabs; then 1.0.0, the first release | ✔ |
| F4 | Third-party plugins in Luau, Sleip and its registry | ✔ |
| F3b | The settings window, then themes made in the app and the Keys page | ✔ |
| F5a | As 1.3 and 1.4: the programs' and agents' status of OSC 7501, a click at the prompt, the tabs' groups, hints; blocks of commands, smooth scrolling | ✔ |
| F5b | As 1.5 to 1.7: the scrollback restored, named sessions and layouts, broadcast, floating panes and popups; Shortcuts, AppleScript and the search's filter; tmux -CC, triggers and profiles | 1.5 ✔ |
| F5c | As 1.8: all of kitty's graphics and sixel, the cursor's trail, drag and drop | |
| F6 | Plugin API 1.0 and hardening | |
| F7 | After 1.0: Linux, a mux daemon, Windows | |

## Performance

Lokyr against Ghostty, Alacritty, kitty and iTerm2, under the same conditions: Menlo 13 in 120 × 40 cells, the same plain shell, release builds, on an M5 Pro. Every terminal was measured of its version of 21 November 2026, in one session, beside Lokyr of 1.4.0's code, as [the full report](docs/bench/compare-F5a-78.md) tells, with every measure, its method and its limits; the latency at 120 Hz, by Typometer, is of 1 November, of the versions of then ([its report](docs/bench/compare-F2-134.md)). Each release's own measures, against its budgets and the best of the others, are in [the releases' table](docs/bench/RELEASES.md).

<p align="center">
  <img src="docs/bench/compare-F5a-78/charts/startup.svg" alt="Startup to the first window: Lokyr 253 ms, Alacritty 271, Ghostty 304, iTerm2 598, kitty 644" width="680">
  <img src="docs/bench/compare-F5a-78/charts/memory.svg" alt="Memory at rest, without the GPU's: Lokyr 28.6 MB, Alacritty 31.3, Ghostty 39.6, kitty 99.6, iTerm2 144.0" width="680">
  <img src="docs/bench/compare-F5a-78/charts/wakeups.svg" alt="Wakeups at rest a second: Alacritty 0.33, Lokyr 0.35, iTerm2 7.48, Ghostty 15.22, kitty 20.07" width="680">
  <img src="docs/bench/compare-F5a-78/charts/throughput.svg" alt="Throughput of cat of 1 GiB on screen: Ghostty 306 MiB/s, Lokyr 294, Alacritty 170, kitty 140, iTerm2 100" width="680">
  <img src="docs/bench/compare-F5a-78/charts/energy.svg" alt="The CPU's energy to show cat of 1 GiB: Lokyr 16.8 J, kitty 42.2, Ghostty 43.5, Alacritty 43.7, iTerm2 201.6" width="680">
  <img src="docs/bench/compare-F5a-78/charts/latency-60hz.svg" alt="Latency at 60 Hz, a key to its echo on screen with Typometer, median and 99th percentile: Lokyr 22.4 and 29.5 ms, kitty 23.9 and 33.1, Alacritty 23.6 and 40.3, Ghostty 24.2 and 44.0, iTerm2 25.3 and 50.8" width="680">
  <img src="docs/bench/compare-F5a-78/charts/latency-120hz.svg" alt="Latency at 120 Hz on 1 November 2026, a key to its echo on screen with Typometer, median and 99th percentile: Alacritty 21.6 and 25.8 ms, kitty 21.8 and 29.6, Lokyr 21.2 and 32.2, iTerm2 21.3 and 45.0, Ghostty 24.8 and 47.7" width="680">
</p>

## What works today

<p align="center">
  <img src="docs/images/lokyr-tabs.png" alt="A Lokyr window of two tabs, the first running a build whose progress fills its tab, showing Rust source by bat between two prompts" width="760">
</p>

- ✔ Windows drawn with Metal, at the display's refresh rate, their frames only when something changed.
- ✔ Tabs, in a bar at the top, at the bottom or down a side, in three looks, dragged between windows and onto panes, a program's progress round its tab.
- ✔ Split panes, their dividers dragged, the panes without the keyboard faded.
- ✔ A find bar that highlights every match, through 100 000 lines in about 25 ms, and marks them along the panes' scroll bars, which show as macOS's do.
- ✔ Shell integration for zsh, bash and fish: jumps between prompts, a mark of each command's outcome, the command at the pane's top as its output scrolls, a notification as a long one ends, a click in the command line that moves the cursor there.
- ✔ Fonts through CoreText: fallback, ligatures, bold and italic faces or their synthesis, emoji, box drawing and powerline symbols drawn to the cell.
- ✔ xterm's sequences, the kitty keyboard protocol, synchronized output, OSC 8 links, OSC 52 with consent, Unicode 18's grapheme clusters and widths.
- ✔ Input methods, Secure Keyboard Entry, Option as Meta, chords and leaders in the keys.
- ✔ Inline images, of iTerm2's protocol and of kitty's graphics protocol, as `imgcat`, `kitten icat`, `chafa`, `timg` and `yazi` send them, within 64 MiB a pane.
- ✔ AI agents on their tabs, working, waiting for you or done, Claude Code's of its hooks, and ⌘-click on a path's line in your editor.
- ✔ Selection, copy on select, a copy mode of vi's keys through the scrollback, paste with a check of what it holds, drag and drop of files.
- ✔ A quick terminal on a global key, from any edge of the screen.
- ✔ Windows, tabs and splits that open again as Lokyr starts, each pane in its folder; New Lokyr Window Here and New Lokyr Tab Here in the Finder, and a folder dropped on the Dock's icon; Lokyr as the default terminal of scripts, `ssh` and man pages' links.
- ✔ Notifications, the bell, smooth cursor blinking that the window server draws.
- ✔ A config in TOML, applied as it is saved, its errors shown in the window.
- ✔ English and Italian, as the system's language is, or as the config says.

## Bundled plugins

Every feature beyond the terminal itself is a plugin, and these come with the app, built into it, on the same API as third-party plugins. Each can be turned off with `enabled = false` in its section of the config, and has its own keys there.

| Plugin | What it does |
|---|---|
| [`keymap-macos`](docs/plugins/keymap-macos.md) | The default shortcuts of macOS's apps: ⌘N, ⌘C, ⌘V, ⌘T, ⌘F and the rest |
| [`core-actions`](docs/plugins/core-actions.md) | Copy and paste, select all, clear, scrolling, and the text's size |
| [`links`](docs/plugins/links.md) | ⌘-click opens a link, a URL or a path, as a compiler's `src/main.rs:42` |
| [`tabs`](docs/plugins/tabs.md) | The tab bar: new, close, select, move, rename, color, and out to a window of its own |
| [`splits`](docs/plugins/splits.md) | Split panes: split, focus by direction, resize, zoom, equalize, swap |
| [`search`](docs/plugins/search.md) | The find bar: a pane's search as you type, its matches highlighted |
| [`command-palette`](docs/plugins/command-palette.md) | The command palette: every command, ranked as you type, the recent ones first |
| [`quick-terminal`](docs/plugins/quick-terminal.md) | The quick terminal: a window that a global key slides in from an edge of the screen, over every app |
| [`progress`](docs/plugins/progress.md) | The programs' progress, of OSC 9;4, in a thin bar across the window's top |
| [`command-header`](docs/plugins/command-header.md) | The command whose output you are looking at, at the pane's top, which a click scrolls to, while `blocks` is off |
| [`blocks`](docs/blocks.md) | Off by default: each command and its output a block, its prompt rows pinned at the pane's top, which a click scrolls to, and its parts copied, its details shown or its command run again from the context menu · Since 1.4 |
| [`copy-mode`](docs/plugins/copy-mode.md) | The copy mode: ⇧⌘C, then vi's keys move a cursor through the scrollback, select and copy, without the mouse |
| [`quick-tips`](docs/plugins/quick-tips.md) | A tip at the start, once a week at most: a shortcut as your keymap binds it, or a feature that shows in no menu |
| [`settings`](docs/plugins/settings.md) | The settings window's Settings page: every key of the config and of the plugins, by section, of its value in force |
| [`plugin-manager`](docs/plugins/plugin-manager.md) | The settings window's Plugins page: every plugin, its state and its capabilities, to disable, enable or remove |

Each page tells the plugin's commands, their keys, its config and what turning it off takes away; [the plugins' index](docs/plugins/README.md) tells how they fit together.

To write a plugin of your own, in Luau, and publish it on [pkg.lokyr.dev](https://pkg.lokyr.dev), follow [Writing a plugin](docs/plugins/authoring.md).

## Building

Lokyr needs macOS 26 or later on Apple Silicon, Xcode 27 with its Metal toolchain, and [just](https://github.com/casey/just). `rust-toolchain.toml` pins Rust.

```sh
git clone https://github.com/sephiroth74/lokyr.git
cd lokyr
just bundle             # target/Lokyr.app
open target/Lokyr.app
```

`just ci` runs what CI runs: formatting and clippy, the build and the tests, the Linux check of the portable crates, the dependencies' licenses, the comments and the docs' links, and the bundle.

## Configuration

Lokyr reads `~/.config/lokyr/config.toml`, and applies it as you save it. `lokyr +show-config --default --docs` prints every key with its default; the [configuration guide](docs/CONFIGURATION.md) explains each, and [Making a theme](docs/themes/authoring.md) tells how to write a theme of your own colors.

```toml
[font]
family = "JetBrains Mono"
size = 14

[window]
background-opacity = 0.9
background-blur = 20

[plugins.quick-terminal]
position = "top"
```

## Progress and notifications

Lokyr shows the progress that a program reports on its tab, round its outline, and in a thin bar across the window's top: [Progress and status](docs/progress-and-status.md) tells how to get it from Gradle, Claude Code and cargo, which send it only to the terminals that they recognize. A program, an AI agent or a long command can also let you know when it needs you, in a notification of macOS's: [Notifications](docs/notifications.md) tells each kind and how to allow them.

## Documentation

- [The documentation's home](docs/README.md), which leads to every page, and [the requirements and their solutions](docs/requirements.md).
- [The changelog](CHANGELOG.md): every release's changes.
- [Architecture](docs/ARCHITECTURE.md): the crates, the threads, the frame's way.
- [Writing a plugin](docs/plugins/authoring.md) and [making a theme](docs/themes/authoring.md): the guides for their authors.
- [The plugin system](docs/PLUGIN-SYSTEM.md) and [Sleip](docs/PACKAGE-MANAGER.md).
- [Performance](docs/PERFORMANCE.md): the budgets, and the [measures](docs/bench/).

## Contributing

Contributions are welcome: [CONTRIBUTING.md](CONTRIBUTING.md) tells how to set up the project and what a pull request needs. Everyone who takes part follows the [code of conduct](CODE_OF_CONDUCT.md).

### Translating

Lokyr speaks English and Italian, and would like to speak yours. Its texts are in [Project Fluent](https://projectfluent.org) catalogs, one set for each language, which need no Rust to translate: [TRANSLATING.md](TRANSLATING.md) tells what there is to translate, how to improve a translation, and how to add a language.

## License

[MIT](LICENSE)
