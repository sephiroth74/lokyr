# `search`: the find bar and Find in All Tabs

<sub>`lokyr/search` · bundled · [source](../../plugins/search)</sub>

A bar at the top right of the pane that searches its screen and its scrollback as you type, highlights every match that shows, and steps through them; and Find in All Tabs, which searches every pane at once.

## The bar

- **⌘F** opens it, of your last search's text, selected, so that typing replaces it, and of its options; ⌘F on an open bar gives its field the keys again.
- **As you type**, it searches the pane, the rows that show first, and counts the matches, as `3 of 12`, numbers past 9999 short, as `12K`. A search of 100 000 lines counts them all in about 25 ms.
- **The matches** that show are highlighted, the current one in a color of its own, over each cell's own colors.
- **The scroll bar** marks every match along it, the current one in its own color, and stays shown while the bar's search shows them, as [`scrollbar`](../CONFIGURATION.md#scrollbar) tells.
- **It goes from the bottom up**, as the newest output is at the bottom: the first match is the one nearest the bottom, the next one the one above it, the previous one the one below it. The view follows the current match, and stays there while the program prints on.
- **The toggles** at the field's end count case, take whole words alone, and read the text as a regular expression, of the syntax of Rust's `regex`; they keep their states for the next search.
- **The filter**, the last toggle, shows only the lines that match, and a blank line where it hides others · not released yet.
  - The lines scroll like the scrollback, a whole line at a time, and the prompt hides while the filter is on.
  - New output's matching lines come in as the program prints them, and the count grows with them. While the filter is on, Lokyr searches each new batch of output, at most 1000 lines at a time. The view follows them while it shows the last line; scrolled up, or at a match that you stepped to, it stays where it is.
  - A full-screen program's own screen, such as less's or vim's, has no filter: its toggle is hidden there, and the filter of a search of the shell's lines comes back once the program ends.
  - Turn it off, or close the bar, and the pane shows every line again, at the current match.
  - It works on the main screen. A full-screen program, such as vim or less, keeps its own screen.
- **The magnifying glass** at the field's start lists your last twenty searches, which Lokyr keeps.
- **The clear button**, a cross in a disc at the field's end while it holds text, empties it, and the search ends · Since 1.4.
- **Escape**, or its close button, closes it: the highlights go, and the current match is selected, ready for ⌘C.

## Find in All Tabs

- **Edit › Find in All Tabs…**, ⇧⌘F, opens a search of every pane of the session's windows, the hidden tabs' too, in a panel near the top of the window · not released yet
- **As you type**, it searches them all, nearest the bottom of each pane first, and lists each match as a row: the line that holds it, and its tab's title at the row's end. The rows go in the order of the windows, their tabs and their panes, then of the lines. It lists 1000 matches at most.
- **The toggles** at the field's end are the bar's: case, whole words and regular expressions. A change in either shows in the other, and the case of [`case`](../CONFIGURATION.md#pluginssearch) counts in both.
- **↑ and ↓** move through the list while you type; **Return**, or a click, brings the match's pane forward, with its tab and its window, selects the match, and opens the bar there with your text, the match its current one, so that ⌘G goes on from it.
- **The bar closes** as Find in All Tabs opens, as Escape would close it, and ⌘F closes Find in All Tabs: one search of a pane at a time.
- **Escape**, or a click outside it, closes it, and the searches end.

## Commands

<!-- commands: lokyr/search -->

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `search.open` | Edit › Find… | ⌘F | Opens the bar at the pane that has the keyboard; of its arguments `text`, `column` and `row`, of that text, the match at that cell its current one, as Find in All Tabs opens it · not released yet |
| `search.next` | Edit › Find Next | ⌘G | Makes the next match, the one above, the current one; it opens the bar while it is closed. Return in the bar does the same |
| `search.previous` | Edit › Find Previous | ⇧⌘G | Makes the previous match, the one below, the current one. ⇧Return in the bar does the same |
| `search.find-all` | Edit › Find in All Tabs… | ⇧⌘F | Opens Find in All Tabs, which searches every pane of the session's windows as you type · not released yet |
| `search.move` | — | ↑, ↓ while `search.find-all.focused` | Moves Find in All Tabs' selection, by its argument `delta`, a whole number of items · not released yet |

<!-- commands end -->

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

The bar, Find in All Tabs and their commands go, with their keys: ⌘F and ⇧⌘F then do nothing in Lokyr.

## Permissions

It reads the terminal's text, which the search goes through, of `terminal-read`, draws the matches, of `decorate`, and selects the current one, of `pane-view`. Find in All Tabs brings the match's pane forward, with its tab and its window, of `workspace-write` and the app's `window.goto` · not released yet.
