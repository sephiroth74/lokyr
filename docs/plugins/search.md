# `search`: the find bar

<sub>`lokyr/search` · bundled · [source](../../plugins/search)</sub>

A bar at the top right of the pane that searches its screen and its scrollback as you type, highlights every match that shows, and steps through them.

## The bar

- **⌘F** opens it, of your last search's text, selected, so that typing replaces it, and of its options; ⌘F on an open bar gives its field the keys again.
- **As you type**, it searches the pane, the rows that show first, and counts the matches, as `3 of 12`, numbers past 9999 short, as `12K`. A search of 100 000 lines counts them all in about 25 ms.
- **The matches** that show are highlighted, the current one in a color of its own, over each cell's own colors.
- **The scroll bar** marks every match along it, the current one in its own color, and stays shown while the bar's search shows them, as [`scrollbar`](../CONFIGURATION.md#scrollbar) tells.
- **It goes from the bottom up**, as the newest output is at the bottom: the first match is the one nearest the bottom, the next one the one above it, the previous one the one below it. The view follows the current match, and stays there while the program prints on.
- **The toggles** at the field's end count case, take whole words alone, and read the text as a regular expression, of the syntax of Rust's `regex`; they keep their states for the next search.
- **The magnifying glass** at the field's start lists your last twenty searches, which Lokyr keeps.
- **Escape**, or its close button, closes it: the highlights go, and the current match is selected, ready for ⌘C.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `search.open` | Edit › Find… | ⌘F | Opens the bar at the pane that has the keyboard |
| `search.next` | Edit › Find Next | ⌘G, and Return in the bar | Makes the next match, the one above, the current one; it opens the bar while it is closed |
| `search.previous` | Edit › Find Previous | ⇧⌘G, and ⇧Return in the bar | Makes the previous match, the one below, the current one |

## Configuration

Its keys go in [`[plugins.search]`](../CONFIGURATION.md#pluginssearch):

| Key | Values | Default | What it does |
|---|---|---|---|
| `case` | `"smart"`, `"insensitive"` | `"smart"` | Whether case counts while the case toggle is off: `smart` counts it for a text of a capital letter, as `Error` finds `Error` and not `error`; `insensitive` never does |
| `highlight-all` | `true`, `false` | `true` | Whether every match that shows is highlighted, or the current one alone |
| `wrap-around` | `true`, `false` | `true` | Whether a step past the last match goes on from the first, and past the first from the last |

```toml
[plugins.search]
case = "insensitive"
highlight-all = false
```

The colors of the matches are the theme's, `search.match` and `search.match.current`.

## Turning it off

```toml
[plugins.search]
enabled = false
```

The bar and its commands go, with their keys: ⌘F then does nothing in Lokyr.

## Permissions

It reads the terminal's text, which the search goes through.
