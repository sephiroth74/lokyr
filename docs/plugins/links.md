# `links`: ⌘-click and hints open links and paths

<sub>`lokyr/links` · bundled · [source](../../plugins/links)</sub>

A ⌘-click on text opens what the text points to: the link that the program gave it, with OSC 8, or the URL or the path that the text holds there. From the keyboard, its hints label what the pane shows, for a letter or two to open it. [Links](../links.md) shows how to use them.

## What it does

- **⌘-click** opens the link, the URL or the path under the pointer, and a path with its line, such as `src/main.rs:42`, at that line in your editor, as [Open a link with ⌘-click](../links.md#open-a-link-with--click) and [Open a path at its line](../links.md#open-a-path-at-its-line-in-your-editor) show. While you hold ⌘, the link under the pointer is underlined and the pointer is a pointing hand.
- **The pane's context menu** opens the link, copies its address, or opens a window or a tab in its folder, as [Use the context menu](../links.md#use-the-context-menu) shows.
- **Hints**, ⇧⌘E, label every URL, path, git hash and IP address on the screen, for a letter or two to open, copy, type or select it, as [Label links with hints](../links.md#label-links-with-hints) shows. Patterns of your own join them, as [Add your own patterns](../links.md#add-your-own-patterns) shows. · not released yet

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
