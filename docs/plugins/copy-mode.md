# `copy-mode`: the copy mode

<sub>`lokyr/copy-mode` · bundled · [source](../../plugins/copy-mode)</sub>

A cursor of the keyboard's over the pane's scrollback and screen, which vi's keys move, to select text and copy it without the mouse.

## The mode

- **⇧⌘C**, or Edit › Copy Mode, starts it in the pane that has the keyboard: its cursor shows at the terminal's, or at the view's last row while the view is up in the scrollback, and a badge at the pane's bottom right tells the mode, `Copy`, or `Visual`, `Visual Line` or `Visual Block` while a selection runs, with the count that you typed.
- **The keys** move the cursor and select, as below; the view follows the cursor. A key that the mode does not take goes nowhere: none reaches the program while the mode runs. The app's own shortcuts, as ⌘T or ⌘F, still work, and ⌘C copies the selection, as always.
- **A selection** starts at the cursor, with `v` of the cells in reading order, `V` of whole lines, or ⌃V of a block of columns, and goes from there to the cursor as it moves. The same key takes it away, and another changes its kind.
- **`y`** or Return copies the selection, or the cursor's line without one, and ends the mode; the selection stays, as after ⌘C.
- **It ends** with ⇧⌘C again, `q`, ⌃C, Escape once the selection is gone, or a click on the badge; and as another pane of the tab takes the keyboard, as a program such as vim shows its own screen, and as a resize lays the rows out anew. Another tab in front leaves it as it is, for when you come back.
- **It does not start** while a program such as vim or less shows its own screen, which has no scrollback.
- **In a pane whose program ended**, which a key closes, the mode's keys move and copy, and close nothing; out of the mode, a key closes it again.

## Keys

The keys are those of the US layout: `$` is `shift+4`, as it types there. Each key can be rebound in [`[keys]`](../CONFIGURATION.md#keys), in the context `copy-mode.open && pane.focused`.

| Keys | What they do |
|---|---|
| `h` `j` `k` `l`, the arrows | A cell left or right, a row down or up |
| `w` `b` `e` | The next word's start, the start of the word before, the word's end: a word is of letters, digits and `_`, or of punctuation |
| `W` `B` `E` | The same of big words, of anything but blanks |
| `0`, Home | The line's start: of the whole line that the program wrote, across the rows that it wraps over |
| `^` | The line's first character but a blank |
| `$`, End | The line's last character but a blank |
| `gg` | The scrollback's first row |
| `G` | The last row that shows something, or the terminal's cursor's, which is later |
| `H` `M` `L` | The view's top row, its middle one, its bottom one |
| ⌃U ⌃D | Half a page up or down, the view with the cursor |
| ⌃B ⌃F, Page Up, Page Down | A page up or down |
| `{` `}` | The blank line before the paragraph, or after it |
| `1` to `9`, then `0` to `9` | A count of the next move, as `12j` moves 12 rows down |
| `v` `V` ⌃V | A selection of cells, of lines, of a block |
| `o` | Swaps the selection's ends |
| `y`, Return | Copies the selection, or the cursor's line, and ends |
| `Y` | Copies the cursor's line, and ends |
| Escape | Takes the count away, or else the selection, or else ends |
| `q`, ⌃C | Ends, the selection taken away |

A line's copy leaves the blanks at its end out.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `copy-mode.toggle` | Edit › Copy Mode | ⇧⌘C | Starts the mode in the pane that has the keyboard, or ends it |
| `copy-mode.move` | | the moves above | Moves the cursor `to` a place: `left`, `right`, `up`, `down`, `word-next`, `word-previous`, `word-end`, `big-word-next`, `big-word-previous`, `big-word-end`, `line-start`, `line-first`, `line-end`, `top`, `bottom`, `view-top`, `view-middle`, `view-bottom`, `half-page-up`, `half-page-down`, `page-up`, `page-down`, `paragraph-previous` or `paragraph-next` |
| `copy-mode.select` | | `v`, `V`, ⌃V | Starts a selection of a `kind`, `cells`, `lines` or `block`, changes its kind, or takes it away |
| `copy-mode.count` | | `0` to `9` | Adds a `digit` to the count of the next move; `0` of no count goes to the line's start |
| `copy-mode.act` | | `o`, `y`, Return, `Y`, Escape, `q`, ⌃C | Its `action`: `swap`, `copy`, `copy-line`, `back` or `exit` |

To leave the mode with `x` as well:

```toml
[keys]
"x" = { command = "copy-mode.act", args = { action = "exit" }, when = "copy-mode.open && pane.focused" }
```

## Turning it off

```toml
[plugins.copy-mode]
enabled = false
```

The mode and its commands go, with their keys: ⇧⌘C then does nothing in Lokyr.
