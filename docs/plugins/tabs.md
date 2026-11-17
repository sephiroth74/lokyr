# `tabs`: the tab bar

<sub>`lokyr/tabs` · bundled · [source](../../plugins/tabs)</sub>

Each window's tabs, in a bar at the top, at the bottom, or in a column on the left or the right, and the commands that open, close, select, move, rename and color them.

## The bar

- **A tab** shows its title, the keys that select it, as ⌘1, its color, and its close button. Its title is the one that you give it with Shell › Rename Tab…, or its program's, or its folder and the program in its foreground, as `src — vim`. A title that a program other than the shell sets, as the remote shell under `ssh` does, goes as the shell has the foreground again, the title from before back, while a title that the shell sets as it comes back, at its prompt, stays.
- **Its marks** tell you what happens in it while you look elsewhere: new output, a bell, a program that runs, the protected input of Secure Keyboard Entry, an AI agent's state, at work, waiting for you, done or failed, as [`[agents]`](../CONFIGURATION.md#agents) tells, the tab's tooltip telling what an agent that waits asked for, a dot of the color that a program gives the status of its session, as OSC 21337 tells it, the status's text and detail in the tooltip, and a program's progress, which OSC 9;4 reports, round the tab's outline. A program that prints of its own shows a spinner. The [README](../progress-and-status.md#get-the-progress-of-gradle-claude-code-and-cargo) tells how to have Gradle, Claude Code and cargo report their progress.
- **With the mouse**: a click shows a tab, a middle click closes it, a double click renames it, and its context menu renames it, colors it, of macOS's system colors or none, moves it to a window of its own, closes it, or closes the others but the pinned ones; it also starts a group with the tab, adds it to another group, or takes it out of its group. The pane's context menu renames its tab too. A drag reorders the tabs, moves a tab to another window's bar, drops it on a pane to split that pane, or takes it out to a window of its own; Escape puts it back.
- **A pinned tab** stays at the start of the bar. In a bar across the window it is narrow and shows a pin, the first letters of its title and its most urgent mark, such as an agent that waits for you or a bell; hover over it to see the whole title, and what an agent asked for or the status that a program tells. VoiceOver reads its whole title. In a column it shows a pin instead of its close button. ⌘W doesn't close a pinned tab, but closes one of its panes when it has several. Restored windows keep their pinned tabs. · not released yet
- **A group of tabs** gathers tabs under a name and a color: right-click a tab and choose New Group with Tab, which asks for its name. It shows as a pill of its name and color before its tabs, which take a tint of its color, the tab you're on aside. Click the pill to fold the group, which hides its tabs behind the pill and shows how many there are, and click it again to unfold it; showing one of its tabs unfolds it too, and so does moving the tab you're on into it. Right-click the pill to rename the group, change its color, fold it or ungroup it. Down a column, the group is a row of its color's dot and its name. Drag a tab right after a group's pill, between two of its tabs or after its last to put it in the group; drag it elsewhere to take it out. A tab moved between two tabs of a group joins it, and restored windows keep their groups. · not released yet
- **A single tab** hides the bar by default, its title and its marks shown in the titlebar.
- **The looks**: `"standard"`, a strip below the titlebar; `"unified"`, the tabs in the titlebar, the default; and `"seamless"`, cells of the titlebar's height, the one shown opening onto its terminal.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `tabs.new` | Shell › New Tab | ⌘T | Opens a tab after the others, its shell in the folder of the pane that has the keyboard |
| `tabs.close` | Shell › Close Tab | ⌘W | Closes the tab, or its pane while it has splits, asking you first while a program other than a shell runs there |
| `tabs.next`, `tabs.prev` | Shell › Select Next Tab, Select Previous Tab | ⇧⌘], ⇧⌘[ | Shows the next or the previous tab, around the ends |
| `tabs.switch`, `tabs.switch-back` | Shell › Switch Tab, Switch Tab Back | ⌃Tab, ⌃⇧Tab | Shows the tab you used before. Press Tab again while you hold Control to go further back; the tab that shows when you release Control becomes the most recent. ⌃⇧Tab goes the other way. With `control-tab = "order"`, they show the next and the previous tab instead · not released yet |
| `tabs.goto` | | ⌘1 to ⌘9 | Shows the tab of that `index`, from 1 at the left |
| `tabs.move` | | ⇧⌘←, ⇧⌘→ | Moves the tab a place, `delta` of -1 or 1 |
| `tabs.rename` | Shell › Rename Tab… | | Gives the tab its `title`; without one, it asks you for it, and an empty one gives the tab its own back |
| `tabs.set-color` | | | Gives the tab its `color`, as `"#ff8800"`; an empty one gives the program's back |
| `tabs.detach` | Shell › Move Tab to New Window | | Moves the tab out to a window of its own |
| `tabs.toggle-pin` | Shell › Pin Tab | | Pins the tab at the start of the bar, or unpins it. The tab's context menu has the same item · not released yet |
| `tabs.reopen` | Shell › Reopen Closed Tab | ⇧⌘T | Brings back the tab you closed last, at its place, as you left it, what its programs printed meanwhile included; its find bar and copy mode stay closed. Press it again for the one before. A closed tab waits `reopen-seconds`, 10 by default, and 16 wait at most. A tab that you close while a program other than a shell runs there doesn't wait: Lokyr asks first, and closing it ends the program · not released yet |
| `tabs.new-group` | Shell › New Group with Tab | | Starts a group with the tab, of a color that the window's other groups don't have, and asks you for its name · not released yet |
| `tabs.add-to-group` | Shell › Add Tab to Group | | Puts the tab at the end of the group of its `group` id; without one, the nearest group before the tab, or after it · not released yet |
| `tabs.remove-from-group` | Shell › Remove Tab from Group | | Takes the tab out of its group, right after the group's tabs · not released yet |
| `tabs.rename-group` | Shell › Rename Group… | | Gives the tab's group its `title`; without one, it asks you for it, and an empty one leaves the group of no name · not released yet |
| `tabs.set-group-color` | | | Gives the tab's group its `color`, as `"#ff8800"`; an empty one leaves it of none, gray · not released yet |
| `tabs.fold-group` | Shell › Fold Group | | Folds the tab's group, showing the tab after it · not released yet |
| `tabs.ungroup` | Shell › Ungroup | | Takes the tab's group apart, its tabs staying where they are · not released yet |

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
| `control-tab` | `"recent"`, `"order"` | `"recent"` | What ⌃Tab and ⌃⇧Tab step through: the tabs in the order you used them, or in the bar's order · not released yet |
| `reopen-seconds` | from `0` to `86400` | `10` | How many seconds a tab you close stays ready to come back with ⇧⌘T. Its programs keep running meanwhile and it keeps its memory; at most 16 closed tabs wait, the oldest closing for good past them; `0` keeps none · not released yet |

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

It reads the windows and the tabs, and changes them, as the plugin API lets every plugin; its manifest asks to read the processes, `process-read`, for the tabs' folders and programs in their titles, and for whether a program runs or prints, an AI agent's state and a program's session's status, which its marks show.
