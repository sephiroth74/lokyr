# Mouse and selection

<sub>macOS · on by default · [`[clipboard] copy-on-select`](CONFIGURATION.md#copy-on-select), [`[cursor] click-to-move`](CONFIGURATION.md#click-to-move), [`[window] smooth-scroll`](CONFIGURATION.md#smooth-scroll)</sub>

Select text with the mouse as in any Mac app, by words, by lines or as a rectangle, and copy it. ⌘-click a link or a path to open it, click in the command line to move the cursor, scroll back through what programs printed, and drop files from the Finder to paste their paths.

## Try it

Run this command:

```sh
ls -l /
```

Double-click the word `usr` in the listing, press ⌘C, then ⌘V: the word pastes at the prompt. Hold ⌥ and drag down across the first column: a rectangle of the permissions is selected.

## What it gives you

- **Selection** by dragging, a word by a double click, a line by a triple click, and a rectangle with ⌥.
- **Copy** with ⌘C, or as you let go of the button, as [`copy-on-select`](CONFIGURATION.md#copy-on-select) says.
- **⌘-click** on a link, a URL or a path opens it, a path with a line at that line in your editor.
- **A click in the command line** moves the shell's cursor there, with [shell integration](shell-integration.md). · Since 1.3
- **A context menu**, from a right click or a ⌃-click, to open a link or a man page, look up or search a word, and copy, paste or select.
- **Scrolling** through the scrollback with the wheel or the trackpad, by pixels with [`smooth-scroll`](CONFIGURATION.md#smooth-scroll) · Since 1.4.
- **Programs that take the mouse**, such as vim or htop, get your clicks, and ⇧ keeps them for the selection.
- **Files dropped** from the Finder paste their paths, ready for a command.

## Select text

Drag across the text to select the cells between where you press and where the pointer is.

- **A double click** selects a word. Spaces, tabs and these characters end a word: `'` `"` `│` `` ` `` `|` `:` `;` `,` `(` `)` `[` `]` `{` `}` `<` `>`. A path such as `src/main.rs` is one word.
- **A triple click** selects a line, joined across the rows that it wraps over.
- **Dragging after a double or a triple click** goes on by words or by lines.
- **⌥ and a drag** selects a rectangle, such as a column of a table.
- **⇧ and a click** extends the last selection to the click.
- **Dragging past the window's top or bottom** scrolls, the faster the farther.
- **A click** takes the selection away.

Output that scrolls the text while you drag keeps the selection's start on its text. For a selection made with the keyboard, see [copy mode](plugins/copy-mode.md).

**Try it**: run `seq 1 100`, then drag from `90` past the window's top: the view scrolls up while the selection grows.

## Copy what you select

Press ⌘C to copy the selection, as in other Mac apps. With [`copy-on-select`](CONFIGURATION.md#copy-on-select) on, letting go of the button copies it, and a double click on a word copies the word.

```toml
[clipboard]
copy-on-select = true   # a selection by the mouse copies its text as it ends
```

The text that Lokyr copies joins the rows of a line that wrapped, drops the spaces at the end of each line, and keeps each row of a rectangle a line of its own.

**Try it**: select two lines of `ls -l /`, press ⌘C, then run `pbpaste`: the two lines print, without trailing spaces.

## Open links and paths with ⌘-click

Hold ⌘ and move the pointer over a link, a URL or a path: Lokyr underlines it and shows the pointing hand. Click to open it: a URL in your browser, a file in its app, a folder in the Finder, and a path with a line, such as `src/main.rs:42`, at that line in your editor. [Links](links.md) tells what Lokyr finds and how to open it from the keyboard.

**Try it**: run `echo https://lokyr.dev/docs/`, then ⌘-click the address: it opens in your browser.

## Click in the command line

<sub>Platforms: macOS · Since 1.3</sub>

With [shell integration](shell-integration.md), a click in the command line moves the shell's cursor there, like in a text field. Select part of the command line and press Delete to remove it, or type to replace it. [`[cursor] click-to-move`](CONFIGURATION.md#click-to-move) turns both off.

**Try it**: type `echo hello world` without pressing Return, click on the `w`, then type `X`: the line reads `echo hello Xworld`.

## Use the context menu

A right click, or a ⌃-click, opens a menu at the pointer:

- **Open Link**, **Copy Link Address**, **New Window at Folder** and **New Tab at Folder**, for the link, the URL or the path under the pointer, as [Links](links.md#use-the-context-menu) describes.
- **Open man Page for “ls”** opens a window of the man page that the selection names, such as `ls` or `ls(1)`, and **Search man Page Index** looks its words up among the man pages.
- **Look Up** shows the selection's definition in macOS's dictionary, and **Search the Web** searches for it, as [`search-engine`](CONFIGURATION.md#search-engine) says.
- **Copy**, **Copy Command Output**, **Paste**, **Select All**, **Select Command Output** and **Clear**.

The menu also lists the bundled plugins' items, and dims an item that would do nothing: **Pane**, a submenu that splits the pane right, left, down or up, moves it to a tab or a window of its own, or closes it, and, while [Blocks](blocks.md) is on, **Block**, a submenu of the actions on the block under the pointer, which takes Copy Command Output and Select Command Output in. A click on a selection keeps it for the menu; a click elsewhere selects the word under the pointer.

**Try it**: run `echo grep`, right-click the word `grep`, and choose Open man Page for “grep”: a window of `grep`'s manual opens.

## Scroll

The wheel and the trackpad scroll through the scrollback, the rows that went up past the screen's top. In a program that takes the whole screen, such as `less` or `man`, they scroll the program's text instead, as its arrow keys do.

**Try it**: run `seq 1 500` and scroll up with the trackpad: the earlier numbers come back. Then run `man ls` and scroll: the manual's page moves. Press `q` to quit it.

### Scroll by pixels

<sub>Platforms: macOS · off by default · Since 1.4</sub>

With [`[window] smooth-scroll`](CONFIGURATION.md#smooth-scroll) on, the text follows your fingers on a trackpad or a Magic Mouse pixel by pixel, rather than a row at a time, and keeps gliding when you lift them. A mouse wheel's step glides over its rows instead of jumping. A click, a selection and a ⌘-click take the text where you see it, even halfway through a row.

- **The keys** still scroll by whole rows, such as Page Up and Page Down and the jumps between prompts.
- **A program that takes the wheel**, such as `less` or vim, still gets whole steps. Hold ⇧ to scroll the scrollback by pixels anyway.
- **An even pace**: the text moves at every refresh of the display while it scrolls, easing toward where your fingers take it, so that it moves smoothly even when the trackpad's events come unevenly. It stops where your fingers took it, a moment after them.
- **The ends**: the text stops at the top of the scrollback and at the screen, with no bounce.
- **Dragging a selection** past the window's top or bottom scrolls by whole rows, keeping the part of a row where you left the text.
- **The scroll bar**: dragging its knob moves the text by pixels, and a click on its track glides a page, or to the spot clicked, as macOS's Click in the scroll bar to setting says.

```toml
[window]
smooth-scroll = true   # the trackpad and the wheel scroll by pixels
```

**Try it**: turn `smooth-scroll` on, run `seq 1 500`, and scroll up slowly with two fingers: the numbers move by pixels. Flick and lift your fingers: the text glides to a stop.

## Programs that take the mouse

A program can ask for the mouse, such as vim with `set mouse=a`, htop or tmux. It then gets your clicks, the right button's too, and the wheel, and the pointer becomes an arrow over its window. Hold ⇧ to select, open the menu or scroll the scrollback anyway.

**Try it**: run `less --mouse /etc/services` and drag across the text: nothing is selected, as `less` takes the mouse. Hold ⇧ and drag: the text is selected. Press `q` to quit.

## Drop files and text

Drag files from the Finder onto a pane: their paths paste at the cursor, each quoted for the shell where it needs it, such as `'/Users/me/My Notes.txt'`, and followed by a space. Dropped text pastes as a paste would, asking first as [`confirm-paste`](CONFIGURATION.md#confirm-paste) says.

**Try it**: type `ls -l ` at the prompt, drag a file from the Finder onto the pane, and press Return: `ls` lists the file.

## Settings

```toml
[clipboard]
copy-on-select = false       # true copies a selection as you let go of the button

[cursor]
click-to-move = true         # a click in the command line moves the shell's cursor

[plugins.splits]
focus-follows-mouse = false  # true gives the keyboard to the pane under the pointer

[window]
smooth-scroll = false        # true scrolls by pixels with the trackpad and the wheel
```

[`[clipboard]`](CONFIGURATION.md#clipboard), [`click-to-move`](CONFIGURATION.md#click-to-move), [`focus-follows-mouse`](CONFIGURATION.md#focus-follows-mouse) and [`smooth-scroll`](CONFIGURATION.md#smooth-scroll) tell more. [`[keys]`](CONFIGURATION.md#bindings) can bind the wheel and ⌘-click to other commands too.

## Limits

- **No middle click paste**: macOS has no primary selection, so a middle click doesn't paste a selection.
- **The word's separators** are fixed, with no setting to change them.
- **A program that takes the mouse** gets the right button too: hold ⇧ for Lokyr's menu.

## Troubleshooting

### Dragging selects nothing

**Why**: the program in front takes the mouse, such as vim, htop or tmux.

**Fix**: hold ⇧ while you drag, or turn the program's mouse off, such as `:set mouse=` in vim.

**Check**: drag with ⇧ held: the text is selected.

### ⌘C copied nothing

**Why**: nothing was selected, or a program that takes the mouse got the drag.

**Fix**: select with ⇧ held, then press ⌘C.

**Check**: run `pbpaste`: it prints what you selected.

### A click in the command line doesn't move the cursor

<sub>Platforms: macOS · Since 1.3</sub>

**Why**: the shell has no integration, the program in front isn't a shell, or `click-to-move` is off.

**Fix**: see [Shell integration](shell-integration.md#a-click-in-the-command-line-doesnt-move-the-cursor).

## See also

- [Links](links.md)
- [Copy mode](plugins/copy-mode.md), to select with the keyboard
- [Shell integration](shell-integration.md)
- [Copy, paste and scrolling](plugins/core-actions.md)
