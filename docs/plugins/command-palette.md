# `command-palette`: the command palette

<sub>`lokyr/command-palette` · bundled · [source](../../plugins/command-palette)</sub>

A panel at the top of the window that lists Lokyr's commands, the plugins' too, ranks them as you type, and runs the one you choose: a large field, «Execute Command…», above a thin line and the list.

## The palette

- **⇧⌘P** opens it, its field taking the keys; ⇧⌘P on an open palette gives its field the keys again.
- **It lists** the commands that run without an argument and that the menus enable now: Copy shows only where there is a selection, as its menu item does. Each shows its title, its category below it, and its keys at its end.
- **As you type**, it ranks them by a fuzzy match of your letters in their order, by each command's title first, then its name, as `tabs.new`, its keys, as `⌘T`, and its plugin: `nt` finds New Tab, `⌘F` finds Find…. A word's first letters and letters in a row count more.
- **The commands that you ran last** come first while the field is empty, and before the others of the same match as you type; Lokyr keeps the last twenty.
- **↑ and ↓** move through the list while you type; **Return**, or a click on a command, runs it and closes the palette. The command under the pointer is highlighted.
- **Escape**, or a click outside it, closes it, running nothing.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `command-palette.open` | View › Command Palette… | ⇧⌘P | Opens the palette in the window that has the focus |
| `command-palette.move` | — | ↑ and ↓ in the palette | Moves the list's selection, by its argument `delta`, a whole number of items |

## Turning it off

```toml
[plugins.command-palette]
enabled = false
```

The palette and its commands go, with their keys: ⇧⌘P then does nothing in Lokyr.

## Permissions

It runs any command as yours, as the menus and the keys do, and reads the whole list of the commands, with each one's plugin, keys and context: the capability `user-commands`, of a very high risk, as running any command can type into the shell. A palette of a third party would ask for it when you install it.
