# `links`: ⌘-click opens links and paths

<sub>`lokyr/links` · bundled · [source](../../plugins/links)</sub>

A ⌘-click on text opens what the text points to: the link that the program gave it, with OSC 8, or the URL or the path that the text holds there. While you hold ⌘, the link under the pointer is underlined and the pointer is a pointing hand, so that you see what a click would open.

## What opens

- **A program's link**, as GNU's `ls --hyperlink` writes one for each file: its target, whatever its text says.
- **A URL** of `http`, `https`, `mailto` or `file`, which ends at a space.
- **A path**: from `/`, from `~`, or from the folder of the program in the pane's foreground, as `ls` lists files. A line and a column after it, as compilers and `grep -n` write them, as `src/main.rs:42:7`, belong to the path. Between quotes, a path that exists can hold spaces, as `git status` and Python write them.

A URL or a file asks you first, as [`[links]`](../CONFIGURATION.md#links) says, and its checkbox can stop asking; a folder opens in the Finder at once. A program, an application or a script that would run never runs: it shows in the Finder, selected.

## Commands

| Command | Default keys | What it does |
|---|---|---|
| `links.open` | ⌘-click | Opens the link, the URL or the path at the cell clicked, as above |

## Configuration

The questions that a link asks are the app's own keys, in [`[links]`](../CONFIGURATION.md#links): `urls` and `files`, each `"ask"` by default.

## Turning it off

```toml
[plugins.links]
enabled = false
```

A ⌘-click then selects, as a click does, and holding ⌘ underlines nothing.

## Permissions

It reads the terminal's text around the click and the folder of the program in the foreground, for a relative path, and opens what it finds with its application.
