# `links`: ⌘-click and hints open links and paths

<sub>`lokyr/links` · bundled · [source](../../plugins/links)</sub>

A ⌘-click on text opens what the text points to: the link that the program gave it, with OSC 8, or the URL or the path that the text holds there. While you hold ⌘, the link under the pointer is underlined and the pointer is a pointing hand, so that you see what a click would open. From the keyboard, its hints label what the pane shows, for a letter or two to open it.

## What opens

- **A program's link**, as GNU's `ls --hyperlink` writes one for each file: its target, whatever its text says.
- **A URL** of `http`, `https`, `mailto` or `file`, which ends at a space.
- **A path**: from `/`, from `~`, or from the folder of the program in the pane's foreground, as `ls` lists files. A line and a column after it, as compilers, tests, `grep -n` and AI agents write them, as `src/main.rs:42:7`, `src/app.ts(42,7)` or `src/main.rs#L42`, are underlined with the path, and open the file at that line in the editor that [`[links] editor`](../CONFIGURATION.md#editor) names, VS Code, Cursor, Zed or Sublime Text among them; `"system"`, by default, opens the file in its application. Between quotes, a path that exists can hold spaces, as `git status` and Python write them.

A URL or a file asks you first, as [`[links]`](../CONFIGURATION.md#links) says, and its checkbox can stop asking; a folder opens in the Finder at once. A program, an application or a script that would run never runs: it shows in the Finder, selected.

## Hints · not released yet

Press ⇧⌘E, or choose View › Show Hints: every URL, path that exists, path with its line, git hash and IP address that the pane shows gets a label of one or two letters, drawn over its first letters in the theme's accent, the nearest the cursor of one letter. Type a label to pick its hint:

- **Open**, by default: as a ⌘-click opens it, a path at its line in your editor; a hash or an address, which open nothing, is copied.
- **A capital**, as Shift types it, copies the hint instead.
- **Tab** moves what a pick does, shown before the small field at the pane's corner: Open, Copy, Type, which types the hint at the prompt as if you typed it, and Select; ⇧Tab goes back.

Typing a label's first letter keeps the labels that start with it; a letter that none follows with starts again. Escape, or a click elsewhere, takes the labels away.

Your own patterns join them, as a ticket's number that opens its page:

```toml
[plugins.links.hints.jira]
pattern = "PROJ-\\d+"
url = "https://jira.example.com/browse/$0"
```

`$0` is the whole match and `$1` to `$9` its groups; a pattern without a `url` is a hint that its opening copies. The [`hints`](../CONFIGURATION.md#hints) and [`alphabet`](../CONFIGURATION.md#alphabet) keys of the configuration guide tell more. The labels' colors are the theme's `hints.label.bg` and `hints.label.fg`.

## Commands

| Command | Default keys | What it does |
|---|---|---|
| `links.open` | ⌘-click | Opens the link, the URL or the path at the cell clicked, as above |
| `links.copy` | | Copies what `links.open` would open at the cell: the link, the URL, or the path from `/` |
| `links.new-window` | | Opens a window whose shell starts in the folder of the path at the cell, or in the folder that holds its file |
| `links.new-tab` | | Opens a tab, after the others, as `links.new-window` does a window |
| `links.hints` | ⇧⌘E | Shows the hints of the pane, a pick doing its `action`: `"open"` by default, `"copy"`, `"type"` or `"select"` |
| `links.hints-action` | Tab, ⇧Tab in the hints | Moves what a pick does by its `delta`, through open, copy, type and select |

The first four take the cell's `column` and `row`; the pane's context menu lists them, as Open Link, Copy Link Address, New Window at Folder and New Tab at Folder, each dimmed where the cell holds nothing that it takes: a URL of the web has no folder. A link's `file:` URL of this Mac, of no host, of `localhost` or of the Mac's name, as `ls --hyperlink`, ripgrep and `fd` write it, opens and has its folder; one of another machine opens not.

## Configuration

The questions that a link asks are the app's own keys, in [`[links]`](../CONFIGURATION.md#links): `urls` and `files`, each `"ask"` by default. The hints' are the plugin's, in [`[plugins.links]`](../CONFIGURATION.md#pluginslinks): `hints`, your patterns, and `alphabet`, the letters of the labels.

## Turning it off

```toml
[plugins.links]
enabled = false
```

A ⌘-click then selects, as a click does, and holding ⌘ underlines nothing.

## Permissions

It reads the terminal's text around the click, and the pane's view for its hints, and the folder of the program in the foreground, for a relative path; draws the hints' labels over the pane, opens what it finds with its application, writes the clipboard, as Copy Link Address copies, selects, and types a hint into the pane, as the hints' Type does.
