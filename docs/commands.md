# Commands

<sub>macOS · Lokyr's commands and its bundled plugins'</sub>

<!-- Made by `cargo xtask docs` of the app's commands and its default keys: change those, not this page. -->

Every action of Lokyr is a command, which a menu, a key, the [command palette](plugins/command-palette.md) or a plugin runs. This page lists every command of Lokyr and of its bundled plugins, with its arguments and the keys that run it by default, so that you can bind keys of your own.

## Bind a command to keys

In your config, `[keys]` binds keys to a command, and to its arguments:

```toml
[keys]
"ctrl+cmd+t" = "tabs.new"                                       # a second key for a new tab
"ctrl+cmd+1" = { command = "tabs.goto", args = { index = 1 } }  # the first tab
"cmd+d" = false                                                  # no split on ⌘D
```

Your keys win over the defaults below. The [keys' section of the configuration guide](CONFIGURATION.md#bindings) tells how to write keys, arguments and contexts.

The keys below show as macOS writes them, then as the config writes them. A key that holds in a context only, such as while the palette is open, says so with `while`.

## App

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| `app.about` | About Lokyr |  |  |
| `app.check-for-updates` | Check for Updates… |  |  |
| `app.install-claude-code-integration` | Install Claude Code Integration… |  |  |
| `app.install-command-line-tools` | Install Command Line Tools… |  |  |
| `app.make-default-terminal` | Make Lokyr the Default Terminal |  |  |
| `app.quit` | Quit Lokyr |  | ⌘Q (`cmd+q`) |
| `app.report-issue` | Report an Issue… |  |  |
| `app.secure-keyboard-entry` | Secure Keyboard Entry |  |  |
| `app.settings` | Settings… |  | ⌘, (`cmd+,`) |
| `app.settings-window` | Open Settings Window |  |  |
| [`plugin-manager.open`](plugins/plugin-manager.md) | Plugins… |  |  |
| [`quick-tips.show`](plugins/quick-tips.md) | Show a Tip |  |  |

## Windows

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`command-palette.agents`](plugins/command-palette.md) | Show Agents… |  | ⌥⇧⌘A (`cmd+alt+shift+a`) |
| [`links.new-window`](plugins/links.md) | New Window at Folder | `column`, whole number; `row`, whole number |  |
| `pane.man-page` | Open man Page |  |  |
| `pane.man-search` | Search man Page Index |  |  |
| `window.close` | Close Window |  | ⇧⌘W (`cmd+shift+w`) |
| `window.close-session` | Close Session |  | · not released yet |
| `window.goto` | Go to Window | `index`, whole number | ⌥⌘1 to ⌥⌘9 (`cmd+alt+1` to `cmd+alt+9`), `index` 1 to 9 |
| `window.new` | New Window |  | ⌘N (`cmd+n`) |
| `window.new-session` | New Session |  | · not released yet |
| `window.next` | Cycle Through Windows |  | ⌘\` (`` cmd+` ``) |
| `window.next-agent` | Go to Next Agent |  | ⌥⌘A (`cmd+alt+a`) |
| `window.previous` | Cycle Back Through Windows |  | ⇧⌘\` (`` cmd+shift+` ``) |
| `window.rename-session` | Rename Session… |  | · not released yet |
| `window.set-title` | Set Title… |  | ⇧⌘I (`cmd+shift+i`) |
| `window.switch-to-last-session` | Switch to Last Session |  | ⌃⌘L (`cmd+ctrl+l`) · not released yet |
| `window.toggle-float-on-top` | Float on Top |  |  |
| [`window.toggle-quick-terminal`](plugins/quick-terminal.md) | Quick Terminal |  |  |

