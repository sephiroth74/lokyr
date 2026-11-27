# `command-palette`: the command palette

<sub>`lokyr/command-palette` · bundled · [source](../../plugins/command-palette)</sub>

A panel near the top of the window that lists Lokyr's commands, the plugins' too, ranks them as you type, and runs the one you choose: a large search field, «Execute Command…», above a thin line and the list.

## The palette

- **⇧⌘P** opens it, its field taking the keys; ⇧⌘P on an open palette gives its field the keys again.
- **It lists** the commands that run without an argument and that the menus enable now: Copy shows only where there is a selection, as its menu item does. Each shows its title, and its keys at its end, as key caps · Since 1.4.
- **It lists the menu bar's items too** that run a command with an argument, as the Shell menu's Split Right, Split Down and Select Pane Above, each with its title and keys as the menu shows them: `split right` finds Split Right, ⌘D. An item that the menu dims is left out, as are the context menu's items, which act on the cell that you click. · not released yet
- **As you type**, it ranks them by a fuzzy match of your letters in their order, by each command's title first, then its name, as `tabs.new`, its keys, as `⌘T`, and its plugin: `nt` finds New Tab, `⌘F` finds Find…. A word's first letters and letters in a row count more.
- **The commands that you ran last** come first while the field is empty, and before the others of the same match as you type; Lokyr keeps the last twenty.
- **The first command is selected** as it opens; **↑ and ↓** move through the list while you type; **Return**, or a click on a command, runs it and closes the palette. The command under the pointer is highlighted.
- **Escape**, or a click outside it, closes it, running nothing.

## The theme picker

- **Choose Theme…**, in the View menu and in the palette, opens the palette of the themes of the appearance in force, light or dark: the files of your themes' folder, the themes that Lokyr holds and its own two, the one in force first, marked Current.
- **As you move** through the list, or type, the selected theme shows at once, in place of the one in force.
- **Return**, or a click, names it in your config's `[theme]`, `light` or `dark` as the appearance is; **Escape** shows the theme in force again. To choose the other appearance's theme, switch the appearance first, in View › Appearance.

## The agents' list

- **Window › Show Agents…**, ⌥⇧⌘A, opens the palette of the AI agents of every window, as [`[agents]`](../CONFIGURATION.md#agents) finds them: those that wait for you first, the longest waiting at the top, then those done, those at work, and those that have shown nothing yet.
- **Each** shows its program and what it asked, as "claude: Claude needs your permission to use Bash", or its tab's title, and at its end its state and how long it has been in it.
- **As you type**, it finds them by their programs, their messages and their tabs' titles; **Return**, or a click, brings the chosen agent's pane forward, with its tab and its window.

## The sessions' switcher

- **Window › Sessions › Switch Session…**, ⌃⌘S, opens the palette of your [sessions](../sessions.md): those open first, the one you used last at the top, the one that shows marked Current; then your layouts; then folders, those that [`[sessions] folders`](../CONFIGURATION.md#sessions) lists, then those where your shells worked last, marked Recent, each with the folder that holds it, your home folder as `~`. Sessions with no name show as Untitled Session, then Untitled Session 2 and on, in the order you opened them. · Since 1.5
- **As you type**, it finds the sessions by their names, the layouts by theirs, and the folders by their names and paths, the best match first: `ui` finds the folder `ui` before Untitled Session. When two match as well, the sessions come first, then the layouts, then the folders.
- **Return**, or a click, switches to the session you chose. A folder opens a session there, named after it, its first window's shell in the folder; a folder that a session was opened in already switches to that session instead, and doesn't show among the folders.

## Find in All Tabs

- **Edit › Find in All Tabs…**, ⇧⌘F, opens the palette as a search of every pane of the session's windows, the hidden tabs' too · not released yet
- **As you type**, it searches them all, nearest the bottom of each pane first, and lists each match as a row: the line that holds it, and its tab's title at the row's end. The rows go in the order of the windows, their tabs and their panes, then of the lines. It lists 1000 matches at most.
- **Case counts** when your text has a capital letter, as in the find bar.
- **Return**, or a click, brings the match's pane forward, with its tab and its window, selects the match, and opens the [find bar](search.md) there with your text, the match its current one, so that ⌘G goes on from it.
- **Escape**, or a click outside it, closes it, and the searches end.

## Commands

<!-- commands: lokyr/command-palette -->

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `command-palette.open` | View › Command Palette… | ⇧⌘P | Opens the palette in the window that has the focus |
| `command-palette.theme` | View › Choose Theme… | | Opens the theme picker, of the themes of the appearance in force |
| `command-palette.agents` | Window › Show Agents… | ⌥⇧⌘A | Opens the agents' list, of every window's AI agents, the most urgent first |
| `command-palette.sessions` | Window › Sessions › Switch Session… | ⌃⌘S | Opens the sessions' switcher, of the sessions open, then the folders of your config and your shells · Since 1.5 |
| `command-palette.find-all` | Edit › Find in All Tabs… | ⇧⌘F | Opens Find in All Tabs, which searches every pane of the session's windows as you type · not released yet |
| `command-palette.move` | — | ↑, ↓ while `command-palette.focused` | Moves the list's selection, by its argument `delta`, a whole number of items |

<!-- commands end -->

## Turning it off

```toml
[plugins.command-palette]
enabled = false
```

The palette and its commands go, with their keys: ⇧⌘P then does nothing in Lokyr.

## Permissions

It runs any command as yours, as the menus and the keys do, and reads the whole list of the commands, with each one's plugin, keys and context, and the menu bar's items: the capability `user-commands`, of a very high risk, as running any command can type into the shell. A palette of a third party would ask for it when you install it. Its theme picker previews and names a theme, of `appearance-write`. Its agents' list reads the agents of the panes, which tell the programs that run, of `process-read`, and brings a pane, its tab and its window forward, of `workspace-write`. Its sessions' switcher reads the folders where your shells worked, of `process-read`, and switches sessions or opens one, of `workspace-write`. Find in All Tabs reads the panes' text, of `terminal-read`, and selects the match that you choose, of `pane-view` · not released yet.
