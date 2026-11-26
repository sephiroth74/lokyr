# Automation

<sub>macOS · not released yet · on by default · the [`automation`](plugins/automation.md) plugin · [`[plugins.automation] enabled`](CONFIGURATION.md#pluginsnamespace)</sub>

Scripts and shortcuts can drive Lokyr: open windows, tabs and splits in a folder, run a command in them, send them text, switch sessions, and read what a terminal shows. AppleScript sees Lokyr's windows, their tabs and each tab's terminals, and the Shortcuts app has Lokyr's actions, which Spotlight can run too.

## Try it

Run this in a terminal:

```sh
osascript -e 'tell application "Lokyr" to create window in folder "/tmp" with command "ls"'
```

A window opens in `/tmp`, and its shell runs `ls`. The first time, macOS may ask whether the app where you ran `osascript` may control Lokyr: click OK.

Then open the Shortcuts app, make a new shortcut, and type Lokyr in the search field of its actions. Add New Tab, type `ls` in its Command field, and click the run button: a tab opens in Lokyr's window in front and runs `ls`.

## What it gives you

- **Windows, tabs and splits**, each in a folder of your choice, with a command line that its shell runs as it starts.
- **Text sent to a terminal**, as if you typed it, such as a command and Return.
- **Any of Lokyr's commands**, run by its name and arguments, as the [command palette](plugins/command-palette.md) runs it.
- **A window, a tab or a terminal** brought forward or closed.
- **The [sessions](sessions.md)**: Shortcuts switches to a session by its name, and opens a saved [layout](layouts.md).
- **What a terminal shows**, or its selection, once you allow the app that asks.

## Script Lokyr with AppleScript

Lokyr's dictionary has three kinds of objects, each inside the one before:

| Object | Properties | Inside |
|---|---|---|
| `window` | `id`, `name` | the application, the one in front first |
| `tab` | `id`, `name` | its window, in their order |
| `terminal` | `id`, `name`, `working directory`, `contents`, `selected text` | its tab: the tab's panes |

- **`id`** stays the same while Lokyr runs, so a script can keep it from one command to the next.
- **`name`** is the title that the window, the tab or the terminal shows.
- **`working directory`** is the folder where the terminal's shell works, as [shell integration](shell-integration.md) reports it.
- **`contents`** is the text that the terminal's screen shows, and **`selected text`** the text of its selection. Lokyr asks you first, as [Allow an app to read your terminals](#allow-an-app-to-read-your-terminals) says.

Its commands:

| Command | What it does | Result |
|---|---|---|
| `create window [with command text] [in folder text]` | Opens a window | the window |
| `create tab [in window] [with command text] [in folder text]` | Opens a tab, in the window in front unless you give one | the tab |
| `split [terminal] [direction right/left/down/up] [with command text] [in folder text]` | Splits a terminal, the focused one unless you give one, on the right unless you give a direction | the new terminal |
| `input text text [to terminal]` | Sends text to a terminal, the focused one unless you give one, as if you typed it | |
| `focus window/tab/terminal` | Brings it forward | |
| `close window/tab/terminal` | Closes it | |
| `perform action text [with arguments record]` | Runs one of Lokyr's commands by its name, with its arguments | |

A folder is a path from `/` or `~`. A command line runs in the shell, so it can hold pipes and variables, and it shows in the shell's history.

This script opens a window, splits it, runs a command in the new terminal and reads what it shows:

```applescript
tell application "Lokyr"
    set w to create window in folder "/tmp"
    set t to split terminal 1 of tab 1 of w direction down
    input text "echo hello" & return to t
    delay 1
    get contents of t
end tell
```

`perform action` takes the names and arguments of [the commands' reference](commands.md):

```applescript
tell application "Lokyr"
    perform action "splits.split" with arguments {direction:"right"}
    perform action "window.switch-session" with arguments {name:"api"}
end tell
```

A field named after one of AppleScript's own words, such as `to`, goes between bars: `{|to|:"bottom"}`.

Script Editor shows the whole dictionary: choose File › Open Dictionary… and choose Lokyr.

## Use Lokyr's actions in Shortcuts

| Action | Parameters | What it does |
|---|---|---|
| New Window | Folder, Command | Opens a window |
| New Tab | Folder, Command | Opens a tab in Lokyr's window in front |
| Split Pane | Terminal, Direction, Folder, Command | Splits a terminal, the focused one unless you choose one |
| Send Text | Text, Terminal | Sends text to a terminal, as if you typed it |
| Run Command | Command | Runs one of Lokyr's commands by its name, such as `tabs.next` |
| Switch Session | Session | Switches to the session of that name, and opens a new session of that name when there is none |
| Open Layout | Layout | Opens a saved layout as a new session |
| Get Terminal Text | Terminal | Returns the text that a terminal shows |
| Get Selected Text | Terminal | Returns the text of a terminal's selection |

- **A Terminal parameter** lists Lokyr's terminals, each with its title and its folder. Leave it empty for the focused terminal.
- **Send Text and the two Get actions** work while Lokyr stays behind. The others bring Lokyr forward.
- **Spotlight** finds New Window and New Tab: type "New Lokyr window" or "New Lokyr tab".

## Allow an app to read your terminals

What a terminal shows may hold passwords and other secrets. The first time a script or a shortcut asks for a terminal's text, Lokyr asks you whether to allow the app that runs it, such as Script Editor or Shortcuts:

- **Don't Allow**, the default button, refuses this request and the app's next ones.
- **Allow** answers this request and the app's next ones.

Lokyr remembers your answer for each app, from one start to the next.

> [!IMPORTANT]
> An app that may control Lokyr can type into your terminals, and so run any command as you. macOS asks you before an app sends Lokyr its first command: allow only the apps you trust. System Settings › Privacy & Security › Automation lists the apps you allowed.

## Turn it off

Turn the [`automation`](plugins/automation.md) plugin off, in the settings window's [Plugins page](plugins/plugin-manager.md), or in your config:

```toml
[plugins.automation]
enabled = false   # every script and shortcut fails, with a message that says why
```

## Settings

- [`[plugins.automation] enabled`](CONFIGURATION.md#pluginsnamespace): whether scripts and shortcuts can drive Lokyr.

## How it works

- **Lokyr's bundled [`automation`](plugins/automation.md) plugin answers** what AppleScript and the Shortcuts app ask. Lokyr hands it each request and waits for its answer.
- **A new window, tab or split answers once it opens**, with its object, so that the script's next command can use it.
- **A command line** that you give a new terminal is typed into its shell as it starts, followed by Return.
- **The commands that `perform action` and Run Command run** act as if you chose them: closing a tab where a program other than a shell runs asks you first.
- **A request waits 10 seconds at most** for Lokyr's answer, then fails.

## Limits

- **The ids last while Lokyr runs.** After a restart, a script finds its windows again by their names or their places.
- **A script can't wait for a command to end** or for its output. Use `delay`, then read `contents`.
- **`contents` is the screen's text**, not the scrollback above it.
- **A new terminal always starts your shell**: a command line runs in it, and shows in its history.
- **Shortcuts lists the actions of one copy of Lokyr**: the one that macOS registered for its identifier, usually the one in your Applications folder.

## Troubleshooting

### "Not authorized to send Apple events to Lokyr. (-1743)"

**Why**: you clicked Don't Allow when macOS asked whether the app that runs the script may control Lokyr.

**Fix**: open System Settings › Privacy & Security › Automation, find the app that runs the script, such as Terminal or Script Editor, and turn Lokyr on under it.

**Check**: run the script again: Lokyr opens a window.

```sh
osascript -e 'tell application "Lokyr" to create window'
```

### "may not read the text of Lokyr's panes"

**Why**: you clicked Don't Allow when Lokyr asked whether the app may read your terminals' text, and Lokyr remembers it.

**Fix**: quit Lokyr, then remove the file where it keeps the answers. Lokyr asks again about every app.

```sh
rm ~/Library/Application\ Support/lokyr/state/plugins/automation.json
```

### "Lokyr's automation did not answer in time"

**Why**: Lokyr waits 10 seconds at most for each request. A question about reading a terminal's text that you leave open longer fails the request.

**Fix**: run the script again, and answer the question within 10 seconds.

### "Lokyr's automation is off"

**Why**: the [`automation`](plugins/automation.md) plugin is turned off.

**Fix**: turn it on in the settings window's Plugins page, or remove `enabled = false` from your config's `[plugins.automation]`.

### Shortcuts doesn't list Lokyr's actions

**Why**: Shortcuts reads the actions of the copy of Lokyr that macOS registered, when that copy first opens. Another copy, such as an older one in your Downloads folder, may hold the registration.

**Fix**: quit Lokyr, remove the other copies, and open Lokyr from your Applications folder.

**Check**: type Lokyr in the search field of the Shortcuts app's actions: the list shows New Window.

## See also

- [The `automation` plugin](plugins/automation.md), which answers the scripts and shortcuts.
- [The commands' reference](commands.md), for the names and arguments that `perform action` takes.
- [Sessions](sessions.md) and [Layouts](layouts.md), which Shortcuts switches to and opens.
