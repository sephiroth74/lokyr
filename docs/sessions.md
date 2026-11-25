# Sessions

<sub>macOS · Since 1.5 · [`[sessions] folders`](CONFIGURATION.md#folders), [`[app] restore-windows`](CONFIGURATION.md#restore-windows)</sub>

A session is a group of windows, such as one for each project. One session shows at a time: when you switch, its windows come back where you left them, and the others hide. The programs in hidden windows keep running, so a build or a server goes on while you work on something else.

## Try it

Choose Window › Sessions › New Session: your windows hide and a new one opens. Press ⌃⌘L: your windows come back. Press ⌃⌘L again to go back to the new session.

Then list your projects. Add this to your config, with the folder that holds your projects:

```toml
[sessions]
folders = ["~/src/*"]
```

Press ⌃⌘S: your projects show after your sessions. Choose one: a session opens there, named after the folder, its shell in the folder.

## What it gives you

- **A session for each project**, each with its own windows, tabs and splits.
- **A switcher**, ⌃⌘S, that finds a session, a [layout](layouts.md) or a project's folder as you type.
- **The last two sessions** a key apart: ⌃⌘L switches back and forth.
- **Names** in the titlebar of each window, before its keys, such as `api · ⌥⌘1`.
- **Sessions that come back** when Lokyr opens your windows again as it starts.
- **Layouts**: a session saved to a file, which opens again as a new session with new shells. [Layouts](layouts.md) covers them.

## Start a session

- **Window › Sessions › New Session** opens a session with one window of a new shell, and hides the windows you had.
- **A folder in the switcher** opens a session there, named after the folder. When a session was opened in that folder already, the switcher switches to it instead.
- **A layout in the switcher** opens a new session named after the layout, with its windows, tabs and splits.

Lokyr starts in a session with no name. A window that you open, with ⌘N or from a folder in the Finder, opens in the session that shows.

## Switch sessions

Press ⌃⌘S, or choose Window › Sessions › Switch Session…, and choose an entry. The switcher lists:

- **Your sessions**, the one you used last first. The session that shows is marked Current. Sessions with no name show as Untitled Session, then Untitled Session 2 and on, in the order you opened them.
- **Your layouts**, each marked Layout.
- **Folders**: those that [`folders`](CONFIGURATION.md#folders) names, then those where your shells worked last, marked Recent. Each shows the folder that holds it, and your home folder shows as `~`.

As you type, the switcher finds sessions and layouts by their names, and folders by their names and paths. The best match comes first: `ui` finds the folder `ui` before Untitled Session. When two match as well, sessions come first, then layouts, then folders.

Press ⌃⌘L, or choose Window › Sessions › Switch to Last Session, to switch to the session you used before. Press it again to come back.

To switch to a session with a key of your own, bind `window.switch-session` with the session's `name`:

```toml
[keys]
"ctrl+cmd+1" = { command = "window.switch-session", args = { name = "api" } }
```

## List your projects' folders

[`folders`](CONFIGURATION.md#folders), in the config's `[sessions]`, lists the folders that the switcher shows:

```toml
[sessions]
folders = ["~/src/*", "~/notes"]
```

- **A path** from `/` or `~` lists that folder.
- **A last part `*`** lists every folder in it, one level down, but those whose names start with a dot.
- **The recent folders** come from your shells: the last 50 folders where they worked, as [shell integration](shell-integration.md) reports them. Lokyr keeps them for its next start.
- **A folder that a session was opened in** shows as that session, not among the folders.

## Name, save and close a session

- **Window › Sessions › Rename Session…** names the session that shows. The name shows in the titlebar of its windows, before their keys.
- **Window › Sessions › Save Session as Layout…** saves the session to a file in `~/.config/lokyr/layouts/`, which the switcher lists. [Layouts](layouts.md) covers the file and the commands that its panes can run.
- **Window › Sessions › Close Session** closes the session's windows. Lokyr asks first while a program other than a shell runs in one of them. The session you used before shows then.

## Settings

```toml
[sessions]
folders = ["~/src/*"]          # the folders that the switcher lists

[app]
restore-windows = "always"     # open your sessions and their windows again at every start
restore-scrollback = true      # their panes show what they showed when you quit
```

- [`[sessions] folders`](CONFIGURATION.md#folders): the folders that the switcher lists.
- [`[app] restore-windows`](CONFIGURATION.md#restore-windows): whether your windows, and with them your sessions, open again as Lokyr starts.
- [`[app] restore-scrollback`](CONFIGURATION.md#restore-scrollback): whether the panes that open again show what they showed when you quit.

## Commands and keys

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| `command-palette.sessions` | Switch Session… | | ⌃⌘S |
| `window.new-session` | New Session | | |
| `window.switch-to-last-session` | Switch to Last Session | | ⌃⌘L |
| `window.switch-session` | Switch to Session | `name`, text | |
| `window.rename-session` | Rename Session… | | |
| `window.save-layout` | Save Session as Layout… | | |
| `window.open-layout` | Open Layout | `name`, text | |
| `window.close-session` | Close Session | | |

[The commands' reference](commands.md#windows) lists them with the other commands of the windows.

## How it works

- **Each window belongs to one session.** A window opens in the session that shows, and a tab or a pane that you drag out of a window opens in that window's session.
- **The quick terminal, the settings window and man pages' windows belong to no session**: they show whichever session you're in. A tab or a pane that you drag out of one of them joins the session that shows.
- **Each session numbers its own windows** from ⌥⌘1, and ⌘\` cycles through the windows of the session that shows. A man page's window takes a number that none of the sessions' windows has.
- **A hidden window draws nothing**, so it costs no frames, but its programs keep running and its panes keep their scrollback in memory. It gives back most of the memory of its picture, and draws itself anew just before it shows again. · not released yet
- **A window in full screen leaves full screen** as its session hides, and goes back into it when its session shows again.
- **A session closes with its last window**, and the session you used before shows. A session with no window goes when you switch away from it.
- **When Lokyr opens your windows again** as it starts, every session comes back with its name, and the one you used last shows. The windows of the other sessions open the first time you switch to each, their shells starting then. Lokyr keeps 100 windows and 256 panes at most as it quits, those of the session that shows first.

## Limits

- **Your programs end when you quit Lokyr.** Your sessions come back with their windows, tabs, splits and folders, and with what their panes showed when [`restore-scrollback`](CONFIGURATION.md#restore-scrollback) is on, but each pane starts a new shell.
- **A window stays in its session**: no command moves it to another one.
- **A window with its tabs in the titlebar doesn't show its session's name.**

## Troubleshooting

### The switcher lists no recent folders

**Why**: the recent folders come from your shells, which report their folder through [shell integration](shell-integration.md).

**Fix**: turn shell integration on, or set up your shell to report its folder with OSC 7, as [the escape codes' reference](escape-codes.md) describes.

**Check**: open a new tab, `cd` into a folder, then press ⌃⌘S: the folder shows, marked Recent.

### A folder doesn't show in the switcher

**Why**: a session was opened in that folder, and shows as that session; or the path in [`folders`](CONFIGURATION.md#folders) starts with neither `/` nor `~`, or names no folder.

**Fix**: choose the session, or correct the path, such as `"~/src/*"` for every folder in `~/src`.

### My sessions don't come back when Lokyr starts

**Why**: [`restore-windows`](CONFIGURATION.md#restore-windows) follows macOS's Close windows when quitting an application, which is on by default.

**Fix**: set `restore-windows = "always"` in your config's `[app]`, or turn that setting off in System Settings › Desktop & Dock.

## See also

- [Windows and tabs](windows-and-tabs.md), for the windows, their numbers and their tabs.
- [Layouts](layouts.md), for a session saved to a file.
- [The command palette](plugins/command-palette.md#the-sessions-switcher), whose switcher lists the sessions.
- [Configuration](CONFIGURATION.md#sessions), for the `[sessions]` section.
