# `command-header`: the command's header

<sub>`lokyr/command-header` · bundled · [source](../../plugins/command-header)</sub>

The command whose output you are looking at, at the top of its pane, once its line has scrolled above the view: a pill of the command as you typed it, in the terminal's font, on a thin line across the pane.

## The header

- **It shows** while the pane's first row shows a command's output and the command's line is above the view, as when you scroll up through a long output, or as a running command's output pushes its line off the top.
- **It follows the scroll**: as the view reaches the command's line, the header goes; above it, the command before takes its place once that one's line is above the view; down, the next one does.
- **The command as you typed it**: zsh and fish tell Lokyr the line itself, so that a right prompt on its row, as zsh's `RPROMPT`, stays out of the pill; bash's is the text of its row.
- **A click on it** scrolls the view to the command's prompt, at the top.
- **It lies over the pane's first row**, taking none of the grid's rows, so that the program's grid keeps its size as the header comes and goes. A scroll over it scrolls the pane, and it takes no keys.
- **It shows nothing** on a program's own screen, as vim's or less's, and hides with its pane, behind another tab or a zoomed pane.

It needs your shell to mark its commands, as Lokyr's [shell integration](../CONFIGURATION.md#shell-integration) has zsh, bash and fish do.

## Configuration

Its keys go in [`[plugins.command-header]`](../CONFIGURATION.md#pluginscommand-header):

| Key | Values | Default | What it does |
|---|---|---|---|
| `background`, `text`, `border`, `line`, `hover` | a theme's token, by its name | `"panel.bg"`, `"text"`, `"panel.border"`, `"panel.border"`, `"button.hover.bg"` | The pill's background, its text, its border, the line across the pane, and the color over the pill's background under the pointer |
| `font-size` | points, `0` to `200` | `0` | The text's size, in the terminal's font; `0` for the terminal's own size |
| `font-weight` | `"light"`, `"regular"`, `"medium"`, `"semibold"`, `"bold"`, `"heavy"` | `"regular"` | The text's weight, the terminal's font's face nearest to it; `regular` the terminal's own face |
| `padding-x`, `padding-y` | points, `0` to `40` and `0` to `20` | `8`, `1` | The room in the pill left and right of the text, and above and below it |
| `align` | `"left"`, `"center"`, `"right"` | `"left"` | Where the pill sits along the line across the pane |

```toml
[plugins.command-header]
border = "accent"
font-size = 12
font-weight = "semibold"
padding-x = 10
align = "center"
```

## Colors

The header's colors are the theme's tokens that its keys name, so that they follow the light and the dark appearance, and a theme's colors: by default the pill takes the panels' background and border and the interface's text, and the line the border's color. Any token of the theme goes, as `accent`, `field.bg`, `text.secondary` or `tabs.active.bg`, as a theme's `[interface]` lists them.

## Turning it off

```toml
[plugins.command-header]
enabled = false
```

## Permissions

`history-read`, to read the commands that the shell marked, and `terminal-read`, to read a command's line.
