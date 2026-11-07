# The bundled plugins

Every feature that you see in Lokyr beyond the terminal itself comes from a plugin, and the plugins below come with the app. They are built into its binary, run on the same plugin API that third-party plugins will get, and each can be turned off in your config with `enabled = false` in its section, as [`[plugins.<namespace>]`](../CONFIGURATION.md#pluginsnamespace) tells.

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

The order of the table is the order in which they start, which matters for the keys alone: a later plugin's key wins over an earlier one's, and your own `[keys]` win over all of them.

Turning a plugin off takes away its commands, its keys, its bars and its panels at once, as you save the file; `true` turns it on again. Without `tabs`, no tab bar shows and no key opens a tab; without `keymap-macos`, the commands keep working from the menus and from your own `[keys]`, but none has a key by default.
