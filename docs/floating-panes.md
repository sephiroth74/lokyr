# Floating panes and popups

<sub>macOS · not released yet · [`[keys]`](CONFIGURATION.md#bindings), [`[app] restore-scrollback`](CONFIGURATION.md#restore-scrollback)</sub>

A floating pane floats over a tab's splits, in a box with its title at the top, such as a scratch shell that you show and hide with a key. A popup is a floating pane in the middle of the tab that runs one command, such as `lazygit` or `fzf`, and closes when the command ends.

## Try it

Press ⇧⌥⌘F and run `top`. Press ⌥⌘F to hide it, and ⌥⌘F again: `top` is still running. Drag the box by its title, then drag one of its corners to resize it.

Then bind a popup. Add this to your config and save it:

```toml
[keys]
"ctrl+cmd+o" = { command = "pane.popup", args = { command = "fzf", width = 0.6, height = 0.5 } }
```

Press ⌃⌘O: `fzf` opens in the middle of the tab. Press Escape: the popup closes.

## What it gives you

- **A pane over your splits**, which leaves their sizes as they are.
- **Several in a tab**, shown and hidden together with one key.
- **The mouse** moves a floating pane by its title and resizes it by its edges and corners.
- **Popups** that run a command of your keys and close when it ends, or stay to show its error.
- **Floating panes that come back** with their tab when Lokyr opens your windows again.
- **Popups from plugins**, such as one that opens a Git client in the folder you're in.

## Open, show and hide floating panes

- **Shell › New Floating Pane**, ⇧⌥⌘F, opens a floating pane over the tab. Its shell starts in the folder of the pane you were in. Each new one opens a little down and to the right of the one before.
- **Shell › Show or Hide Floating Panes**, ⌥⌘F, hides the tab's floating panes together, their programs running, and shows them again. On a tab with none, it opens one.
- **Click a pane** to give it the keyboard, whether it floats or not. The floating pane that you click comes in front of the others.

## Move, resize and close a floating pane

- **Move it**: drag its title bar. It stays within the tab.
- **Resize it**: drag its edges or its corners. The pointer shows the way it resizes.
- **Close it**: click the × at the right of its title, press ⌘W, choose Shell › Pane › Close Pane, or exit its shell. Lokyr asks first while a program other than a shell runs there.

## Move a pane between the splits and the floating panes

- **Float a pane of the splits**: right-click it and choose Float Pane. Its room goes to the pane beside it. A tab of one pane has nothing to float.
- **Put a floating pane back into the splits**: right-click it and choose Move Pane into Splits. It goes to the right of the split pane that you used last, half and half.

## Run a command in a popup

Bind `pane.popup` to a key in the config's `[keys]`, with the command to run:

```toml
[keys]
"ctrl+cmd+g" = { command = "pane.popup", args = { command = "lazygit" } }
"ctrl+cmd+o" = { command = "pane.popup", args = { command = "fzf | xargs open", width = 0.6, height = 0.5 } }
"ctrl+cmd+n" = { command = "pane.popup", args = { command = "vim notes.md", folder = "~/notes" } }
```

- **Where it runs**: your login shell runs the command, so it finds the programs of your `PATH`, in the folder of the pane you were in. `folder` names another, such as `"~/notes"`.
- **Its size**: `width` and `height` are shares of the tab, from `0.05` to `1`: `0.8` and `0.7` when you leave them out.
- **When it closes**: when the command exits with the status 0, or when you interrupt it, such as with Control-C, or Escape in `fzf`. When the command fails, the popup stays open with a line that tells how it ended, so that you can read its error, until you press a key. This holds whatever [`close-on-exit`](CONFIGURATION.md#close-on-exit) says.
- **Your keys**: the command gets them all, Escape too.
- **On its own**: a popup shows whether the tab's other floating panes show or not. ⌥⌘F shows or hides those under it, and the popup keeps the keyboard.

You can move, resize and close a popup as any floating pane.

## Popups from plugins

A plugin can open a popup too, such as a Git client in the folder you're in, and learn how its command ended. It needs two [capabilities](plugins/reference.md#capabilities): `workspace-write`, as it opens a pane, and `pty-write`, as it runs a command there. Lokyr asks you for both when the plugin first runs. [The plugin reference](PLUGIN-SYSTEM.md) covers `cx.open_popup` and the `popup-closed` event.

## Settings

```toml
[keys]
"ctrl+cmd+g" = { command = "pane.popup", args = { command = "lazygit" } }

[app]
restore-windows = "always"     # your floating panes come back with their tabs
restore-scrollback = true      # with what they showed when you quit
```

- [`[keys]`](CONFIGURATION.md#bindings): your popups' keys, and other keys for the floating panes' commands.
- [`[app] restore-windows`](CONFIGURATION.md#restore-windows): whether your windows, and the floating panes of their tabs, open again as Lokyr starts.
- [`[app] restore-scrollback`](CONFIGURATION.md#restore-scrollback): whether the panes that open again show what they showed when you quit.
- [`[window] close-on-exit`](CONFIGURATION.md#close-on-exit): what a pane does when its program ends, which popups don't follow.

## Commands and keys

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| `pane.new-floating` | New Floating Pane | | ⇧⌥⌘F |
| `pane.toggle-floating` | Show or Hide Floating Panes | | ⌥⌘F |
| `pane.float` | Float Pane | | |
| `pane.move-into-splits` | Move Pane into Splits | | |
| `pane.popup` | Open Popup | `command`, text; `folder`, text; `width`, number; `height`, number | |

Float Pane and Move Pane into Splits are in a pane's context menu alone. [The commands' reference](commands.md#panes) lists them with the other commands of the panes.

## How it works

- **A floating pane belongs to its tab.** It moves with the tab to another window, and closes with it.
- **Its frame is a share of the tab's room**, so it keeps its place and size, in proportion, as the window resizes. It is at least a twentieth of the room each way.
- **Hidden floating panes keep running**, and their scrollback stays in memory; Lokyr draws nothing for them.
- **A popup's status**: a command interrupted by Control-C exits with 130 in most shells, which closes the popup as 0 does.
- **When Lokyr opens your windows again** as it starts, each floating pane comes back in its tab, at its place, shown or hidden as you left it. Popups don't come back, so that a command doesn't run again on its own.

## Limits

- **Your programs end when you quit Lokyr**: a floating pane comes back with a new shell.
- **A saved [layout](layouts.md) leaves out a session's floating panes**: it opens with the splits alone.
- **Floating panes don't snap** to the tab's edges or to each other.

## See also

- [Windows and tabs](windows-and-tabs.md), for the splits, tabs and broadcast.
- [Splits](plugins/splits.md), for the panes of a tab's layout.
- [Sessions](sessions.md), whose windows keep their floating panes.