## Tabs

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`links.new-tab`](plugins/links.md) | New Tab at Folder | `column`, whole number; `row`, whole number |  |
| [`tabs.add-to-group`](plugins/tabs.md) | Add Tab to Group | `group`, whole number, `0` by default |  |
| [`tabs.close`](plugins/tabs.md) | Close Tab |  | ⌘W (`cmd+w`) |
| [`tabs.detach`](plugins/tabs.md) | Move Tab to New Window |  |  |
| [`tabs.fold-group`](plugins/tabs.md) | Fold Group |  |  |
| [`tabs.goto`](plugins/tabs.md) | Select Tab | `index`, whole number | ⌘1 to ⌘9 (`cmd+1` to `cmd+9`), `index` 1 to 9 |
| [`tabs.move`](plugins/tabs.md) | Move Tab | `delta`, whole number | ⇧⌘← (`cmd+shift+left`), `delta = -1`; ⇧⌘→ (`cmd+shift+right`), `delta = 1` |
| [`tabs.new`](plugins/tabs.md) | New Tab |  | ⌘T (`cmd+t`) |
| [`tabs.new-group`](plugins/tabs.md) | New Group with Tab |  |  |
| [`tabs.next`](plugins/tabs.md) | Select Next Tab |  | ⇧⌘] (`cmd+shift+]`) |
| [`tabs.prev`](plugins/tabs.md) | Select Previous Tab |  | ⇧⌘[ (`cmd+shift+[`) |
| [`tabs.remove-from-group`](plugins/tabs.md) | Remove Tab from Group |  |  |
| [`tabs.rename`](plugins/tabs.md) | Rename Tab… | `title`, text, `""` by default |  |
| [`tabs.rename-group`](plugins/tabs.md) | Rename Group… | `title`, text, `""` by default |  |
| [`tabs.reopen`](plugins/tabs.md) | Reopen Closed Tab |  | ⇧⌘T (`cmd+shift+t`) |
| [`tabs.set-color`](plugins/tabs.md) | Set Tab Color | `color`, text |  |
| [`tabs.set-group-color`](plugins/tabs.md) | Set Group Color | `color`, text |  |
| [`tabs.switch`](plugins/tabs.md) | Switch Tab |  | ⌃Tab (`ctrl+tab`) |
| [`tabs.switch-back`](plugins/tabs.md) | Switch Tab Back |  | ⌃⇧Tab (`ctrl+shift+tab`) |
| [`tabs.toggle-pin`](plugins/tabs.md) | Pin Tab |  |  |
| [`tabs.ungroup`](plugins/tabs.md) | Ungroup |  |  |

## Panes

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`splits.close`](plugins/splits.md) | Close Pane |  |  |
| [`splits.equalize`](plugins/splits.md) | Equalize Panes |  |  |
| [`splits.focus`](plugins/splits.md) | Select Pane | `direction`, text | ⌥⌘← (`cmd+alt+left`), `direction = "left"`; ⌥⌘→ (`cmd+alt+right`), `direction = "right"`; ⌥⌘↑ (`cmd+alt+up`), `direction = "up"`; ⌥⌘↓ (`cmd+alt+down`), `direction = "down"` |
| [`splits.focus-next`](plugins/splits.md) | Select Next Pane |  | ⌘] (`cmd+]`) |
| [`splits.focus-prev`](plugins/splits.md) | Select Previous Pane |  | ⌘[ (`cmd+[`) |
| [`splits.move-to-tab`](plugins/splits.md) | Move Pane to New Tab |  |  |
| [`splits.move-to-window`](plugins/splits.md) | Move Pane to New Window |  |  |
| [`splits.resize`](plugins/splits.md) | Resize Pane | `direction`, text; `cells`, whole number, `1` by default | ⌃⌘← (`cmd+ctrl+left`), `direction = "left"`; ⌃⌘→ (`cmd+ctrl+right`), `direction = "right"`; ⌃⌘↑ (`cmd+ctrl+up`), `direction = "up"`; ⌃⌘↓ (`cmd+ctrl+down`), `direction = "down"` |
| [`splits.split`](plugins/splits.md) | Split Pane | `direction`, text | ⌘D (`cmd+d`), `direction = "right"`; ⇧⌘D (`cmd+shift+d`), `direction = "down"` |
| [`splits.swap`](plugins/splits.md) | Swap Pane | `direction`, text |  |
| [`splits.zoom-toggle`](plugins/splits.md) | Zoom Pane |  | ⇧⌘Return (`cmd+shift+enter`) |

## Input

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`pane.send-text`](plugins/core-actions.md) | Send Text | `text`, text |  |

## Clipboard

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`links.copy`](plugins/links.md) | Copy Link Address | `column`, whole number; `row`, whole number |  |
| [`pane.copy`](plugins/core-actions.md) | Copy |  | ⌘C (`cmd+c`) |
| [`pane.copy-output`](plugins/core-actions.md) | Copy Command Output | `column`, whole number, `-1` by default; `row`, whole number, `-1` by default | ⌥⇧⌘C (`cmd+alt+shift+c`) |
| [`pane.paste`](plugins/core-actions.md) | Paste |  | ⌘V (`cmd+v`) |

