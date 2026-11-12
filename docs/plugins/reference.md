# Plugin reference

What a plugin's author looks up while writing one: the capabilities and their risks, the slots, the events, the limits, the errors and the manifest. The [guide](authoring.md) tells how to write a plugin step by step; `lokyr.d.luau`, which `sleip new` puts beside the plugin, is the reference of each call of `cx`, its arguments and its answer, of a comment each.

## The manifest

`plugin.toml` holds the plugin's name, `owner/name`, its version, its license, its description, the plugin API that it takes, `api = "^0.1"`, its entry, its activation, the capabilities that it asks for, the commands of others that it runs, `[uses] commands`, and what it contributes: its slots, its config's schema and its catalogs. Every field is in [`plugin.toml`'s reference](../PACKAGE-MANAGER.md#plugintoml); `sleip check` checks the manifest as the registry does.

- **Activation**: a plugin starts as one of its triggers comes, `command:<name>` of its own commands, `event:<name>` or `slot:<name>`, rather than as Lokyr starts; one of no trigger starts with the app.
- **Its namespace** is its name after the owner, as `hello` of `acme/hello`: its commands are `hello.<action>`, its events `hello.<name>`, its config the section `[plugins.hello]`, and no other plugin has it.

## Capabilities

A capability lets a plugin reach what belongs to the person who runs Lokyr. The manifest declares the capabilities, `sleip add` shows them, and Lokyr asks for them at the plugin's first activation, each of its risk, its default button refusing; a refused capability makes the calls that need it fail with `denied`.

| Capability | Allows | Risk |
|---|---|---|
| *(none)* | Registering commands, and running those that the manifest declares; keymap, events, UI, themes, config, storage, logging, timers | — |
| `terminal-read` | Reading screen and scrollback text, and the selection | High: the screen may show secrets |
| `pty-write` | Sending input to the shell | Very high |
| `clipboard-read` | Reading the clipboard | High |
| `clipboard-write` | Writing to the clipboard | Medium |
| `notifications` | Showing system notifications | Low |
| `open` | Opening URLs and files with the system handler | Medium |
| `process-spawn` | Starting processes; refused before 1.0.0, with a notice | Very high |
| `net:<host>` | HTTP requests to the listed hosts, whatever their case; refused before 1.0.0, with a notice | High |
| `fs-read:<path>`, `fs-write:<path>` | Access to the listed paths and to the paths under them, of whole components and no `..` | High |
| `global-hotkey` | System-wide shortcuts, bindings of `global:` keys, and quick windows, which slide in over every app's | Medium |
| `env-read:<name>` | Reading the listed variables of the app's environment and of the panes' programs', with `*` as `AWS_*` | High: they often hold secrets, as `GITHUB_TOKEN` |
| `input-read` | Seeing the keys typed before they reach the program | Very high: a keylogger's power |
| `history-read` | Reading the commands that the shell ran and their output, which shell integration marks, and hearing their starts and ends | High: a password typed by mistake ends up there |
| `process-read` | Seeing the panes' processes: their command lines and working directories, the programs that run and the agents' states | Medium: command lines can hold passwords |
| `workspace-write` | Opening, closing and moving windows, tabs and panes, the keyboard's focus between them, a split's ratio, the zoom, and the app's quit | High: it can end every program, start shells, or move the keys typed to another pane |
| `pane-view` | The panes' scroll and selection, the screen and the scrollback cleared, the font's size, the tabs' titles and colors, the tab bar's place, the inactive panes' fade and the focus that follows the mouse | Medium: a cleared scrollback is lost text |
| `decorate` | Drawing over a pane: decorations, the gutter's marks, a search's matches and a held cursor | Medium: a drawing over the text can show what the terminal does not hold |
| `pointer-read` | The `pointer` event: the cell under the pointer over the panes, and the modifiers held | Low |
| `keep-awake` | Keeping the system, or the system and the display, from idle sleep | High: a display kept awake keeps the screen from locking |
| `packages` | The plugins themselves, through Sleip: the installed ones, the registry's search, installs, updates and removals; Lokyr's own plugins' alone | Very high: a plugin that installs another runs code of its choice |
| `user-commands` | Running any command as if run by hand, without declaring it, as the palette does; reading the whole registry of commands | Very high: `pane.send-text` types into the shell |
| `appearance-write` | Previewing a theme and a monospaced font family, and naming the ones chosen in the config's `[theme]` and `[font] family` | Medium: the config changes, visibly and reversibly |

A request takes every capability that it needs, as a subscription to `pointer` and to `command-started` takes `pointer-read` and `history-read`. The reads of the workspace, the plugin's own views, panels and questions, its declarations, its config and its state take none; neither does a file saved through the save panel, where the place is chosen in the panel and Lokyr writes it.

## Slots

A slot is a place of the window that plugins' views fill; a plugin names the slots of its views in its manifest's `contributes.slots`.

| Slot | Plugins | What it shows |
|---|---|---|
| `tab-bar` | One, chosen in the config | The tab bar, in the titlebar, below it, at the window's bottom or down a column at its side |
| `title-buttons` | Many, three at most, more behind a menu | A button of an icon right after the window's buttons, as a toggle of a state that the plugin keeps |
| `status-bar` | Many, side by side | Status information across the window's bottom, of its views' height |
| `window-strip` | Many | A thin strip across the panes' top, over them, as the programs' progress |
| `pane-header` | One | A header across each pane's top, above its grid |

Floating panels are no slot: a plugin opens and closes them, anchored to a cell, the cursor, the pointer, a node of its views, or a corner of a pane or of the window.

## Events

A plugin subscribes to the events that it hears; those that a capability guards need it.

| Event | When | Capability |
|---|---|---|
| `window-opened`, `window-closed` | A window opens or closes | — |
| `window-focus` | A window takes the keyboard's focus, or loses it | — |
| `tab-created`, `tab-closed`, `tab-activated`, `tab-moved` | A tab opens, closes, shows or moves | — |
| `tab-changed` | A tab's title, color, place or marks change | Its programs', progress' and agents' fields of `process-read` |
| `tab-bar-resized` | The tab bar's column is dragged to another width | — |
| `pane-created`, `pane-closed`, `pane-focused`, `pane-moved` | A pane opens, closes, takes the keyboard or moves | — |
| `pane-changed` | A pane's title or header changes | `process-read` |
| `layout-changed` | A tab's splits change | — |
| `viewport-changed` | A pane's view scrolls or resizes, once a frame | — |
| `command-started`, `command-ended` | A command of the shell starts or ends, of shell integration's marks | `history-read` |
| `pointer` | The pointer moves over a pane's cells | `pointer-read` |
| `keys-changed` | The keymap changes | — |
| `config-changed` | The plugin's section of the config changes | — |
| `lines` | A read of rows, `read_lines`, ends | `terminal-read` |
| `searched` | A search, `search`, finds its matches | `terminal-read` |
| `answered` | A question, `ask_text`, is answered or cancelled | — |
| `saved` | A file given to the save panel, `save_file`, is written, or not | — |
| `panel-closed` | One of the plugin's panels closes | — |
| `timer` | One of the plugin's timers fires | — |
| `package`, `packages-ended` | A job of the packages' service tells a line, or ends | `packages` |
| `<namespace>.<name>` | A plugin's own event, which it emits of its namespace alone | — |

The views' events, as `click`, `submit` or `toggle`, come to the plugin's `on_view_event` of the view that it rendered, of no subscription.

## Limits

Each plugin runs in a VM of its own, so a failing plugin never affects the terminal: it is disabled, and a notice tells it. The details are in [Limits and isolation](../PLUGIN-SYSTEM.md#limits-and-isolation).

- **Deadlines**: 16 ms for `render` and `applies`, 250 ms for a command, an event or a view's event, and a second for the activation, the entry's run included; past it, the call fails and the plugin faults.
- **Memory**: 64 MB a plugin.
- **Faults**: a plugin that faults starts again after a second, then two; at its third fault within a minute, it stays off until it is turned on again.
- **A call's writes** and requests of the app's, which apply as it returns: 10 000 at most together.
- **Reads of rows**: four under way at most; **searches**: 16 a call; **images**: four a call, 16 million pixels each.
- **Timers**: 64 at most, from 10 ms to a day.
- **Declarations**: a plugin's commands, keys and menu items, 4096 at most together.
- **Chains** of commands that run commands: 16 deep and 256 runs at most.
- **Data**: a view nests 64 levels at most, and a value of data 64 too; texts of 16 MiB at most a call; numbers finite.
- **State**: the plugin's own file, 1 MiB at most.

## Errors

A failed call answers `nil` and an error: its `code`, which never changes, its `kind`, as Luau names it, and its `message`. The hundreds tell the category.

| Code | Kind | Category | Meaning |
|---|---|---|---|
| 101 | `not-found` | Usage | No command of the name, or no window, tab or pane of the id |
| 102 | `invalid-args` | Usage | An argument that the command does not take |
| 103 | `invalid-args` | Usage | A required argument missing |
| 104 | `invalid-args` | Usage | An argument of another kind than its parameter's |
| 105 | `invalid` | Usage | A command's or a parameter's name that is not one |
| 106 | `invalid` | Usage | A command registered already |
| 107 | `invalid` | Usage | A schema that repeats a parameter, or whose default is of another kind |
| 108 | `invalid` | Usage | A binding whose keys, command or context is not one |
| 109 | `invalid` | Usage | A config that does not read, or whose values are not its keys' |
| 201 | `failed` | Failure | The command ran, and failed |
| 202 | `failed` | Failure | What an action needs is not there, as a font's family that is not installed |
| 301 | `denied` | Security | A command that the plugin's manifest does not declare |
| 302 | `denied` | Security | A capability that the request needs, which the plugin lacks |
| 401 | `fault` | Fault | A command that broke, as by a panic |
