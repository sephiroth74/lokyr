# Plugins

Every feature that you see in Lokyr beyond the terminal itself comes from a plugin: some from other authors, installed from the registry at [pkg.lokyr.dev](https://pkg.lokyr.dev), and the bundled ones, which come with the app. Both kinds run on the same plugin API.

## Third-party plugins

A third-party plugin is a script in [Luau](https://luau.org) that runs in a sandbox of its own, with a memory limit and a deadline for each call. It reaches only what its manifest declares, its [capabilities](reference.md#capabilities): Lokyr asks you for them the first time the plugin starts, refusing unless you allow them, and keeps your answers in `permissions.toml`, beside your config, where you can take one back at any time, which stops the plugin at once.

Sleip, Lokyr's package manager, installs plugins from the registry, checking each one against the registry's signed index, and tells you the capabilities that a plugin asks for before it installs it:

```sh
sleip search                  # every plugin of the registry
sleip info sephiroth74/snap   # its versions, capabilities and the commands it runs
sleip add sephiroth74/snap    # installs it
sleip update                  # takes the new versions
sleip remove sephiroth74/snap
```

Lokyr loads a plugin as it is installed, updated or removed, without restarting. The Homebrew cask puts `sleip` on your `PATH`; with the disk image, Lokyr › Install Command Line Tools does. Every command and its options are in [Sleip and the registry](../PACKAGE-MANAGER.md#commands).

Some of the registry's plugins:

| Plugin | What it gives you | Capabilities |
|---|---|---|
| [`sephiroth74/snap`](https://pkg.lokyr.dev/plugins/sephiroth74/snap/) | An image of the terminal's text, of the selection or the screen, in a frame as a macOS window's, to copy or to save | `clipboard-write`, `process-read`, `terminal-read` |
| [`sephiroth74/awake`](https://pkg.lokyr.dev/plugins/sephiroth74/awake/) | Keeps the Mac awake from a button beside the window's buttons: always, or while a tab runs a program | `keep-awake`, `process-read` |
| [`sephiroth74/font-picker`](https://pkg.lokyr.dev/plugins/sephiroth74/font-picker/) | Chooses the terminal's font from the system's monospaced families, each previewed as you move to it | `appearance-write` |

To write a plugin of your own and publish it on [pkg.lokyr.dev](https://pkg.lokyr.dev), follow [Writing a plugin](authoring.md), and look up the capabilities, the slots, the events and the limits in [the plugin reference](reference.md).

## The bundled plugins

The plugins below come with the app. They are built into its binary, run on the same plugin API as third-party plugins, and each can be turned off in your config with `enabled = false` in its section, as [`[plugins.<namespace>]`](../CONFIGURATION.md#pluginsnamespace) tells.

| Plugin | What it gives you |
|---|---|
| [`keymap-macos`](keymap-macos.md) | The default shortcuts of macOS's apps: ⌘N, ⌘C, ⌘V, ⌘T, ⌘F and the rest |
| [`core-actions`](core-actions.md) | Copy and paste, select all, clear, scrolling, and the text's size |
| [`links`](links.md) | ⌘-click opens a link, a URL or a path |
| [`tabs`](tabs.md) | The tab bar, and the tabs' commands |
| [`splits`](splits.md) | Split panes, and the commands that move between them, resize and zoom them |
| [`search`](search.md) | The find bar: a pane's search, its matches highlighted |
| [`command-palette`](command-palette.md) | The command palette: every command, ranked as you type |
| [`quick-terminal`](quick-terminal.md) | The quick terminal: a window that a global key slides in over every app |
| [`progress`](progress.md) | The programs' progress, in a thin bar across the window's top |
| [`command-header`](command-header.md) | The command whose output you are looking at, at the pane's top |
| [`copy-mode`](copy-mode.md) | The copy mode: a cursor that vi's keys move through the scrollback, to select and copy without the mouse |
| [`quick-tips`](quick-tips.md) | A tip at the start, once a week at most, in a card at the window's bottom right |

The order of the table is the order in which they start, which matters for the keys alone: a later plugin's key wins over an earlier one's, and your own `[keys]` win over all of them.

Turning a plugin off takes away its commands, its keys, its bars and its panels at once, as you save the file; `true` turns it on again. Without `tabs`, no tab bar shows and no key opens a tab; without `keymap-macos`, the commands keep working from the menus and from your own `[keys]`, but none has a key by default.
