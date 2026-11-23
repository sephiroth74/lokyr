# Layouts

<sub>macOS · not released yet · `~/.config/lokyr/layouts/`</sub>

A layout is a session saved to a file: its windows, their tabs and splits, the folder of each pane, and, if you add them, the commands that the panes run. Open a layout and Lokyr opens a new session that looks the same, with new shells. · not released yet

## Try it

Open a window in a project's folder and split it with ⌘D. Choose Window › Sessions › Save Session as Layout…, name it `api`, and choose Save. Press ⌃⌘S: the switcher lists `api` among your layouts. Choose it: a new session opens, its window split the same way, each shell in its folder.

## Save a session

Choose Window › Sessions › Save Session as Layout… and give the layout a name. Lokyr writes `~/.config/lokyr/layouts/<name>.toml` with the windows of the session that shows: each window's place, each tab's title and color, its splits, and the folder of each pane's shell. When a layout of that name exists already, Lokyr asks before it replaces it, because you may have added commands to it.

## Open a layout

Press ⌃⌘S, or choose Window › Sessions › Switch Session…, and choose a layout: the switcher lists them after your sessions, each marked Layout. Lokyr opens a new session named after the layout, with its windows where the file puts them. Each pane's shell starts in its folder, and then runs the pane's command, as though you typed it, so the shell stays when the command ends.

To open a layout from a key of your own, bind `window.open-layout` with the layout's `name`:

```toml
[keys]
"ctrl+cmd+a" = { command = "window.open-layout", args = { name = "api" } }
```

## Write a layout by hand

A layout is a TOML file. This one opens two windows: the first of a tab split in two, a server on the left and a shell on the right, then a tab for the logs; the second of one tab, in your notes' folder.

```toml
# The session's folder, which the switcher finds it by.
folder = "~/src/api"

[[window]]
# Where the window opens, in points from the top left of the main screen.
frame = { x = 80, y = 60, width = 1200, height = 800 }

[[window.tab]]
title = "server"
color = "#3478f6"
[window.tab.split]
direction = "right"    # the second part on the right; "down" puts it below
ratio = 0.6            # the first part's share of the room
[window.tab.split.first.pane]
folder = "~/src/api"
command = "cargo run"
[window.tab.split.second.pane]
folder = "~/src/api"
focused = true         # this pane has the tab's keyboard

[[window.tab]]
title = "logs"
active = true          # the window shows this tab
[window.tab.pane]
folder = "~/src/api"
command = "tail -f log/development.log"

[[window]]

[[window.tab]]
pinned = true
[window.tab.pane]
folder = "~/notes"
```

| Key | Where | What it is |
|---|---|---|
| `folder` | The file | The session's folder, a path from `/` or `~`. The switcher lists no folder of a session, and finds this one there. |
| `[[window]]` | The file | A window, one at least. |
| `frame` | A window | Where the window opens: `x` and `y` from the main screen's top left, `width` and `height`, in points. Without it, the window opens where a new one would. |
| `[[window.tab]]` | A window | A tab, one at least. |
| `title`, `color` | A tab | A title of its own, and a color, `#rrggbb`. |
| `active` | A tab | Whether the window shows this tab rather than its first one. |
| `pinned` | A tab | Whether the tab is pinned at the window's start. |
| `pane` | A tab, or a split's part | One pane. A tab, and each part of a split, has a `pane` or a `split`. |
| `split` | A tab, or a split's part | Two parts: `direction`, `"right"` or `"down"`, the `ratio` of the first part, more than 0 and less than 1, and the parts, `first` and `second`. |
| `folder` | A pane | The folder that its shell starts in, a path from `/` or `~`; your home folder without it. |
| `command` | A pane | A command line that its shell runs as it starts, as though you typed it. |
| `focused` | A pane | Whether the pane has its tab's keyboard, rather than the tab's first pane. |
| `zoomed` | A pane | Whether the pane takes its tab's whole room. |

## Limits

- **A layout opens new shells**: the programs that ran when you saved it don't come back, and neither does what the panes showed. Add a `command` to start a program.
- **A layout opens 256 panes at most**, as restored windows do.
- **A file that doesn't read**, such as of a key that a layout lacks, opens nothing: a notice tells the file and the line of the error.
- **A pane's command runs in its folder alone**: when the folder isn't there, such as on another Mac, the pane's shell starts in your home folder and the command doesn't run, as a notice tells.

## See also

- [Sessions](sessions.md), for the sessions and their switcher.
- [Commands](commands.md), for `window.save-layout` and `window.open-layout`.
