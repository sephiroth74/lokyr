# `links`: ⌘-click and hints open links and paths

<sub>`lokyr/links` · bundled · [source](../../plugins/links)</sub>

A ⌘-click on text opens what the text points to: the link that the program gave it, with OSC 8, or the URL or the path that the text holds there. While you hold ⌘, the link under the pointer is underlined and the pointer is a pointing hand, so that you see what a click would open. From the keyboard, its hints label what the pane shows, for a letter or two to open it.

## What opens

- **A program's link**, as GNU's `ls --hyperlink` writes one for each file: its target, whatever its text says.
- **A URL** of `http`, `https`, `mailto` or `file`, which ends at a space.
- **A path**: from `/`, from `~`, or from the folder of the program in the pane's foreground, as `ls` lists files. A line and a column after it, as compilers, tests, `grep -n` and AI agents write them, as `src/main.rs:42:7`, `src/app.ts(42,7)` or `src/main.rs#L42`, are underlined with the path, and open the file at that line in the editor that [`[links] editor`](../CONFIGURATION.md#editor) names, VS Code, Cursor, Zed or Sublime Text among them; `"system"`, by default, opens the file in its application. Between quotes, a path that exists can hold spaces, as `git status` and Python write them.

A URL or a file asks you first, as [`[links]`](../CONFIGURATION.md#links) says, and its checkbox can stop asking; a folder opens in the Finder at once. A program, an application or a script that would run never runs: it shows in the Finder, selected.

## Hints

<sub>Platforms: macOS · not released yet</sub>

Press ⇧⌘E, or choose View › Show Hints: every URL, every path that exists, with or without a line after it, every git hash and every IP address that the pane shows gets a label of one or two letters, drawn over its first letters in the theme's accent, the nearest the cursor of one letter. A URL right after a bracket, a quote or a `=`, such as in Markdown's `[docs](https://…)`, HTML's `href="https://…"` or a line of JSON, gets its own label, and so does a path with spaces between quotes or after backslashes. Type a label to pick its hint:

- **Open**, by default: as a ⌘-click opens it, a path at its line in your editor; a hash or an address, which open nothing, is copied.
- **A capital**, as Shift types it, copies the hint instead.
- **Tab** moves what a pick does, shown before the small field at the pane's corner: Open, Copy, Type, which types the hint at the prompt as if you typed it, and Select; ⇧Tab goes back.

Typing a label's first letter keeps the labels that start with it; a letter that none follows with starts again. Escape, or a click elsewhere, takes the labels away.

The labels' colors are the theme's `hints.label.bg` and `hints.label.fg`. The [`alphabet`](../CONFIGURATION.md#alphabet) key sets the labels' letters.

### Add your own patterns

<sub>Platforms: macOS · not released yet</sub>

Besides URLs, paths, hashes and addresses, the hints can label text of your own, such as your issue tracker's ticket numbers, and open a page for each. Each pattern is a table of the config, under `[plugins.links.hints]`, named as you like:

```toml
# A ticket number, such as PROJ-1234, opens the ticket's page.
[plugins.links.hints.tickets]
pattern = 'PROJ-\d+'
url = "https://jira.example.com/browse/$0"
```

- **The table's name**, `tickets` here, is yours to choose. Lokyr uses it in the notices about that pattern, and when two patterns match the same text, the one whose name comes first in alphabetical order wins.
- **`pattern`** is a [regular expression](https://docs.rs/regex/latest/regex/#syntax). It is case-sensitive; start it with `(?i)` to ignore case. Write it between single quotes, as above, so that TOML keeps its backslashes as they are. Between double quotes, each backslash is doubled: `"PROJ-\\d+"`.
- **`url`** is what picking the hint opens. In it, `$0` stands for the whole text that the pattern matched, `$1` to `$9` for the parts of the pattern between parentheses, and `$$` for a `$`. It must open an `http`, `https`, `mailto` or `file` URL, or a path from `/` or `~`. A `url` that starts with another scheme, such as `vscode:` or `jira:`, is left out, and a notice reports it.
- **Without a `url`**, picking the hint copies its text.

Try it: add the table above to your config and save it, then run this command and press ⇧⌘E:

```sh
echo "Fixed in PROJ-42, see PROJ-7 too"
```

`PROJ-42` and `PROJ-7` get a label each, and typing a label opens `https://jira.example.com/browse/PROJ-42` or `https://jira.example.com/browse/PROJ-7`.

When only part of the match goes into the URL, put that part between parentheses and use `$1`. The ticket's number alone, for example:

```toml
# PROJ-1234 opens https://tracker.example.com/issue?id=1234.
[plugins.links.hints.tracker]
pattern = 'PROJ-(\d+)'
url = "https://tracker.example.com/issue?id=$1"
```

More patterns:

```toml
# A GitHub issue, such as #128, of one repository.
[plugins.links.hints.issues]
pattern = '#(\d+)'
url = "https://github.com/acme/app/issues/$1"

# A UUID, which picking copies.
[plugins.links.hints.uuids]
pattern = '(?i)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}'
```

A pattern that doesn't read is left out, and a notice reports why. A pick that fails, such as a `url` that starts with `$0` and turns into a URL that doesn't open, shows a notice too, and the labels go. The [`hints`](../CONFIGURATION.md#hints) key of the configuration guide lists the same rules.

## Commands

<!-- commands: lokyr/links -->

| Command | Default keys | What it does |
|---|---|---|
| `links.open` | ⌘-click | Opens the link, the URL or the path at the cell clicked, as above |
| `links.copy` | | Copies what `links.open` would open at the cell: the link, the URL, or the path from `/` |
| `links.new-window` | | Opens a window whose shell starts in the folder of the path at the cell, or in the folder that holds its file |
| `links.new-tab` | | Opens a tab, after the others, as `links.new-window` does a window |
| `links.hints` | ⇧⌘E | Shows the hints of the pane, a pick doing its `action`: `"open"` by default, `"copy"`, `"type"` or `"select"` · not released yet |
| `links.hints-action` | Tab, ⇧Tab while `links.focused` | Moves what a pick does by its `delta`, through open, copy, type and select · not released yet |

<!-- commands end -->

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
