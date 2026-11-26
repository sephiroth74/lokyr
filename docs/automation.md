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

- **Windows, tabs and splits**, each in a folder of your choice, with a command line that its shell runs as it starts and environment variables of your own.
- **A command line run** in a terminal's shell, such as `ls -la`, at its prompt.
- **Mouse events** over a cell of a terminal, for a program that uses the mouse, such as a click in an editor or the wheel in a file manager.
- **Text sent to a terminal**, as if you typed it, such as a command and Return, and **keys**, such as ⌃C, the arrows or ⌘A, as you press them.
- **Any of Lokyr's commands**, run by its name and arguments, as the [command palette](plugins/command-palette.md) runs it.
- **A window, a tab or a terminal** brought forward or closed, and terminals found by their title or their folder.
- **The [quick terminal](plugins/quick-terminal.md)** shown or hidden from a shortcut.
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
- **`contents`** is the text that the terminal's screen shows, and **`selected text`** the text of its selection. Lokyr reads them for an app that you allowed, as [Allow an app to control Lokyr](#allow-an-app-to-control-lokyr) says.

Its commands:

| Command | What it does | Result |
|---|---|---|
| `create window [with command text] [in folder text] [with environment list]` | Opens a window | the window |
| `create tab [in window] [with command text] [in folder text] [with environment list]` | Opens a tab, in the window in front unless you give one | the tab |
| `split [terminal] [direction right/left/down/up] [with command text] [in folder text] [with environment list]` | Splits a terminal, the focused one unless you give one, on the right unless you give a direction | the new terminal |
| `input text text [to terminal]` | Sends text to a terminal, the focused one unless you give one, as if you typed it | |
| `execute text [to terminal]` | Runs a command line in a terminal's shell, the focused one unless you give one, at its prompt | |
| `send key text [to terminal]` | Presses a key in a terminal, the focused one unless you give one, as you press it in Lokyr, such as `"ctrl+c"` or `"cmd+a"` | |
| `focus window/tab/terminal` | Brings it forward | |
| `close window/tab/terminal` | Closes it | |
| `perform action text [with arguments record]` | Runs one of Lokyr's commands by its name, with its arguments | |

A folder is a path from `/` or `~`. A command line runs in the shell, so it can hold pipes and variables, and it shows in the shell's history. `with environment` takes a list of environment variables, each `NAME=value`, which the new shell starts with:

```applescript
tell application "Lokyr"
    create tab with environment {"AWS_PROFILE=staging", "EDITOR=vim"}
end tell
```

This script opens a window, splits it, runs a command in the new terminal and reads what it shows:

```applescript
tell application "Lokyr"
    set w to create window in folder "/tmp"
    set t to split terminal 1 of tab 1 of w direction down
    delay 1
    execute "echo hello" to t
    delay 1
    get contents of t
end tell
```

