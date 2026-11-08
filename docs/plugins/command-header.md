# `command-header`: the command's header

<sub>`lokyr/command-header` · bundled · [source](../../plugins/command-header)</sub>

The command whose output you are looking at, at the top of its pane, once its line has scrolled above the view: a pill of the command as you typed it, on a thin line across the pane.

## The header

- **It shows** while the pane's first row shows a command's output and the command's line is above the view, as when you scroll up through a long output, or as a running command's output pushes its line off the top.
- **It follows the scroll**: as the view reaches the command's line, the header goes; above it, the command before takes its place once that one's line is above the view; down, the next one does.
- **A click on it** scrolls the view to the command's prompt, at the top.
- **It lies over the pane's first row**, taking none of the grid's rows, so that the program's grid keeps its size as the header comes and goes. A scroll over it scrolls the pane, and it takes no keys.
- **It shows nothing** on a program's own screen, as vim's or less's, and hides with its pane, behind another tab or a zoomed pane.

It needs your shell to mark its commands, as Lokyr's [shell integration](../CONFIGURATION.md#shell-integration) has zsh, bash and fish do.

## Turning it off

```toml
[plugins.command-header]
enabled = false
```

## Permissions

`history-read`, to read the commands that the shell marked, and `terminal-read`, to read a command's line.
