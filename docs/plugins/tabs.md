# `tabs`: the tab bar

<sub>`lokyr/tabs` · bundled · [source](../../plugins/tabs)</sub>

Each window's tabs, in a bar at the top, at the bottom, or in a column on the left or the right, and the commands that open, close, select, move, rename and color them.

## The bar

- **A tab** shows its title, the keys that select it, as ⌘1, its color, and its close button. Its title is the one that you give it with Shell › Rename Tab…, or its program's, or its folder and the program in its foreground, as `src — vim`.
- **Its marks** tell you what happens in it while you look elsewhere: new output, a bell, a program that runs, the protected input of Secure Keyboard Entry, and a program's progress, which OSC 9;4 reports, round the tab's outline. A program that prints of its own shows a spinner. The [README](../../README.md#programs-progress) tells how to have Gradle, Claude Code and cargo report their progress.
- **With the mouse**: a click shows a tab, a middle click closes it, a double click renames it, and its context menu renames it, moves it to a window of its own, closes it, or closes the others. The pane's context menu renames its tab too. A drag reorders the tabs, moves a tab to another window's bar, drops it on a pane to split that pane, or takes it out to a window of its own; Escape puts it back.
- **A single tab** hides the bar by default, its title and its marks shown in the titlebar.
- **The looks**: `"standard"`, a strip below the titlebar; `"unified"`, the tabs in the titlebar, the default; and `"seamless"`, cells of the titlebar's height, the one shown opening onto its terminal.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `tabs.new` | Shell › New Tab | ⌘T | Opens a tab after the others, its shell in the folder of the pane that has the keyboard |
| `tabs.close` | Shell › Close Tab | ⌘W | Closes the tab, or its pane while it has splits, asking you first while a program other than a shell runs there |
| `tabs.next`, `tabs.prev` | Shell › Select Next Tab, Select Previous Tab | ⇧⌘], ⇧⌘[, ⌃Tab, ⌃⇧Tab | Shows the next or the previous tab, around the ends |
| `tabs.goto` | | ⌘1 to ⌘9 | Shows the tab of that `index`, from 1 at the left |
| `tabs.move` | | ⇧⌘←, ⇧⌘→ | Moves the tab a place, `delta` of -1 or 1 |
| `tabs.rename` | Shell › Rename Tab… | | Gives the tab its `title`; without one, it asks you for it, and an empty one gives the tab its own back |
| `tabs.set-color` | | | Gives the tab its `color`, as `"#ff8800"`; an empty one gives the program's back |
| `tabs.detach` | Shell › Move Tab to New Window | | Moves the tab out to a window of its own |

The commands without keys are there for your own bindings:

```toml
[keys]
"cmd+shift+r" = "tabs.rename"
"cmd+alt+r" = { command = "tabs.set-color", args = { color = "#d14" } }
```

## Configuration

Its keys go in [`[plugins.tabs]`](../CONFIGURATION.md#pluginstabs), each applied as you save the file:

| Key | Values | Default | What it does |
|---|---|---|---|
| `position` | `"top"`, `"bottom"`, `"left"`, `"right"` | `"top"` | Where the bar goes; on the left, in the `"unified"` and `"seamless"` looks, the column runs up through the titlebar, as macOS's sidebars do |
| `style` | `"standard"`, `"unified"`, `"seamless"` | `"unified"` | The bar's look |
| `hide-when-single` | `true`, `false` | `true` | Whether the bar hides while the window has a single tab |
| `show-index` | `true`, `false` | `true` | Whether each of the first nine tabs shows the keys that select it |
| `close-button` | `"always"`, `"hover"`, `"never"` | `"hover"` | When a tab shows its close button |
| `column-width` | from `60` to `600` | `200` | The column's width in points, on the left or the right, which a drag of its edge changes too |

```toml
[plugins.tabs]
position = "left"
style = "seamless"
```

## Turning it off

```toml
[plugins.tabs]
enabled = false
```

The bar and the tabs' commands go, with their keys: no key opens a tab, and ⌘W closes the window.

## Permissions

It reads the windows and the tabs, and changes them, as the plugin API lets every plugin; its manifest asks to read the processes, `process-read`, for the tabs' folders and programs in their titles, and for whether a program runs or prints, which its marks show.
