# `splits`: panes side by side

<sub>`lokyr/splits` · bundled · [source](../../plugins/splits)</sub>

A tab of several panes, side by side and one above another: the commands that split a pane, move the keyboard between panes, resize, zoom, equalize and swap them, move a pane out to a tab or a window of its own, and close it.

## The panes

- **The dividers** between panes move with a drag; a double click shares the room half and half again.
- **The pane that has the keyboard** stands out: the others fade a little toward their background, as `dim-inactive` says.
- **A new pane** starts its shell in the folder of the pane that it splits, as the shell tells it.
- **A tab dragged onto a pane** splits that pane, the tab's panes taking the half where it fell, as the [`tabs`](tabs.md) plugin's drag does.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `splits.split` | Shell › Split Right, Split Left, Split Down, Split Up | ⌘D (right), ⇧⌘D (down) | Splits the pane, `direction` `right`, `left`, `down` or `up`: the new pane, on that side, takes the keyboard |
| `splits.focus` | Shell › Select Pane Left, Select Pane Right, Select Pane Above, Select Pane Below | ⌥⌘ and an arrow | Gives the keyboard to the pane beside, `direction` `left`, `right`, `up` or `down` |
| `splits.focus-next`, `splits.focus-prev` | Shell › Select Next Pane, Select Previous Pane | ⌘], ⌘[ | Gives the keyboard to the tab's next or previous pane, around the ends |
| `splits.resize` | | ⌃⌘ and an arrow | Moves a divider of the pane `cells`, 1 by default, towards `direction` |
| `splits.zoom-toggle` | Shell › Zoom Pane | ⇧⌘Return | Has the pane take the tab's whole room, or gives the others theirs back |
| `splits.equalize` | Shell › Equalize Panes | | Shares every split of the tab half and half again |
| `splits.swap` | | | Swaps the pane with the one beside it, `direction` `left`, `right`, `up` or `down` |
| `splits.move-to-tab` | Shell › Move Pane to New Tab | | Moves the pane out to a tab of its own |
| `splits.move-to-window` | Shell › Move Pane to New Window | | Moves the pane out to a window of its own |
| `splits.close` | Shell › Close Pane | | Closes the pane of a tab of more than one, asking first while a program other than the shell runs in it; ⌘W closes it too, as the tab's close does while the tab has splits |

The pane's context menu splits it right, left, down or up, moves it to a tab or a window of its own, and closes it; an item that would do nothing there is dimmed, as Close Pane in a tab of one pane.

The commands without keys are there for your own bindings:

```toml
[keys]
"cmd+alt+e" = "splits.equalize"
"cmd+alt+shift+left" = { command = "splits.swap", args = { direction = "left" } }
```

## Configuration

Its keys go in [`[plugins.splits]`](../CONFIGURATION.md#pluginssplits), each applied as you save the file:

| Key | Values | Default | What it does |
|---|---|---|---|
| `dim-inactive` | from `0` to `1` | `0.15` | How much the panes that have not the keyboard fade into their background; `0` leaves them as they are |
| `pane-titles` | `true`, `false` | `false` | Whether each pane shows its title across its top |
| `focus-follows-mouse` | `true`, `false` | `false` | Whether the keyboard goes to the pane that the pointer comes over, without a click |

```toml
[plugins.splits]
dim-inactive = 0.3
pane-titles = true
```

## Turning it off

```toml
[plugins.splits]
enabled = false
```

The splits' commands go, with their keys, and the panes no longer fade or show titles; the panes that a tab has stay, and a click still gives one the keyboard.

## Permissions

It reads the tabs' layouts, and changes them, as the plugin API lets every plugin; its manifest asks to read the processes, `process-read`, for the panes' folders and programs in their titles.
