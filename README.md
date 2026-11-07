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

> [!IMPORTANT]
> **Lokyr is not ready for daily use yet.** Its terminal core is done, and its macOS app has tabs, splits, search and themes; the command palette and third-party plugins come next. See the [status](#status).

<p align="center">
  <img src="docs/images/lokyr-code.png" alt="A Lokyr window of 100 by 30 cells, showing Rust source with syntax highlighting, by bat" width="760">
</p>

## Why Lokyr

- **A small core, and plugins for the rest.** The tab bar, splits, search and the command palette are plugins, on the same API that third parties get. The core keeps the terminal itself: the screen, the input, the rendering. Leave a plugin out, and Lokyr is a terminal without it.
- **Plugins you can trust.** A third-party plugin is Luau source, in a VM of its own, with a memory limit and a deadline. It reaches the screen, the clipboard, files or the network only through the capabilities that it declares, which Sleip shows at install and Lokyr asks for on first use. The app itself contains no networking code.
- **Fast where you feel it.** Its own VT core, with 8-byte cells and a paged grid; a Metal renderer, one surface per window; a frame of steady output in about a quarter of a millisecond at 4K. It is measured against six other terminals, each change against the builds before it, and the numbers are [in the repository](docs/bench/).
- **Native on macOS.** An AppKit window, CoreText shaping and font fallback, input methods, Secure Keyboard Entry, a quick terminal on a global key.
- **Ready for agents.** Lokyr will be an [MCP server](docs/adr/0017-mcp-and-agents.md): an agent works in its own pane and the panes it opens, and the tabs show what each agent is doing.

## Status

Lokyr is built in phases, each with exit criteria that are measured, not assumed ([roadmap](docs/ROADMAP.md)):

| Phase | What it brings | Status |
|---|---|---|
| F0 | Foundations: the workspace, CI, the measuring tools, the plugin runtime chosen | ✅ |
| F1 | The headless terminal core, checked against xterm, `alacritty_terminal` and fuzzers | ✅ |
| F2 | The macOS app, a window of one pane | 🚧 |
| F3 | The macOS MVP: tabs, splits, search, themes, the command palette | 🚧 |
| F3b | MCP and agents | |
| F4 | Third-party plugins in Luau, Sleip and its registry | |
| F5 | Shell integration, iTerm2's power features, images | |
| F6 | 1.0 | |
| F7 | Linux | |

## Performance

Lokyr against Ghostty, Alacritty, kitty, WezTerm, iTerm2 and Terminal.app, under the same conditions: Menlo 13 in 120 × 40 cells, the same plain shell, release builds, on an M5 Pro. The [full report](docs/bench/compare-F2-134.md) has every measure, its method and its limits.

<p align="center">
  <img src="docs/bench/compare-F2-134/charts/startup.svg" alt="Startup to the first window: Lokyr 256 ms, Ghostty 331, Terminal.app 340, Alacritty 349, WezTerm 394, kitty 672, iTerm2 794" width="680">
  <img src="docs/bench/compare-F2-134/charts/memory.svg" alt="Memory at rest, without the GPU's: Lokyr 25.0 MB, Alacritty 31.3, Terminal.app 34.8, WezTerm 37.2, Ghostty 40.0, kitty 97.3, iTerm2 136.5" width="680">
  <img src="docs/bench/compare-F2-134/charts/wakeups.svg" alt="Wakeups at rest a second: Alacritty 0.37, Lokyr 0.38, Terminal.app 0.38, WezTerm 1.35, iTerm2 8.18, Ghostty 16.57, kitty 20.08" width="680">
  <img src="docs/bench/compare-F2-134/charts/throughput.svg" alt="Throughput of cat of 1 GiB on screen: Ghostty 306 MiB/s, Lokyr 296, Alacritty 170, kitty 147, iTerm2 51, WezTerm 45, Terminal.app 40" width="680">
  <img src="docs/bench/compare-F2-134/charts/energy.svg" alt="The CPU's energy to show cat of 1 GiB: Lokyr 17.3 J, kitty 41.1, Alacritty 41.9, Ghostty 43.1, iTerm2 107.9, Terminal.app 124.3, WezTerm 196.3" width="680">
  <img src="docs/bench/compare-F2-134/charts/latency-120hz.svg" alt="Latency at 120 Hz, a key to its echo on screen with Typometer, median and 99th percentile: Alacritty 21.6 and 25.8 ms, kitty 21.8 and 29.6, Lokyr 21.2 and 32.2, iTerm2 21.3 and 45.0, Ghostty 24.8 and 47.7" width="680">
  <img src="docs/bench/compare-F2-134/charts/latency-60hz.svg" alt="Latency at 60 Hz, a key to its echo on screen with Typometer, median and 99th percentile: Alacritty 21.2 and 47.9 ms, Lokyr 22.8 and 50.3, kitty 32.0 and 50.5, iTerm2 35.8 and 50.6, Ghostty 33.1 and 64.7" width="680">
</p>

## What works today

<p align="center">
  <img src="docs/images/lokyr-htop.png" alt="htop in a Lokyr window of 100 by 30 cells, with the tree of the window's own processes: lokyr, login, bash and htop" width="760">
</p>

- ✅ Windows drawn with Metal, at the display's refresh rate, their frames only when something changed.
- ✅ Tabs, in a bar at the top, at the bottom or down a side, in three looks, dragged between windows and onto panes, a program's progress round its tab.
- ✅ Split panes, their dividers dragged, the panes without the keyboard faded.
- ✅ A find bar that highlights every match, through 100 000 lines in about 25 ms.
- ✅ Fonts through CoreText: fallback, ligatures, bold and italic faces or their synthesis, emoji, box drawing and powerline symbols drawn to the cell.
- ✅ xterm's sequences, the kitty keyboard protocol, synchronized output, OSC 8 links, OSC 52 with consent, Unicode 18's grapheme clusters and widths.
- ✅ Input methods, Secure Keyboard Entry, Option as Meta, chords and leaders in the keys.
- ✅ Selection, copy on select, paste with a check of what it holds, drag and drop of files.
- ✅ A quick terminal on a global key, from any edge of the screen.
- ✅ Notifications, the bell, smooth cursor blinking that the window server draws.
- ✅ A config in TOML, applied as it is saved, its errors shown in the window.
- ✅ English and Italian, as the system's language is, or as the config says.

## Bundled plugins

Every feature beyond the terminal itself is a plugin, and these come with the app, built into it, on the same API that third-party plugins will get. Each can be turned off with `enabled = false` in its section of the config, and has its own keys there.

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

Each page tells the plugin's commands, their keys, its config and what turning it off takes away; [the plugins' index](docs/plugins/README.md) tells how they fit together.

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

