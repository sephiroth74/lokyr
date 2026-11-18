# Windows and tabs

<sub>macOS · on by default · the [`tabs`](plugins/tabs.md) plugin · [`[window]`](CONFIGURATION.md#window), [`[plugins.tabs]`](CONFIGURATION.md#pluginstabs), [`[app] restore-windows`](CONFIGURATION.md#restore-windows)</sub>

Open as many windows as you need, each with its own tabs, and move between them from the keyboard. Drag tabs to reorder them, to move them to another window or to split a pane, give them titles and colors of your own, and have your windows open again where you left them.

## Try it

Press ⌘T: the tab bar shows two tabs, the new one in front. Press ⌘1 to show the first tab again. Then drag the second tab out of the window and let go over the desktop: it opens in a window of its own, whose titlebar shows ⌥⌘2 at its right. Press ⌥⌘1: the first window comes forward.

## What it gives you

- **Windows** that you open with ⌘N and bring forward with ⌥⌘1 to ⌥⌘9, each showing its number in its titlebar.
- **Tabs** in a bar at the top or the bottom, or in a column on the left or the right, in three looks.
- **Drags** that reorder tabs, move them to another window, split a pane with them, or take them out to a window of their own.
- **Titles and colors** of your own for each tab, and marks that show what happens in the tabs you don't look at, as [the `tabs` plugin](plugins/tabs.md#the-bar) describes.
- **The tab you used before**, a press of ⌃Tab away. · Since 1.3
- **Pinned tabs**, kept at the start of the bar, and **groups** of tabs under a name and a color. · Since 1.3
- **A tab you closed**, back with ⇧⌘T. · Since 1.3
- **Your windows**, opened again as Lokyr starts, with their tabs and splits, when you turn it on.
- **A window that floats** above other apps, or one that you see through.
- **The [quick terminal](plugins/quick-terminal.md)**, a window that slides in from an edge of the screen over any app, at a key's press.

## Open and close windows

Press ⌘N, or choose Shell › New Window, to open a window. Its shell starts in the folder of the window you were in, as [`new-directory`](CONFIGURATION.md#new-directory) says. The first window opens where you last moved or resized one, as [`remember-frame`](CONFIGURATION.md#remember-frame) says, and later windows open a little down and to the right of the one you're in.

Press ⇧⌘W, or choose Shell › Close Window, to close the window with all its tabs. Lokyr asks first while a program other than a shell runs in it, such as vim or a build, because closing the window ends that program.

A pane closes when its shell exits, such as after `exit`, as [`close-on-exit`](CONFIGURATION.md#close-on-exit) says. A tab closes with its last pane, and a window with its last tab. Lokyr stays open without windows, like other Mac apps: click its icon in the Dock, or press ⌘N, to open one. Set [`quit-after-last-window-closed`](CONFIGURATION.md#quit-after-last-window-closed) to `true` to quit Lokyr instead.

**Try it**: press ⌘N, then run `exit` in the new window: the window closes, and the first one comes forward.

## Move between windows

Each window takes a number when it opens, the lowest that no other open window has, and shows its keys at the right of its titlebar, such as ⌥⌘2.

- **⌥⌘1 to ⌥⌘9** bring that window forward.
- **⌘\`** brings forward the next window, in the order they opened, and **⇧⌘\`** the one before. The key is the one left of 1, whatever your keyboard's layout prints on it.
- **The Window menu** lists Lokyr's windows, as in every Mac app.
- **Window › Go to Next Agent**, ⌥⌘A, brings forward the next [AI agent](agents.md) that waits for you, in any window.

[`title-shortcut`](CONFIGURATION.md#title-shortcut) hides the keys in the titlebar.

**Try it**: press ⌘N: the new window's titlebar shows ⌥⌘2. Press ⌥⌘1: the first window comes forward.

## Float a window or use full screen

Choose Window › Float on Top to keep the window above other apps' windows, such as while you follow a build beside your browser. Choose it again to let the window go back among the others. Each window floats on its own.

Click the window's green button, or choose View › Enter Full Screen, to use full screen. The tab bar stays at the screen's top, and the menu bar slides down over it when the pointer reaches the top. A floating window stops floating in full screen, and floats again when it leaves it.

**Try it**: choose Window › Float on Top, then click a window of another app: the Lokyr window stays above it.

## See through a window

Give the window's background an opacity below 1 to see what's behind it, and blur what shows through:

```toml
[window]
background-opacity = 0.85   # 0 is clear, 1 is opaque
background-blur = 20        # the blur's radius, in points
```

Only the background lets things show through: the text, the cursor, the selection and the colors that programs give stay opaque. A blur costs the window server some work: over a video that plays behind the window, a blur of 20 points took about 4 % more of one core. [`background-opacity`](CONFIGURATION.md#background-opacity) and [`background-blur`](CONFIGURATION.md#background-blur) tell more.

Press ⌘U, or choose View › Use Transparency, to show a translucent window opaque while you read, and again to see through it once more. Each window keeps its own choice, and your config stays as it is. · Since 1.3

**Try it**: add the lines above to your config file, which Lokyr › Settings… (⌘,) opens, and save it: the window shows what's behind it, blurred.

## Open your windows again when Lokyr starts

Lokyr can open the windows that were open when you quit it: each where it was, with its tabs in their order, their titles and colors, their splits, and each pane in its folder. [`[app] restore-windows`](CONFIGURATION.md#restore-windows) decides whether it does:

- `"system"`, the default, follows macOS's Close windows when quitting an application, in System Settings › Desktop & Dock. That setting is on by default, so your windows don't open again until you turn it off.
- `"always"` opens them again, whatever macOS's setting says.
- `"never"` never does.

```toml
[app]
restore-windows = "always"   # open the last windows again at every start
```

Each pane starts a new shell, in the folder where its shell was: what the panes showed, and the programs that ran in them, don't come back.

Pinned tabs come back pinned, and groups come back with their names, colors and folds. · Since 1.3

**Try it**: set `restore-windows = "always"`, open a second tab with ⌘T, and run `cd /usr/share` in it. Quit Lokyr with ⌘Q, then open it again: the window opens with both tabs, the second one in `/usr/share`.

## Open and close tabs

Press ⌘T, choose Shell › New Tab, or click the + at the bar's end to open a tab after the others. Its shell starts in the folder of the pane you were in, which your shell reports with [shell integration](shell-integration.md).

Press ⌘W, or choose Shell › Close Tab, to close the tab, or only the pane you're in while the tab has splits. You can also middle-click a tab, or click the close button that shows when you point at it. Lokyr asks first while a program other than a shell runs there. Close Other Tabs, in a tab's context menu, closes the window's other tabs.

While a window has a single tab, the bar hides, and the titlebar shows the tab's title and marks. Set [`hide-when-single`](CONFIGURATION.md#hide-when-single) to `false` to keep the bar.

**Try it**: press ⌘T twice: the bar shows three tabs. Press ⌘W twice: the bar hides again, and the titlebar shows the first tab's title.

## Move between tabs

- **⌘1 to ⌘9** show the first nine tabs, and each tab shows its keys, as [`show-index`](CONFIGURATION.md#show-index) says.
- **⇧⌘]** and **⇧⌘[** show the next and the previous tab in the bar, going round at the ends.
- **⌃Tab** shows the tab you used before. Hold Control and press Tab again to go further back: the tab that shows when you let go of Control becomes the most recent. ⌃⇧Tab goes the other way, and [`control-tab = "order"`](CONFIGURATION.md#control-tab) makes them follow the bar's order instead. · Since 1.3
- **⇧⌘←** and **⇧⌘→** move the tab one place left or right.

**Try it**: with three tabs, press ⌘1, then ⇧⌘]: the second tab shows. Press ⇧⌘→: it moves to the third place.

## Drag tabs

Drag a tab along the bar to put it elsewhere in the bar: the other tabs slide out of its way. Drag it out of the bar, and a picture of the tab and its panes follows the pointer. Where you let go decides what happens:

- **On a window's bar**, its own or another window's, the tab goes there, at the place under the pointer.
- **On a pane**, the tab splits that pane: the half nearest the pointer lights up, and the tab's panes take it. A tab with splits goes in whole.
- **Outside every window**, the tab opens in a window of its own. Shell › Move Tab to New Window does the same from the menu.

Press Escape during a drag to put the tab back where it was.

**Try it**: with two tabs, drag the second tab down onto the right half of the pane, and let go when that half lights up: the pane splits, with the second tab's shell on the right.

## Name and color tabs

A tab shows the title that its program sets, such as vim or your prompt, or else its folder and the program that runs in it, such as `src — vim`. A title set by a program other than the shell goes when the shell is back in front, such as when `ssh` ends.

- **Rename a tab**: double-click it, or choose Shell › Rename Tab…, and type its title. An empty title gives the tab its own back.
- **Color a tab**: right-click it and choose a color under Color, or No Color. A program can color its tab too, with [OSC 6;1](https://iterm2.com/documentation-escape-codes.html); the color you choose goes before the program's.
- **Title the window**: Shell › Set Title…, ⇧⌘I, gives the window a title of your own, and an empty title gives it its own back.
- **The window's size** ends its title, such as `120×30`, as [`title-size`](CONFIGURATION.md#title-size) says.

**Try it**: open a second tab with ⌘T, and run this command in it, which colors the tab red, as a program would:

```sh
printf '\033]6;1;bg;red;brightness;220\033\\'
```

The tab turns red. Run this command to take the program's color away:

```sh
printf '\033]6;1;bg;*;default\033\\'
```

## Pin tabs

<sub>Platforms: macOS · Since 1.3</sub>

Right-click a tab and choose Pin Tab, or choose Shell › Pin Tab, to keep it at the start of the bar. In a bar across the window, a pinned tab is narrow: it shows a pin, the first letters of its title and its most urgent mark, such as an agent that waits for you, and its whole title when you point at it. Down a column, it shows a pin instead of its close button.

⌘W doesn't close a pinned tab, nor does Close Other Tabs, but ⌘W still closes one of its panes when it has several. Right-click it and choose Unpin Tab to let it go.

**Try it**: with two tabs, right-click the second and choose Pin Tab: it moves to the start of the bar and narrows. Press ⌘W while it shows: it stays.

## Reopen a closed tab

<sub>Platforms: macOS · Since 1.3</sub>

Press ⇧⌘T, or choose Shell › Reopen Closed Tab, to bring back the tab you closed last, at its place and as you left it. Press it again for the one before.

For 10 seconds after you close a tab, its programs keep running and what they print is kept; then the tab closes for good. A closed tab keeps its memory while it waits. [`reopen-seconds`](CONFIGURATION.md#reopen-seconds) sets how long it waits, and `0` closes tabs at once.

Some tabs close for good at once:

- A tab that you close while a program other than a shell runs there, as Lokyr asks first and closing it ends the program.
- A tab whose program ends, the last tab of a window, and a window that you close.
- The oldest of 16 closed tabs, when you close another.

**Try it**: open a tab with ⌘T, run `ls`, and close the tab with ⌘W. Press ⇧⌘T within 10 seconds: the tab comes back, with the listing.

## Group tabs

<sub>Platforms: macOS · Since 1.3</sub>

Right-click a tab, choose New Group with Tab, and type the group's name. The group shows as a pill of its name and color before its tabs, which take a tint of its color.

- **Fold a group**: click its pill to hide its tabs behind it, and click it again to show them. Showing one of its tabs unfolds it too.
- **Put a tab in a group**: drag it right after the pill, between two of the group's tabs, or after its last one. A tab's context menu also has Add to Group.
- **Take a tab out**: drag it elsewhere in the bar, or right-click it and choose Remove Tab from Group.
- **Rename, recolor or ungroup**: right-click the pill.

Down a column, a group is a row of its color's dot and its name.

**Try it**: with three tabs, right-click the first, choose New Group with Tab and type `build`. Drag the third tab right after the first: both are in the group. Click the pill: the group folds.

## Choose where the bar goes

[`position`](CONFIGURATION.md#position) puts the bar at the top or the bottom of the window, or in a column on the left or the right. [`style`](CONFIGURATION.md#style-1) chooses its look:

- `"unified"`, the default: the tabs in the titlebar.
- `"standard"`: a strip below the titlebar.
- `"seamless"`: cells of the titlebar's height, the tab in front opening onto its terminal.

On the left, in the unified and seamless looks, the column runs up through the titlebar, like a sidebar of macOS. Drag a column's edge to change its width.

```toml
[plugins.tabs]
position = "left"
style = "seamless"
```

**Try it**: with two tabs open, add these lines to your config file and save it: the tabs move to a column on the left.

## Settings

```toml
[window]
columns = 100                     # a new window's width, in cells
rows = 30                         # and its height
remember-frame = true             # the first window opens where you left one
title-shortcut = true             # the window's keys, such as ⌥⌘1, in its titlebar

[app]
restore-windows = "system"        # "always" opens your last windows again
quit-after-last-window-closed = false

[plugins.tabs]
position = "top"                  # "bottom", "left" or "right"
style = "unified"                 # "standard" or "seamless"
hide-when-single = true           # a single tab hides the bar
close-button = "hover"            # "always" or "never"
```

[`[window]`](CONFIGURATION.md#window), [`[app]`](CONFIGURATION.md#app) and [`[plugins.tabs]`](CONFIGURATION.md#pluginstabs) tell every key, [`control-tab`](CONFIGURATION.md#control-tab) and [`reopen-seconds`](CONFIGURATION.md#reopen-seconds) among them.

## Commands and keys

| Keys | In the config | What they do |
|---|---|---|
| ⌘N | `cmd+n` | Open a window |
| ⇧⌘W | `cmd+shift+w` | Close the window |
| ⌥⌘1 to ⌥⌘9 | `cmd+alt+1` to `cmd+alt+9` | Bring that window forward |
| ⌘\`, ⇧⌘\` | `` cmd+` ``, `` cmd+shift+` `` | Bring the next or the previous window forward |
| ⇧⌘I | `cmd+shift+i` | Give the window a title |
| ⌘T | `cmd+t` | Open a tab |
| ⌘W | `cmd+w` | Close the tab, or the pane while the tab has splits |
| ⌘1 to ⌘9 | `cmd+1` to `cmd+9` | Show that tab |
| ⇧⌘], ⇧⌘[ | `cmd+shift+]`, `cmd+shift+[` | Show the next or the previous tab |
| ⌃Tab, ⌃⇧Tab | `ctrl+tab`, `ctrl+shift+tab` | Show the tab you used before, or go the other way · Since 1.3 |
| ⇧⌘←, ⇧⌘→ | `cmd+shift+left`, `cmd+shift+right` | Move the tab a place |
| ⇧⌘T | `cmd+shift+t` | Reopen the tab you closed last · Since 1.3 |
| ⌘U | `cmd+u` | Show the window opaque, or translucent again · Since 1.3 |

[The commands' reference](commands.md#windows) lists every command of the windows and the tabs, those of no keys too, such as `tabs.toggle-pin` and `window.toggle-float-on-top`, which you can bind in [`[keys]`](CONFIGURATION.md#bindings).

## Limits

- **A program can't move, resize or minimize its window**, nor read its title.
- **Restored windows start new shells**: the panes' text and their programs don't come back, and a window that was in full screen opens out of it.
- **The quick terminal and man pages' windows** don't open again when Lokyr starts.
- **macOS's own window tabs**, and its Prefer tabs setting, don't apply: Lokyr's tabs are its own.
- **Keys reach the first nine** windows and tabs: bind more in [`[keys]`](CONFIGURATION.md#bindings).

## Troubleshooting

### The tab bar doesn't show

**Why**: the window has a single tab, which hides the bar by default, or the `tabs` plugin is turned off.

**Fix**: press ⌘T to open a second tab, or keep the bar for a single tab too:

```toml
[plugins.tabs]
hide-when-single = false
```

**Check**: the bar shows the window's tab. If ⌘T opens no tab, remove `enabled = false` from your config's `[plugins.tabs]`.

### Your windows don't open again when Lokyr starts

**Why**: `restore-windows` follows macOS's Close windows when quitting an application, which is on by default.

**Fix**: set `restore-windows = "always"` in your config's `[app]`, or turn that setting off in System Settings › Desktop & Dock.

**Check**: quit Lokyr with two windows open, then open it again: both windows open.

### New tabs open in your home folder

**Why**: your shell doesn't report its folder, which Lokyr learns from [shell integration](shell-integration.md).

**Fix**: see [Shell integration](shell-integration.md#new-tabs-open-in-your-home-folder).

## See also

- [The `tabs` plugin](plugins/tabs.md), its marks, commands and settings
- [The quick terminal](plugins/quick-terminal.md)
- [Splits](plugins/splits.md)
- [AI agents](agents.md)
- [Mouse and selection](mouse-and-selection.md)