`perform action` takes the names and arguments of [the commands' reference](commands.md). It runs the command as if you chose it in Lokyr, on the window with the focus, so bring Lokyr forward first with `activate`:

```applescript
tell application "Lokyr"
    activate
    perform action "splits.split" with arguments {direction:"right"}
    perform action "tabs.goto" with arguments {index:1}
end tell
```

A field named after one of AppleScript's own words, such as `to`, goes between bars: `{|to|:"bottom"}`.

Script Editor shows the whole dictionary: choose File › Open Dictionary… and choose Lokyr.

## Use Lokyr's actions in Shortcuts

| Action | Parameters | What it does |
|---|---|---|
| New Window | Folder, Command, Environment Variables | Opens a window |
| New Tab | Folder, Command, Environment Variables | Opens a tab in Lokyr's window in front |
| Split Pane | Terminal, Direction, Folder, Command, Environment Variables | Splits a terminal, the focused one unless you choose one |
| Run Command Line | Command Line, Terminal | Runs a command line, such as `ls -la`, in a terminal's shell, at its prompt |
| Send Text | Text, Terminal | Sends text to a terminal, as if you typed it |
| Send Mouse Button Event | Action (Click, Press, Release), Button, Column, Row, Modifiers, Terminal | Clicks, presses or releases a mouse button over a cell of a terminal |
| Send Mouse Position Event | Column, Row, Button Held, Modifiers, Terminal | Moves the mouse's pointer over a cell of a terminal |
| Send Mouse Scroll Event | Direction, Steps, Column, Row, Modifiers, Terminal | Turns the mouse's wheel over a terminal |
| Send Keyboard Event | Key, Modifiers, Terminal | Presses a key in a terminal as you press it in Lokyr, such as A with Command to select all, or C with Control to interrupt a program |
| Focus Terminal | Terminal | Brings a terminal forward, with its window and its tab |
| Close Terminal | Terminal | Closes a terminal |
| Find Terminal | filters on Title, Folder, Tab ID and Window ID | Returns the terminals that match, such as those whose folder contains `src` |
| Get Details of Terminal | Terminal | Returns a terminal's title, folder, tab ID or window ID |
| Open Quick Terminal | | Shows the [quick terminal](plugins/quick-terminal.md), opening it first |
| Close Quick Terminal | | Hides the quick terminal, its shell still running |
| Run Command | Command, Arguments | Runs one of Lokyr's commands by its name, such as `tabs.next` |
| Run Palette Command | Command, Arguments | Runs an entry of the command palette, chosen from its list, such as Split Right, with the entry's arguments |
| Switch Session | Session | Switches to the session of that name, and opens a new session of that name when there is none |
| Open Layout | Layout | Opens a saved layout as a new session |
| Get Terminal Text | Terminal | Returns the text that a terminal shows |
| Get Selected Text | Terminal | Returns the text of a terminal's selection |

- **A Terminal parameter** lists Lokyr's terminals, each with its title and its folder. Leave it empty for the focused terminal, where the action allows it, or pass the terminals that Find Terminal returns. The focused terminal is the one with the keyboard in the window in front: the window that you used last, while you work in the Shortcuts app or another app.
- **Environment Variables** is a list of texts, each `NAME=value`, such as `AWS_PROFILE=staging`, which the new shell starts with.
- **Column and Row** count a terminal's cells from 1, at the top left of its screen.
- **Arguments** go one a line, as `name=value`, such as `index=2` for `tabs.goto`. Lokyr reads a number or `true` and `false` as the kind that the command takes. In Run Palette Command, they add to the entry's own, or replace them.
- **Send Text, Close Terminal, the quick terminal's actions and the Get actions** work while Lokyr stays behind. The others bring Lokyr forward.
- **Spotlight** finds New Window and New Tab: type "New Lokyr window" or "New Lokyr tab".

## Allow an app to control Lokyr

An app that controls Lokyr can type into your terminals, run commands there, press keys and read what they show, which may hold passwords and other secrets. The first time a script or a shortcut sends Lokyr a request, of any kind, Lokyr asks you whether to allow the app that runs it:

- **Don't Allow**, the default button, refuses this request and the app's next ones.
- **Allow** answers this request and the app's next ones, and the app reads your terminals with no more questions.

Lokyr tells apps apart by their code signature, not by the names that they give, so another app can't take an allowed app's answer. It remembers your answer for each app, from one start to the next.

- **A script that runs from another app's terminal**, such as `osascript` in a terminal of your editor, asks as that program in that app: "osascript in Visual Studio Code".
- **A program that runs in one of Lokyr's own terminals** asks for itself, and Lokyr's answer holds while that program runs alone: each new program asks again, so that a script in one terminal can't drive your other terminals, a root shell or an ssh session, without your answer.
- **Shortcuts** asks once for all your shortcuts, Siri and Spotlight.

While the question waits for your answer, a script's request fails with "Lokyr asks whether … may control it": answer, then run the script again. A shortcut waits for your answer.

> [!IMPORTANT]
> Allow only the apps you trust: an app that controls Lokyr can run any command as you. For an app of another developer's, macOS asks you too before its first command, and System Settings › Privacy & Security › Automation lists the apps you allowed there.

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
- **The window in front** is the one with the focus, or, while Lokyr is behind the app that runs the script or the shortcut, the window that you used last. A command of no terminal, a new tab and a question go to it.
- **A command line** that you give a new terminal is typed into its shell as it starts, followed by Return.
- **A command line that you run** in a terminal, with `execute` or Run Command Line, is pasted at the shell's prompt and followed by Return, while the shell waits at an empty prompt, as [shell integration](shell-integration.md) reports it. Lokyr refuses it otherwise, such as while a program runs or when the command line holds text, so that it never types into another program.
- **A key** goes as you press it in Lokyr. The terminal takes the focus first. A key of one of Lokyr's shortcuts runs its command, as ⌘A selects all and ⌘T opens a tab. Any other key goes to the program as the keyboard sends it, following the modes that the program asked for: ↑ moves the cursor in an editor that takes the cursor keys in their application mode, and a program of the keyboard protocol gets the key as that protocol encodes it. A key is a character, or a name: `enter`, `tab`, `backspace`, `escape`, `space`, `delete`, `insert`, `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`, or `f1` to `f35`. ⌥ acts as Meta, so `alt+f` sends Escape and `f`. A ⌘ key of no shortcut reaches a program that uses the keyboard protocol, and no other.
- **A mouse event** goes to the program as the mouse sends it, in the reports and the encoding that the program asked for. A program that doesn't use the mouse gets nothing, but the wheel: its steps scroll the terminal's view a row each, or, on a full-screen program's screen that asks for it, send the arrow keys. A cell off the screen is refused.
- **Environment variables** that you give a new terminal come over Lokyr's own, such as `TERM`. [Shell integration](shell-integration.md) keeps the variables that it needs, and takes your `ZDOTDIR` as zsh's own folder. A terminal takes 64 variables at most.
- **The commands that `perform action` and Run Command run** act as if you chose them, on the window with the focus: closing a tab where a program other than a shell runs asks you first. While Lokyr is behind another app, no window has the focus, and a command such as `tabs.next` does nothing. Run Command brings Lokyr forward by itself.
- **A request waits 10 seconds at most** for Lokyr's answer, then fails.

## Limits

- **The ids last while Lokyr runs.** After a restart, a script finds its windows again by their names or their places.
- **A script can't wait for a command to end** or for its output. Use `delay`, then read `contents`.
- **`contents` is the screen's text**, not the scrollback above it.
- **Mouse events are Shortcuts' alone**: AppleScript's dictionary has none.
- **`perform action` and Run Command report a command that Lokyr doesn't have**, but not a command that fails as it runs, such as a switch to a session that isn't there. That failure shows in a card in Lokyr's window, and in its log.
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

### "may not control Lokyr"

**Why**: you clicked Don't Allow when Lokyr asked whether the app may control it, and Lokyr remembers it.

**Fix**: quit Lokyr, then remove the file where it keeps the answers. Lokyr asks again about every app.

```sh
rm ~/Library/Application\ Support/lokyr/state/plugins/automation.json
```

### "the shell of the pane … is not at its prompt"

**Why**: `execute` and Run Command Line run a line at the shell's prompt alone, and the shell was running a program, or [shell integration](shell-integration.md) is off, so that Lokyr can't see its prompt.

**Fix**: wait for the program to end, or turn shell integration on. To type into a program, use `input text` or Send Text, with a return character at the end.

### `perform action` does nothing

**Why**: Lokyr was behind another app, so no window had the focus for the command to act on. Or the command failed as it ran.

**Fix**: add `activate` before `perform action`. When it still does nothing, read the card in Lokyr's window, or the end of its log:

```sh
tail ~/Library/Logs/Lokyr/lokyr.log
```

### "Lokyr's automation did not answer in time"

**Why**: Lokyr waits 10 seconds at most for each request, and a shortcut's request almost two minutes while Lokyr's question waits for your answer.

**Fix**: run the script or the shortcut again, and answer the question in time.

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
