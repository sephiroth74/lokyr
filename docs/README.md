# Lokyr's documentation

Lokyr is a fast terminal for macOS, written in Rust. Every feature that you see, such as the tabs, the splits and the find bar, is a plugin, and you can add plugins of your own, written in Luau. These pages show you how to use Lokyr, set it up and extend it.

## Start here

- **Install Lokyr** from its disk image, or with Homebrew, as [the project's page](../README.md#install) shows. Lokyr runs on macOS 26 or later, on Apple Silicon.
- **See what it looks like** in the [screenshots](screenshots.md): splits and tabs, the command palette, find, a program's progress, themes and inline images.
- **Set it up** in Lokyr › Open Settings Window, which changes your config file for you, or edit the file itself: Lokyr › Settings…, ⌘,, opens it. The [configuration guide](CONFIGURATION.md) tells every setting.

## Use Lokyr

- [Shell integration](shell-integration.md): how your shell reports its prompts and commands, so that you can jump between them, see which failed, and click in the command line.
- [Blocks](blocks.md): each command and its output as a block that you select, copy and jump between, under a pinned prompt; off by default.
- [AI agents](agents.md): what a tab shows of the AI coding agents that run in it, and how to find the one that needs you.
- [Progress and status](progress-and-status.md): a program's progress on its tab and across the window's top, and how to get it from Gradle, Claude Code and cargo.
- [Notifications](notifications.md): the notifications of programs, agents, long commands and the bell, and how to allow them.
- [Mouse and selection](mouse-and-selection.md): selecting and copying text, ⌘-click on links and paths, the context menu, scrolling, and programs that take the mouse.
- [Windows and tabs](windows-and-tabs.md): opening, moving between and restoring windows, and the tabs' drags, titles, colors, pins and groups.
- [Floating panes and popups](floating-panes.md): panes that float over a tab's splits, and popups that run a command, such as `lazygit`, and close when it ends. · Since 1.5
- [Sessions](sessions.md): groups of windows, such as one for each project, that you switch between, with the programs of the hidden ones running. · Since 1.5
- [Layouts](layouts.md): a session saved to a file, which opens again as a new session with new shells. · Since 1.5
- [Links](links.md): ⌘-click on URLs, paths and programs' links, a path's line in your editor, and hints from the keyboard, with patterns of your own.
- [Automation](automation.md): AppleScript and the Shortcuts app driving Lokyr, such as a window that opens in a folder and runs a command, and Lokyr's actions in Spotlight. · Since 1.6
- [Plugins](plugins/README.md): the plugins that come with Lokyr, a page each, such as the [tab bar](plugins/tabs.md), the [splits](plugins/splits.md), [find](plugins/search.md), [links and hints](plugins/links.md) and the [quick terminal](plugins/quick-terminal.md); and how to install more with Sleip.

## Reference

- [Configuration](CONFIGURATION.md): every setting, its values and its default.
- [Commands](commands.md): every command, its arguments and its default keys, to bind your own.
- [Changelog](../CHANGELOG.md) and [release notes](releases/README.md): what changed in each version.
- [Writing a plugin](plugins/authoring.md) and the [plugin reference](plugins/reference.md), for plugin authors.
- [Making a theme](themes/authoring.md), for theme authors.

## How Lokyr is built

For contributors: the [architecture](ARCHITECTURE.md), the [requirements](requirements.md) and their solutions, the [roadmap](ROADMAP.md), and a page for each part: the [terminal core](VT-CORE.md), the [user interface](UI-AND-FEATURES.md), the [plugin system](PLUGIN-SYSTEM.md), the [package manager](PACKAGE-MANAGER.md), [security](SECURITY.md) and [performance](PERFORMANCE.md), with its [measurements](bench/RELEASES.md). [Contributing](../CONTRIBUTING.md) and [translating](../TRANSLATING.md) tell you how to take part.
