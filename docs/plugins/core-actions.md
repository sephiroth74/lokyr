# `core-actions`: copy, paste, scrolling and the text's size

<sub>`lokyr/core-actions` · bundled · [source](../../plugins/core-actions)</sub>

The terminal's everyday actions: copy and paste, select all, clear, a command's output copied or selected, the scrolling of the view through the scrollback, and the size of the text. Its keys are [`keymap-macos`](keymap-macos.md)'s.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `pane.copy` | Edit › Copy | ⌘C | Copies the selection |
| `pane.paste` | Edit › Paste | ⌘V | Pastes the clipboard's text, asking you first when [`confirm-paste`](../CONFIGURATION.md#confirm-paste) says so |
| `pane.select-all` | Edit › Select All | ⌘A | Selects the scrollback and the screen |
| `pane.copy-output` | Edit › Copy Command Output | ⌥⇧⌘C | Copies the output of the last command that ended, or, from the pane's context menu, of the command under the pointer, without its prompt and its command line, its wrapped rows joined, 100 000 rows at most; of the commands that the shell marks, as [`shell-integration`](../CONFIGURATION.md#shell-integration) has it |
| `pane.select-output` | Edit › Select Command Output | ⇧⌘A | Selects that output, as a drag would |
| `pane.clear` | Edit › Clear | ⌘K | Clears the scrollback and the screen but the prompt, which goes to the top |
| `font.bigger` | View › Bigger | ⌘=, ⌘+ | Makes the text a point larger, in every window |
| `font.smaller` | View › Smaller | ⌘- | Makes the text a point smaller |
| `font.reset` | View › Default Size | ⌘0 | Gives the text the size of your config again |
| `pane.scroll-line-up`, `pane.scroll-line-down` | View › Scroll Up a Line, Scroll Down a Line | ⇧⌘↑, ⇧⌘↓ | Scrolls the view a line |
| `pane.prompt-previous`, `pane.prompt-next` | View › Jump to Previous Prompt, Jump to Next Prompt | ⌘↑, ⌘↓ | Scrolls the view to the prompt above its top, or below it, of the commands that the shell marks, as [`shell-integration`](../CONFIGURATION.md#shell-integration) has it; past the last, to the screen |
| `pane.scroll-page-up`, `pane.scroll-page-down` | View › Scroll Up a Page, Scroll Down a Page | ⌘Page Up, ⌘Page Down, ⇧Page Up, ⇧Page Down | Scrolls the view a page |
| `pane.scroll-top`, `pane.scroll-bottom` | View › Scroll to the Top, Scroll to the Bottom | ⌘Home, ⌘End | Scrolls the view to the scrollback's first row, or back to the screen |
| `pane.send-text` | | | Sends its `text` to the program, as keys that typed it would: for your own bindings, as it needs its text |

`pane.send-text` gives your keys the line editing of macOS's text fields, as the [configuration guide](../CONFIGURATION.md#bindings) shows:

```toml
[keys]
"alt+left" = { command = "pane.send-text", args = { text = "\u001bb" } }   # a word back
"alt+right" = { command = "pane.send-text", args = { text = "\u001bf" } }  # a word on
```

## Turning it off

```toml
[plugins.core-actions]
enabled = false
```

Copy, paste, select all, clear, a command's output, the scrolling commands and the text's size go, with their keys and their menu items. The selection with the mouse, and the scrolling with the wheel or the trackpad, stay: they are the terminal's own.

## Permissions

It reads the terminal's text, for the selection that it copies; reads and writes the clipboard; and writes to the program, the text that it pastes or sends.