## Edit

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`copy-mode.act`](plugins/copy-mode.md) | Act in Copy Mode | `action`, text | `o`, `action = "swap"`, while `copy-mode.open && pane.focused`; `y`, `action = "copy"`, while `copy-mode.open && pane.focused`; Return (`enter`), `action = "copy"`, while `copy-mode.open && pane.focused`; `Y` (`shift+y`), `action = "copy-line"`, while `copy-mode.open && pane.focused`; Escape (`escape`), `action = "back"`, while `copy-mode.open && pane.focused`; `q`, `action = "exit"`, while `copy-mode.open && pane.focused`; ⌃C (`ctrl+c`), `action = "exit"`, while `copy-mode.open && pane.focused` |
| [`copy-mode.count`](plugins/copy-mode.md) | Count in Copy Mode | `digit`, whole number | `0` to `9`, `digit` 0 to 9, while `copy-mode.open && pane.focused` |
| [`copy-mode.move`](plugins/copy-mode.md) | Move in Copy Mode | `to`, text | `h`, `to = "left"`, while `copy-mode.open && pane.focused`; ← (`left`), `to = "left"`, while `copy-mode.open && pane.focused`; `l`, `to = "right"`, while `copy-mode.open && pane.focused`; → (`right`), `to = "right"`, while `copy-mode.open && pane.focused`; `k`, `to = "up"`, while `copy-mode.open && pane.focused`; ↑ (`up`), `to = "up"`, while `copy-mode.open && pane.focused`; `j`, `to = "down"`, while `copy-mode.open && pane.focused`; ↓ (`down`), `to = "down"`, while `copy-mode.open && pane.focused`; `w`, `to = "word-next"`, while `copy-mode.open && pane.focused`; `b`, `to = "word-previous"`, while `copy-mode.open && pane.focused`; `e`, `to = "word-end"`, while `copy-mode.open && pane.focused`; `W` (`shift+w`), `to = "big-word-next"`, while `copy-mode.open && pane.focused`; `B` (`shift+b`), `to = "big-word-previous"`, while `copy-mode.open && pane.focused`; `E` (`shift+e`), `to = "big-word-end"`, while `copy-mode.open && pane.focused`; Home (`home`), `to = "line-start"`, while `copy-mode.open && pane.focused`; ⇧6 (`shift+6`), `to = "line-first"`, while `copy-mode.open && pane.focused`; ⇧4 (`shift+4`), `to = "line-end"`, while `copy-mode.open && pane.focused`; End (`end`), `to = "line-end"`, while `copy-mode.open && pane.focused`; `g` `g` (`g>g`), `to = "top"`, while `copy-mode.open && pane.focused`; `G` (`shift+g`), `to = "bottom"`, while `copy-mode.open && pane.focused`; `H` (`shift+h`), `to = "view-top"`, while `copy-mode.open && pane.focused`; `M` (`shift+m`), `to = "view-middle"`, while `copy-mode.open && pane.focused`; `L` (`shift+l`), `to = "view-bottom"`, while `copy-mode.open && pane.focused`; ⌃U (`ctrl+u`), `to = "half-page-up"`, while `copy-mode.open && pane.focused`; ⌃D (`ctrl+d`), `to = "half-page-down"`, while `copy-mode.open && pane.focused`; ⌃B (`ctrl+b`), `to = "page-up"`, while `copy-mode.open && pane.focused`; Page Up (`pageup`), `to = "page-up"`, while `copy-mode.open && pane.focused`; ⌃F (`ctrl+f`), `to = "page-down"`, while `copy-mode.open && pane.focused`; Page Down (`pagedown`), `to = "page-down"`, while `copy-mode.open && pane.focused`; ⇧[ (`shift+[`), `to = "paragraph-previous"`, while `copy-mode.open && pane.focused`; ⇧] (`shift+]`), `to = "paragraph-next"`, while `copy-mode.open && pane.focused` |
| [`copy-mode.select`](plugins/copy-mode.md) | Select in Copy Mode | `kind`, text | `v`, `kind = "cells"`, while `copy-mode.open && pane.focused`; `V` (`shift+v`), `kind = "lines"`, while `copy-mode.open && pane.focused`; ⌃V (`ctrl+v`), `kind = "block"`, while `copy-mode.open && pane.focused` |
| [`copy-mode.toggle`](plugins/copy-mode.md) | Copy Mode |  | ⇧⌘C (`cmd+shift+c`) |
| [`pane.clear`](plugins/core-actions.md) | Clear |  | ⌘K (`cmd+k`) |
| `pane.look-up` | Look Up |  |  |
| `pane.search-web` | Search the Web |  |  |
| [`pane.select-all`](plugins/core-actions.md) | Select All |  | ⌘A (`cmd+a`) |
| [`pane.select-output`](plugins/core-actions.md) | Select Command Output | `column`, whole number, `-1` by default; `row`, whole number, `-1` by default | ⇧⌘A (`cmd+shift+a`) |

