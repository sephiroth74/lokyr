# `automation`: what scripts and shortcuts ask of Lokyr

<sub>`lokyr/automation` · bundled · [source](../../plugins/automation) · not released yet</sub>

The answers to what scripts and shortcuts of other apps ask of Lokyr: windows, tabs and splits opened, text sent to a pane, a command run, and the windows, tabs and panes listed, or a pane's text read once you allow it.

## What it answers

- **The windows, their tabs and their panes**, each of its title, the folder where its shell works and whether it has the focus. Their ids stay the same from one request to the next, so that a script that opened a tab can send text to its pane later.
- **A window, a tab or a split**, its pane in a folder of your choice, its shell running a command line as it starts, as if you typed it. A tab opens in the window in front, or in the first window while Lokyr is behind another app; a split opens beside the focused pane.
- **Text sent to a pane**, as if you typed it.
- **A window, a tab or a pane brought forward, or closed.** Closing a tab or a pane where a program other than a shell runs asks you first, as Close does.
- **A command run by its name and arguments**, as the [command palette](command-palette.md) runs it, such as `splits.split` of `direction = "right"`. A command that Lokyr lacks is refused, of the reason.

## A pane's text

A pane's text may hold passwords and other secrets. The first time that an app asks for it, Lokyr asks you whether to allow that app, on the window in front:

- **Don't Allow**, the default button, refuses this request and the app's next ones.
- **Allow** answers this request and the app's next ones.

Lokyr remembers your answer for each app, from one start to the next.

## Turning it off

Turn the plugin off, in the settings window's [Plugins page](plugin-manager.md) or with `enabled = false` in [`[plugins.automation]`](../CONFIGURATION.md#pluginsnamespace), and every request of a script or a shortcut fails, of a message that tells why.
