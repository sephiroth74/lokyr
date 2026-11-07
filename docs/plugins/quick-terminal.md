# `quick-terminal`: a terminal from every app

<sub>`lokyr/quick-terminal` · bundled · [source](../../plugins/quick-terminal)</sub>

A window that a global key slides in from an edge of the screen, over every app, the ones in full screen too, and out again: a terminal at hand from anywhere.

## The quick terminal

- **Its key** is yours to choose, one that works in every app, written after `global:` in [`[keys]`](../CONFIGURATION.md#bindings), which macOS gives Lokyr without the Accessibility permission. Pick one that no other app takes.
- **The key shows it** and gives it the keys; again, it hides it, and the app that had the focus takes it back; shown without the focus, it takes the focus.
- **Its shell runs while it is hidden**, so that it shows at once, where [`new-directory`](../CONFIGURATION.md#new-directory) says; closed, as its shell exits, the next key opens a new one.
- **It hides as it loses the focus**, as `autohide` says, but to a question of Lokyr's on it.
- **A notification of its shell**, clicked, shows it where it showed last.

```toml
[keys]
"global:ctrl+`" = "window.toggle-quick-terminal"
```

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `window.toggle-quick-terminal` | Shell › Quick Terminal | | Shows the quick terminal, opening it first, or hides it, as above |

## Configuration

Its keys go in [`[plugins.quick-terminal]`](../CONFIGURATION.md#pluginsquick-terminal), which was `[quick-terminal]` before:

| Key | Values | Default | What it does |
|---|---|---|---|
| `position` | `"top"`, `"bottom"`, `"left"`, `"right"`, `"center"` | `"top"` | The edge that it slides in from, or the centre, where it fades in |
| `size` | a percentage, from `"1%"` to `"100%"`, or points, as `"300px"`; a second after a comma | `"50%"` | How far it reaches into the screen, then its length along the edge |
| `screen` | `"main"`, `"mouse"`, `"menu-bar"` | `"main"` | The screen that it shows on |
| `animation` | from `0` to `2000` | `200` | How long it slides, in milliseconds |
| `autohide` | `true`, `false` | `true` | Whether it hides as it loses the focus |
| `spaces` | `"move"`, `"remain"` | `"move"` | Whether it follows you to the Space in front |

```toml
[plugins.quick-terminal]
position = "bottom"
size = "40%,80%"
```

## Turning it off

```toml
[plugins.quick-terminal]
enabled = false
```

The quick terminal goes, its window and its shell with it, and Shell › Quick Terminal too; a global key bound to its command does nothing.

## Permissions

It asks for the global keys and the quick windows, `global-hotkey`, to show its window over every app's.