## Find

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| [`search.next`](plugins/search.md) | Find Next |  | ⌘G (`cmd+g`) |
| [`search.open`](plugins/search.md) | Find… |  | ⌘F (`cmd+f`) |
| [`search.previous`](plugins/search.md) | Find Previous |  | ⇧⌘G (`cmd+shift+g`) |

## View

| Command | Title | Arguments | Default keys |
|---|---|---|---|
| `app.appearance-dark` | Dark Appearance |  |  |
| `app.appearance-light` | Light Appearance |  |  |
| `app.appearance-system` | Automatic Appearance |  |  |
| `app.import-themes` | Import Themes… |  |  |
| `app.new-theme` | New Theme from Current… | `theme`, text, `""` by default; `variant`, text, `""` by default |  |
| `app.open-themes-folder` | Open Themes Folder |  |  |
| `app.performance` | Show Performance |  |  |
| [`command-palette.move`](plugins/command-palette.md) | Move in the Command Palette | `delta`, whole number | ↑ (`up`), `delta = -1`, while `command-palette.focused`; ↓ (`down`), `delta = 1`, while `command-palette.focused` |
| [`command-palette.open`](plugins/command-palette.md) | Command Palette… |  | ⇧⌘P (`cmd+shift+p`) |
| [`command-palette.theme`](plugins/command-palette.md) | Choose Theme… |  |  |
| [`font.bigger`](plugins/core-actions.md) | Bigger |  | ⌘= (`cmd+=`); ⌘+ (`cmd++`) |
| [`font.reset`](plugins/core-actions.md) | Default Size |  | ⌘0 (`cmd+0`) |
| [`font.smaller`](plugins/core-actions.md) | Smaller |  | ⌘- (`cmd+-`) |
| [`links.hints`](plugins/links.md) | Show Hints | `action`, text, `"open"` by default | ⇧⌘E (`cmd+shift+e`) |
| [`links.hints-action`](plugins/links.md) | Next Hint Action | `delta`, whole number | Tab (`tab`), `delta = 1`, while `links.focused`; ⇧Tab (`shift+tab`), `delta = -1`, while `links.focused` |
| [`links.open`](plugins/links.md) | Open Link | `column`, whole number; `row`, whole number | ⌘-click (`cmd+click`) |
| [`pane.prompt-next`](plugins/core-actions.md) | Jump to Next Prompt |  | ⌘↓ (`cmd+down`) |
| [`pane.prompt-previous`](plugins/core-actions.md) | Jump to Previous Prompt |  | ⌘↑ (`cmd+up`) |
| [`pane.scroll-bottom`](plugins/core-actions.md) | Scroll to the Bottom |  | ⌘End (`cmd+end`) |
| [`pane.scroll-line-down`](plugins/core-actions.md) | Scroll Down a Line |  | ⇧⌘↓ (`cmd+shift+down`) |
| [`pane.scroll-line-up`](plugins/core-actions.md) | Scroll Up a Line |  | ⇧⌘↑ (`cmd+shift+up`) |
| [`pane.scroll-page-down`](plugins/core-actions.md) | Scroll Down a Page |  | ⌘Page Down (`cmd+pagedown`); ⇧Page Down (`shift+pagedown`) |
| [`pane.scroll-page-up`](plugins/core-actions.md) | Scroll Up a Page |  | ⌘Page Up (`cmd+pageup`); ⇧Page Up (`shift+pageup`) |
| [`pane.scroll-top`](plugins/core-actions.md) | Scroll to the Top |  | ⌘Home (`cmd+home`) |
| `window.toggle-cursor-guide` | Show Cursor Guide |  | ⌥⌘; (`cmd+alt+;`) |
| `window.toggle-opacity` | Use Transparency |  | ⌘U (`cmd+u`) |

## See also

- [The default shortcuts](plugins/keymap-macos.md)
- [The configuration guide](CONFIGURATION.md#bindings), of the keys' section