Lokyr reads `~/.config/lokyr/config.toml`, and applies it as you save it. `lokyr +show-config --default --docs` prints every key with its default; the [configuration guide](docs/CONFIGURATION.md) explains each.

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

## Programs' progress

Lokyr shows the progress that a program reports with OSC 9;4 on the program's tab, round its outline, and in a thin bar across the window's top. Some programs send it only to the terminals that they recognize by their environment's variables, and do not recognize Lokyr yet, whose `TERM_PROGRAM` is `lokyr`. Until they do, turn their progress on as the table tells:

| Program | Sends its progress by itself to | To get it in Lokyr |
|---|---|---|
| Gradle 9.8 | ConEmu, Ghostty, kitty, and iTerm2 from 3.6.6 | `export ConEmuPID=1` in your shell's profile, as Gradle has no setting of its own |
| Claude Code 2.1 | ConEmu, Ghostty from 1.2, and iTerm2 from 3.6.6, while its `terminalProgressBarEnabled` is on, as it is by default | `export ConEmuPID=1` in your shell's profile |
| cargo 1.99 | Ghostty, WezTerm, Windows Terminal and ConEmu | `term.progress.term-integration = true` in `~/.cargo/config.toml`, or `CARGO_TERM_PROGRESS_TERM_INTEGRATION=true` in the environment |

`ConEmuPID` is the variable of ConEmu, a terminal for Windows: a program that reads it may take Lokyr for ConEmu, which is rare on macOS, but if a program behaves oddly, remove the variable first. Homebrew, npm, pip, uv, Go and Maven send no progress, in any terminal. The table holds the versions checked in November 2026: Gradle 9.8.2, Claude Code 2.1.318 and cargo 1.99.1.

## Documentation

- [The documentation's index](docs/README.md), with the requirements and the decisions.
- [Architecture](docs/ARCHITECTURE.md): the crates, the threads, the frame's way.
- [The plugin system](docs/PLUGIN-SYSTEM.md) and [Sleip](docs/PACKAGE-MANAGER.md).
- [Performance](docs/PERFORMANCE.md): the budgets, and the [measures](docs/bench/).
- [The decisions](docs/adr/README.md), one record each.

## Supporting Lokyr

If you like Lokyr, or what it aims for, a coffee helps to keep it going:

<a href="https://www.buymeacoffee.com/sephiroth74"><img alt="Buy me a coffee" src="https://img.buymeacoffee.com/button-api/?text=Buy%20me%20a%20coffee&emoji=&slug=sephiroth74&button_colour=BD5FFF&font_colour=ffffff&font_family=Cookie&outline_colour=000000&coffee_colour=FFDD00"></a>

## Contributing

Contributions are welcome: [CONTRIBUTING.md](CONTRIBUTING.md) tells how to set up the project and what a pull request needs. Everyone who takes part follows the [code of conduct](CODE_OF_CONDUCT.md).

### Translating

Lokyr speaks English and Italian, and would like to speak yours. Its texts are in [Project Fluent](https://projectfluent.org) catalogs, one set for each language, which need no Rust to translate: [TRANSLATING.md](TRANSLATING.md) tells what there is to translate, how to improve a translation, and how to add a language.

## License

[MIT](LICENSE)
