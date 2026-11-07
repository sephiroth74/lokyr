# `keymap-macos`: the default shortcuts

<sub>`lokyr/keymap-macos` · bundled · macOS · [source](../../plugins/keymap-macos)</sub>

The keys that macOS's apps have taught you, bound to Lokyr's commands and to the other bundled plugins': ⌘N opens a window, ⌘C copies, ⌘T opens a tab, ⌘F finds, a ⌘-click opens a link. It has no command of its own: it only binds keys, which the menus then show beside their items.

## What it binds

| Keys | Command |
|---|---|
| ⌘, | `app.settings`: opens your config file |
| ⌘Q | `app.quit` |
| ⌘N, ⇧⌘W, ⇧⌘I, ⌥⌘1 to ⌥⌘9 | `window.new`, `window.close`, `window.set-title`, `window.goto` |
| ⌘C, ⌘V, ⌘A, ⌘K | `pane.copy`, `pane.paste`, `pane.select-all`, `pane.clear` |
| ⌘=, ⌘+, ⌘-, ⌘0 | `font.bigger`, `font.smaller`, `font.reset` |
| ⌘↑, ⌘↓, ⌘Page Up, ⌘Page Down, ⌘Home, ⌘End | scrolling, by a line, a page, or to the top and the bottom |
| ⌘-click | `links.open` |
| ⌘F, ⌘G, ⇧⌘G | `search.open`, `search.next`, `search.previous` |
| ⌘T, ⌘W, ⇧⌘], ⇧⌘[, ⌃Tab, ⌃⇧Tab, ⌘1 to ⌘9, ⇧⌘← and ⇧⌘→ | the tabs: new, close, next, previous, by number, move |
| ⌘D, ⇧⌘D, ⌥⌘ and an arrow, ⌘], ⌘[, ⌃⌘ and an arrow, ⇧⌘Return | the splits: split right or down, the pane in a direction, the next and the previous pane, resize, zoom |

The [keys' table in the configuration guide](../CONFIGURATION.md#bindings) has every binding, with what each command does.

## Your own keys

Your `[keys]` come after these bindings, and win: bind a key to another command, or take a default away with `false`.

```toml
[keys]
"cmd+d" = false                       # no split on ⌘D
"cmd+shift+t" = "tabs.new"            # a second key for a new tab
```

A default of a command that is not there takes no key: with a plugin turned off, as `[plugins.links] enabled = false`, its keys go back to what they would do without it, and a ⌘-click selects, as a click does.

## Turning it off

```toml
[plugins.keymap-macos]
enabled = false
```

Every default key goes, so that your `[keys]` bind every key from nothing. The menus keep the system's own keys alone: ⌘H hides Lokyr, ⌥⌘H the other apps, ⌘M minimizes the window, and ⌃⌘Space shows the emoji and symbols.

## Permissions

None: it binds keys, and reads nothing of your terminals.
