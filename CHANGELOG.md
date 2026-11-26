# Changelog

All notable changes to Lokyr are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
from 1.0.0; before it, a minor version may change what a previous one did.

## [Unreleased]

### Added

- AppleScript: Lokyr's dictionary has its windows, their tabs and their terminals, each with its id, name and folder, and commands that create windows, tabs and splits in a folder with a command to run and environment variables, run a command line at the shell's prompt, send text, press a key as you would, such as `"ctrl+c"` or `"cmd+a"`, focus, close, and perform any of Lokyr's commands by its name and arguments. A script reads a terminal's text once you allow the app that runs it, and Lokyr remembers your answer.
- Shortcuts: actions that open a window or a tab in a folder with a command to run and environment variables, split a terminal, run a command line in it, send it text or press a key in it as you would, focus or close it, find terminals by their title or folder and get their details, show or hide the quick terminal, run any of Lokyr's commands by its name, or an entry of the palette's list, such as Split Right, switch to a session by its name, opening it when there is none, open a saved layout, and get a terminal's text or its selection, which Lokyr asks you about first. Spotlight finds New Window and New Tab too. Scripts and shortcuts stop working with `enabled = false` in `[plugins.automation]`.
- `window.show-quick-terminal` and `window.hide-quick-terminal` show and hide the quick terminal, for a key or a shortcut of your own, where Quick Terminal toggles it.

### Changed

- The first window shows sooner at the start, some 7 to 11 ms, as the menu bar, the global keys and the updates' checks wait until it opens; when Lokyr starts to open a script, a folder or a link, that window no longer waits for them.
- A hidden session's windows give back part of the memory of their pictures, some 10 MB a window on a Retina display and more of a larger window, and draw themselves anew just before they show again.

### Fixed

- `tabs.set-color` and `tabs.set-group-color` take a color of three digits, such as `#d14`, as the tab bar's page shows, which they refused.
- Kept in the Dock, Lokyr shows the icon's variant that `[app] icon` names after it quits too, where the Dock showed the bundle's own icon.
- Lokyr's icon shows as itself in the system's lists of small icons, such as System Settings' privacy panes, where it showed as colored noise on a display of no scaling.
- The command palette lists the menu bar's items that run a command with an argument, as Split Right, Split Down and Select Pane Above, with their keys, which only the menus had.

### Security

- A plugin's call opens 16 panes at most, and a plugin 64 a second, by new tabs, splits, popups, windows and sessions together, so that one cannot start thousands of shells at once and freeze Lokyr.

## [1.5.0] - 2026-11-25

### Added

- The plugin API draws a line above a list's row, `divider` of a list's item, as a sidebar divides its groups.
- Sessions, groups of windows that you switch between: Window › Sessions › New Session opens one with a window of a new shell and hides your other windows, whose programs keep running; Switch to Last Session, ⌃⌘L, brings back the session you used before, its windows where you left them; Rename Session… names it, the name showing in its windows' titlebars before their keys; and Close Session closes its windows, asking first while a program runs in one. Each session numbers its own windows from ⌥⌘1, and the quick terminal, the settings window and man pages' windows show in every session. With your windows, sessions come back as Lokyr starts, the one you used last showing; the windows of the others open the first time you switch to each, their shells starting then.
- Window › Sessions › Switch Session…, ⌃⌘S, lists your sessions, your layouts, then folders: those that `[sessions] folders` names, such as `["~/src/*"]` for every project in `~/src`, and the last 50 where your shells worked. As you type, the best match comes first. A session switches; a folder opens a session there, named after it, or switches to the session opened there already. `window.switch-session` switches to a session by its `name`, for a key of your own.
- Layouts: Window › Sessions › Save Session as Layout… writes the session's windows, tabs, splits and folders to `~/.config/lokyr/layouts/<name>.toml`, and the switcher lists the layouts, each opening as a new session with new shells. Add a `command` to a pane in the file, and its shell runs it as it starts. A layout that doesn't read tells its file and line in a notice. `window.open-layout` opens a layout by its `name`, for a key of your own.
- The plugin API reads and switches sessions, groups of windows of which the active one's show: `cx.sessions` reads them, in the order of their use, `cx.new_session` makes one of a name and switches to it, its first window in a folder, and `cx.switch_session` switches to another, of `workspace-write`; the event `session-changed` tells each change. A session tells the folder that it was opened in, `folder`, and `cx.session_folders`, both of `process-read`, reads the folders and the layouts that the sessions' switcher lists.
- Shell › Broadcast Input sends what you type into the pane with the keyboard to every pane of its tab, ⌥⌘I, or of the window, ⇧⌥⌘I, too: your keys, an input method's text and your pastes and drops, each as its own program asks, a paste of several lines asking once for all. A red ring marks each pane that takes it, of the theme's color `broadcast.ring`, and a megaphone marks its tab; a pane's context menu leaves it out with Leave Broadcast and takes it back with Join Broadcast. A popup, and the panes that a zoom hides, take none of it, and what you type into a popup goes to it alone. Your keys bound to `pane.send-text` reach every pane too.
- Floating panes: Shell › New Floating Pane, ⇧⌥⌘F, opens a pane over the tab's panes, in a box with its title, which you move by its title, resize by its edges and corners, and close by the × at its title's right; Show or Hide Floating Panes, ⌥⌘F, hides a tab's floating panes together and shows them again. A pane's context menu floats a pane of the splits, Float Pane, and moves a floating one back, Move Pane into Splits. ⌘W closes the pane you're in, not the tab, while the tab has floating panes, hidden ones too. They come back with their tab when your windows open again. The plugin API reads a tab's floating panes, `cx.floating_panes`.
- Popups: bind a key to `pane.popup` with a `command`, such as `lazygit`, which your login shell runs in a floating pane in the middle of the tab, in the folder of the pane you were in or the one that `folder` names, of the `width` and `height` you give. It closes when the command exits with 0 or you interrupt it; when the command fails, it stays open with a line of how it ended until you press a key. It shows on its own, the tab's floating panes that you hid staying hidden, and stays over its tab, closing with it.
- The plugin API opens popups, `cx.open_popup`, of a command, a folder and shares of the tab, which take `workspace-write` and `pty-write`, and tells a plugin as its popup closes, `popup-closed`, of its command's status.
- The plugin API broadcasts a window's typed input: `cx.set_broadcast` sends the keys, the input method's text and the pastes typed into the pane with the keyboard to every pane of its tab or of the window too, each as its own program asks, `cx.set_pane_broadcast` leaves a pane out, both of `workspace-write` and `pty-write`, and `cx.broadcast` reads it, its changes told by the event `broadcast-changed`. `cx.paste_broadcast` pastes as you do, into the panes that the window broadcasts to, asking once for all; `cx.send_text_broadcast` sends text to them as keys would.
- With `[app] restore-scrollback = true`, the panes that open again with your windows show what they showed as you quit: their scrollback, its colors and links, and the commands that your shell marked, whose gutter marks, blocks and jumps work as before, up to the prompt where you typed, then a line that tells when it was kept, then a new shell. Lokyr keeps all that a pane held within its `[scrollback] limit`, in a file of each pane that only your account can read, removed once the pane shows it again. It's off by default, as what the panes showed may hold what you wouldn't keep.

### Changed

- When a pane's program ends with an error, the line that the pane shows says that a key closes the pane, rather than the window: in a tab of several panes, the key closes the pane alone.
- In the settings window, a key's More info and Docs links line up with its description above them, as a link shows no box.
- The settings window's sidebar lists Lokyr's own sections first, then, after a line, those of the plugins that you installed.
- The Shell menu gathers the splits into a Split submenu and the panes' other commands, such as Select Next Pane and Zoom Pane, into a Pane submenu, and sets the tab groups' commands, such as New Group with Tab, apart from the tabs' with a line.
- In the settings window, a key at its default shows no Reset link, rather than a dimmed one, and the Theme Editor's Reset shows under a color's field once you change the color.

### Fixed

- Quitting with more than 100 windows or 256 panes no longer loses every window at the next start: Lokyr keeps those of the session that shows first, up to that many, and a notice tells how many it left out.

### Security

- A plugin that runs Lokyr's own commands needs the capabilities of what they do: `workspace-write` to open, close, move or select windows, tabs and panes or to quit, `pane-view` for a tab's or a window's title, `open` for what opens in another app, and what the find bar and the copy mode read and draw; the commands of the system and the integrations, such as Make Lokyr the Default Terminal, run of `user-commands` alone. A plugin could run them of none before.

## [1.4.0] - 2026-11-22

### Added

- Shortcuts drawn as key caps, as ⇧ ⌘ N, in the Keys page and the command palette; callouts, tinted boxes of a level's icon, of a config file that does not read, a value refused and a plugin's failure; tags of the Plugins page's Active, Disabled and Replaced; and an empty state of a search that finds nothing, in the Settings and Keys pages and the plugins' registry. Plugins draw key caps of `kind = "keys"` or a button's `keycaps`, and themes color them of `keycap.bg`, `keycap.fg` and `callout.info`, `.success`, `.warning` and `.error`'s `bg` and `fg`.
- Group boxes, as macOS 27's forms: the Settings page's keys and the Keys page's bindings, and the Plugins page's lists, show in rounded groups of a faint fill and a hairline between each two rows, as System Settings groups its rows. Plugins draw one of `kind = "group"`, and themes color it of `group.bg` and `group.separator`.
- Search fields, as macOS 27's: the Settings page's search, the Keys page's and the Theme Editor's filters, the Plugins page's registry field and the command palette are capsules of a magnifying glass at their left and, while they hold text, a button at their right that empties them; Escape empties them first, but in the palette, which it closes. The find bar takes their glyphs, within a box of rounded corners: the magnifying glass that lists your recent searches, and the same button. Plugins draw one of a field's `search = true`, and the icons `magnifying-glass` and `clear-circle`.
- Pop-up buttons, as macOS 27's: a plugin draws one of `kind = "pop-up"`, of its `choices`, the one `chosen`, and a `placeholder` of none, its text beside a pair of chevrons; a click, or Space, Return or an arrow key while it has the focus, opens macOS's own menu over it, the choice in force ticked over its text, which tells a `highlight` of the choice under the pointer or the arrow keys and a `select` of the choice made. VoiceOver reads it as a pop-up button of its value.
- Checkboxes and groups of radio buttons, as macOS 27's: a plugin draws a checkbox of `kind = "checkbox"`, which tells a `toggle` as a switch does, its check a bold line of round ends, and `mixed = true` of a dash, neither on nor off, as a checkbox over others of which some are on; and a group of one choice of `kind = "radios"`, of its options down a column or across a row, which a click or the arrow keys choose, telling a `select`; VoiceOver reads them as a check box, mixed too, and a radio group. Themes color them of `check.off.bg`, `check.on.bg`, `check.mark`, and `check.off.pressed.bg` and `check.on.pressed.bg` of a checkbox pressed.
- Default and destructive buttons: a dialog's main action, such as the Keys page's Add, is filled with the accent and Return presses it wherever the focus is, but in a field or a list; an action that removes something, such as the Plugins page's Remove, has a red label. Plugins draw them of a button's `variant`, and themes color them of `button.default.bg`, `button.default.pressed.bg`, `button.default.fg` and `button.destructive.fg`.
- A plugin can stand in for another that shows in the same place, of its manifest's `replaces`, such as a tab bar of its own in place of Lokyr's: Lokyr turns the other off while it runs, and on again as it is turned off or removed, writing nothing to your config. A plugin that you install asks for it with its other capabilities, the Plugins page shows the other as Replaced by it, `sleip list` tells it, and `sleip add` refuses plugins that would replace each other. Nothing replaces the Plugins page itself.
- Blocks, off by default, which `[plugins.blocks] enabled = true` or the Plugins page turns on: each command that your shell marks, with its output, is a block. While you scroll through a long output, the block's prompt rows, its prompt and its command line as your shell drew them, stay pinned at the pane's top, and a click on them goes to the command's prompt. `[plugins.blocks] header = "pill"` shows the command line in a pill instead, as the command header showed it, with an arrow beside it to the block's end, the output's last row at the pane's bottom, or the screen for a command that still runs. Blocks takes the place of the command header, which comes back when you turn Blocks off, and has the same keys, in `[plugins.blocks]`; keys left in `[plugins.command-header]` apply only while Blocks is off, so move them to `[plugins.blocks]` to keep the header's look; the settings window notes it above a replaced plugin's keys, naming the plugin that replaces it.
- Right-click a block and open its Block submenu to copy its command line, Copy Command, its output, Copy Command Output, or both, Copy Command and Output, or to select its output, Select Command Output, or the whole block, Select Block, from its prompt to its output's end. They are in the Edit menu and the command palette too, where they take the last command. A right-click on the block's prompt rows pinned at the pane's top, or on its pill, opens a menu of the block's actions alone.
- Right-click a block and choose Block › Block Details to see its command line, its exit status, when it started, how long it ran and the folder where it ran, each copied by a click. Edit › Block Details shows the last command that started, which tells Running until it ends.
- Select a block with the keyboard: ⌘↑ selects the last block, tinted, ↑ and ↓ move to the block before or after, the view following, ⌘C copies its command line and output unless text is selected, and Escape, a key typed or a click ends the selection. When you have scrolled up, ⌘↑ and ⌘↓ jump to the prompt above or below the view's top and select its block. The Edit menu's block commands act on the selected block. Themes color the outline of `blocks.selected`.
- A thin line divides each command's block from the one before, and the gutter's bar of each command runs along its whole block while Blocks runs; `[plugins.blocks] dividers` turns the line off, and `failed-tint` tints a failed block's rows too. Themes color them of `blocks.divider` and `blocks.failed`, in a `[blocks]` section that the plugin's own tokens share, and of `blocks.selected` for the block that the keyboard selects; the Theme Editor shows them in its terminal's preview.
- Click a block to select it: its rows are tinted across the pane, as when ⌘↑ selects it, and the block's actions, such as ⌘C, act on it; a click at the prompt where you type, a drag that selects text, or a key ends the selection; a click in a command that still runs, such as a remote shell, selects none. Themes color it with `blocks.selected.bg`.
- Right-click a block and choose Run Command Again to type its command line at the shell's prompt and run it, while the command line is empty; otherwise a notice tells why nothing ran, such as a command that still runs or text that you typed.
- `[plugins.blocks] spacing = true` puts a blank row before each prompt that follows a command, as room between the blocks, the line between them through its middle and a selected or failed block's tint reaching it; Lokyr puts the row there itself, in any shell that marks its prompts, none before the first prompt, after an empty command line or at the top of a cleared pane.
- Smooth scrolling, off by default, which `[window] smooth-scroll = true` turns on: the text follows your fingers on a trackpad or a Magic Mouse pixel by pixel, at every refresh of the display and at an even pace however the trackpad's events fall, and keeps gliding as you lift them, a mouse wheel's step glides over its rows, and the scroll bar's knob moves the text by pixels as you drag it, a click on its track gliding there; a click, a selection and a ⌘-click take the text where you see it, halfway through a row too. The keys still scroll by whole rows, a program that takes the wheel, such as `less` or vim, still gets whole steps, ⇧ scrolling the scrollback by pixels anyway, and the text stops at the scrollback's top and at the screen, with no bounce.

- The plugin API reads a command by its id, `cx.shell_command`, and the command whose block holds a row, `cx.command_at`, of `history-read`.
- The plugin API tells a command's start and end times, `started` and `ended`, and the folder where it started, `folder`, of its machine, `host`, where it is another one, the folder to a plugin of `process-read` alone; and the system's time and the local time zone's offset from UTC, `cx.now`, of no capability.
- A bundled plugin's section of the Settings page starts with an Enabled switch, which turns the plugin on or off as the Plugins page does; while the plugin is off, such as Blocks by default, its settings are dimmed and their controls disabled, and they apply once you turn it on.
- The plugin API disables a text field, `disabled` of a field: dimmed, it takes neither a click, the focus nor a value.
- The status bar lays its plugins' views out on three sides, `start`, `center` and `end`, each at its own width rather than an equal share of the bar, over a band across the whole window that themes color of `status-bar.bg`, the tab bar's color by default: a plugin's manifest names its side in `[contributes.status-bar] side`, and the plugin API moves it, `cx.set_status_bar_side`, of no capability. Plugins draw the icon `git-branch`.
- The plugin API writes a temporary text file of the plugin's own, `cx.temp_file`, of no capability, readable by your account alone, in a folder that Lokyr empties as it quits and as it starts, marked as downloaded, so that macOS asks before it opens as a program, its path the answer, as an editor opens it; and reads the branch of a folder's git repository, `cx.git_branch`, of `process-read`, without running git.
- The plugin API reads the cells of a range of rows by their numbers, `cx.row_cells`, of `terminal-read`, as an image draws them, such as a command's block's rows.
- A Luau plugin's tests give the pane the commands that its shell marked, `host.set_commands`, tell what the plugin's calls read, `host.reads`, and cut its reads of rows short, as a resize does, `host.cut_reads`.
- The plugin API tells a plugin of `process-read` as a command that the shell marked starts and ends, `command-started` and `command-ended`, of no exit status, which `history-read` tells, as a folder's branch may change then.
- The plugin API tells of a right-click on a block's pinned prompt rows, the `prompt-rows-menu` event, of `pointer-read`, and a context menu's item of a command of a cell, of no arguments of its own, runs of the cell clicked.
- The plugin API draws the commands' blocks of every pane, `cx.set_block_looks`, of `pane-view`: the gutter's bars along them, their dividers, a failed block's tint, the block's prompt rows pinned at the view's top, `prompt_rows`, and a blank row before each prompt that follows a command, `spacing`.
- The plugin API selects a block of a pane, `cx.select_block`, of `pane-view`, which the host tints and ends as a key is typed or the pane clicked, and reads it, `cx.selected_block`, of `history-read`; a binding's `when` takes `<namespace>.selected` while it lasts.
- The plugin API reads the command line that the shell edits at its prompt, `cx.edited_line`, of `terminal-read`, and runs a command line there, `cx.run_line`, of `pty-write`, while that line is empty.
- The plugin API tells a plain click on a pane's cell, the `click` event, of `pointer-read`: one click of no modifier that selects no text, as Blocks selects the block under it.
- The plugin API tells the folds that a pane's view shows, `folds` of `cx.viewport`, each in place of a command's output or of its whole block, from its first row to its last, so that the view shows the rows past its own count too.
- The plugin API 0.1.6 draws a switch, a view node of `kind = "switch"`, as macOS's, of two sizes, which tells its plugin a `toggle` as a toggle does; a theme colors it of `switch.off.bg`, `switch.on.bg`, `switch.knob.bg` and `switch.knob.shadow`, which the Theme Editor lists under Toggles and shows in its panel's preview.

### Changed

- The pane's context menu is shorter: Split Right, Left, Down and Up, Move Pane to New Tab, Move Pane to New Window and Close Pane are in its Pane submenu.
- Lokyr's controls take macOS 27's sizes: buttons, toggles, fields and segmented controls are 24 points high, as macOS's regular controls, with rounder corners; a disabled control dims whole, its box too; the highlight under the pointer is fainter. The plugin API gives each control a size, large, regular, small or mini, 28, 24, 20 or 16 points high, of smaller text at small and mini, and a switch's sizes are macOS's.
- The Settings page shows each on-or-off key as a switch, as macOS's, in place of an On or Off button: its knob slides across as you click it, at once where Reduce motion is on, and VoiceOver reads it as a switch of the key's name.
- The Settings page's menus of values, such as a key's few values, a font's family and style, and a plugin's color, are pop-up buttons, as macOS's: the menu opens over the button, the value in force ticked over its text, and a font's family or style still shows in the panes as its item rests highlighted.

### Fixed

- A Luau plugin that hands back an empty list that Lokyr gave it, such as a setting's value of `[]`, hands back a list, no longer an empty map.
- Show Hints labels the links and paths of a long line of no spaces, such as a minified JSON wrapped over more rows than the pane shows, where it labelled none.
- A text field that has the keyboard, such as the find bar's, the command palette's or the settings window's, leaves Option-Command and an arrow, as the move between panes, and Shift-Command-C, Copy Mode, to Lokyr, where it took them as its own.
- In zsh, with a plugin that switches the history list as you type, such as Oh My Zsh's per-directory-history together with `share_history`, the shell integration reports each command's line again, so that the command's header leaves the right prompt out.
- A plugin that starts while a pane's view stays still, such as the command header when you turn Blocks off, sees the view at once, rather than at the next scroll or output.
- A program that prints the shell's mark of a command's start while a command runs, such as `cat` of a file that holds one, no longer replaces the command line that the command's header shows, nor moves the command's marks.

## [1.3.0] - 2026-11-18

### Added

- The settings link the documentation's page of a key's topic beside its More info, such as shell integration's, which tells how to add it by hand to macOS's own bash or to a shell that you start from another one.
- A program that tells its status of OSC 7501, the Program Status Protocol, as Claude Code and Codex do, shows it exactly on its tab, whether `[agents] programs` lists it or not: at work, of its progress in the tab's ring; waiting for you, of what it asks for, a permission, an answer or a login, and of its message in the tab's tooltip; done, or failed of a red mark, until you look at its tab or type into its pane; idle, of no mark. Its output, its quiet and its bell no longer guess its state while it tells it, until it leaves the foreground; Lokyr answers the protocol's query, `OSC 7501 ; ?`, as such programs ask before they tell anything.
- The plugin API 0.1.5 reads what an agent that waits asks for, the `kind` of each agent of `cx.agents`, of a program that tells it.
- A program's status of its session, as OSC 21337 tells it and Codex sends it: its tab shows a dot of its color, unless an agent's mark, and its text and detail in its tooltip; it goes as the program exits.
- Desktop notifications of OSC 99, as programs send them: their title and body, in parts or in base64, shown once their last part comes, as those of OSC 9 and 777; Lokyr answers the protocol's query, as programs ask before they send one. Their icons and buttons are not shown.
- The plugin API reads the status that a program tells of its session, `status` of a tab, of its text, its colors and its detail.
- A click in the command line moves the shell's cursor there, as in a text field, in zsh, bash and fish of Lokyr's shell integration, and in any shell that announces it, as Nushell: Lokyr hands the click to the shell, or sends the arrow keys to the clicked character; a drag still selects, and a click on the prompt, or past the command's end, moves it no further than the end.
- Backspace, Delete or a typed character over a selection of the command line takes its place, in those shells, as in a text field; `[cursor] click-to-move` turns both off.
- ⌃Tab shows the tab you used before. Press Tab again while you hold Control to go further back; the tab that shows when you release Control becomes the most recent. ⌃⇧Tab goes the other way. Set `[plugins.tabs] control-tab = "order"` to step through the bar's order instead.
- The plugin API reads a window's tabs in the order they were used, `cx.recent_tabs`.
- Pinned tabs: right-click a tab and choose Pin Tab, or run `tabs.toggle-pin`. A pinned tab stays at the start of the bar, narrow, with a pin and its most urgent mark, its full title in its tooltip with what an agent asked for or the status that a program tells. ⌘W doesn't close it, nor does Close Other Tabs, but ⌘W still closes one of its panes when it has several. Restored windows keep their pinned tabs.
- The plugin API pins and unpins tabs, `cx.set_tab_pinned`, and tells whether a tab is pinned, `pinned` of a tab.
- Hints: press ⇧⌘E, or choose View › Show Hints, to label every URL, path, path with its line, git hash and IP address on the screen with a letter or two, then type a label to open it as a ⌘-click would, a path at its line in your editor. A capital copies it; Tab switches what a pick does between open, copy, type at the prompt and select. Your own patterns, as a ticket's number to its page, join them in `[plugins.links] hints`, each opening an `http`, `https`, `mailto` or `file` URL, or a path, which the settings window's Links section adds, changes, renames and removes too, under Hints, each a table of its own in your config file, and `[plugins.links] alphabet` sets the labels' letters; themes color the labels with `hints.label.bg` and `hints.label.fg`.
- Tab groups: right-click a tab and choose New Group with Tab, which asks for its name, to gather tabs under a name and a color. The group shows as a pill before its tabs, which take a tint of its color; click the pill to fold the group behind it, and right-click it to rename the group, change its color or ungroup it. Drag a tab right after a group's pill, between two of its tabs or after its last to put it in, and elsewhere to take it out; a tab's context menu also adds it to a group or takes it out. Restored windows keep their groups. The Shell menu and the command palette have them too, for the tab you're on: `tabs.new-group`, `tabs.add-to-group`, `tabs.remove-from-group`, `tabs.rename-group`, `tabs.set-group-color`, `tabs.fold-group` and `tabs.ungroup`, which you can bind to keys.
- The plugin API groups a window's tabs: `cx.new_group` makes a group of tabs, of a name and a color, its tabs next to one another; `cx.add_to_group`, `cx.remove_from_group`, `cx.rename_group`, `cx.set_group_color`, `cx.fold_group` and `cx.ungroup` change it; `cx.groups` reads a window's groups, `group` of a tab tells its group, and the event `group-changed` tells each change. A tab moved between two tabs of a group joins it, one moved out of it leaves it, and restored windows keep their groups.
- A plugin's decoration takes a label of one to three letters or digits, `label` of its look, which Lokyr draws over the decoration's first cells in the accent's colors, as hints' letters.
- Show or hide the quick terminal from any app with a double tap of a modifier: set `[plugins.quick-terminal] double-tap` to `"ctrl"`, `"alt"`, `"cmd"` or `"shift"`, then press that key twice quickly, alone. A key or a click while it is held, or between the taps, doesn't count, so Control-C twice is no double tap. macOS asks you to allow Lokyr in System Settings › Privacy & Security › Input Monitoring the first time; Lokyr reads the modifiers alone, never what you type.
- The plugin API hears the double tap of a modifier from any app, `cx.watch_double_tap`, of `global-hotkey`, and its event `double-tapped`, to the plugin alone.
- Press ⌘U, or choose View › Use Transparency, to show a translucent window opaque while you read, and again to see through it once more. Each window keeps its own choice, and your config's opacity and blur stay as they are; changing them in the config shows them in every window.
- Reopen a tab you just closed: Shell › Reopen Closed Tab, ⇧⌘T, brings back the tab you closed last, at its place, as you left it. For 10 seconds after you close a tab, its programs keep running and their output is read; then it closes for good. Set the time with `[plugins.tabs] reopen-seconds`, or `0` to close tabs at once. A tab whose program ends, the last tab of a window and a window you close don't wait, nor a tab you close while a program other than a shell runs there, as closing it ends the program. At most 16 closed tabs wait; closing another closes the oldest for good.
- The plugin API keeps a closed tab to give back, `keep` of `cx.close_tab`, 16 at most, and gives it back, `cx.reopen_tab`.
- The text fields have a context menu of Cut, Copy, Paste and Select All, each dimmed where it does nothing, and take macOS's Control-T, which swaps the characters around the caret, Control-P and Control-N, and Control-Y, which types back what Control-K took.

### Changed

- The settings window's sidebar is wider, and its search field and its sections line up at the same distance from its edges, on both sides.
- The scroll indicator of a list or a scrolling area, such as the settings window's sidebar or the command palette, shows while you scroll it and fades a moment after, like macOS's scroll bars, or stays when System Settings shows scroll bars always.
- ⌃Tab and ⌃⇧Tab follow the order you used the tabs in, not the bar's order. ⇧⌘] and ⇧⌘[ still follow the bar.
- An agent's notification tells the message that it told, as "claude: Claude needs your permission to use Bash", or, of none, what it asks for while it waits, as "codex asks for your permission".

### Fixed

- The command's header across a pane's top stays within the pane: a long command is cut at its end of an ellipsis, past `[plugins.command-header] max-length`'s characters, 80 by default, and wherever the pane is narrower, its tooltip and what VoiceOver reads keeping it whole.
- The text fields, such as the settings window's, its search and the find bar's, take their editing keys before Lokyr's shortcuts: Shift-Command-← selects to the field's start rather than move the tab, and Command-↑ moves the caret rather than jump to the previous prompt.
- A command's mark in the gutter shows on its prompt's first line of text, not on the empty line that some prompt themes print before the prompt.
- A tab's context menu no longer runs the item that the pointer moves over, such as Rename Tab…: it waits for you to click an item.
- The cursor no longer shows, as it blinks, the character of another cell, as a few moves at once, of a click in the command line or of quick output, find its drawing busy: it stays where it was drawn until its next frame draws it in its new cell.
- A plugin whose command fails while you answer its earlier question can ask you again, rather than being refused every question until Lokyr starts again.

## [1.2.0] - 2026-11-16

### Added

- The Themes page's Editor, in the settings window, shows every color of the theme in force, or of the one that a card's Edit or its menu of the themes chooses, by groups, of its value and where it comes from, its file's line, a theme of no file or its rule, lit as a saved file changes it: a button opens its line in your editor or copies it; beside them, in the colors of the theme shown, which your terminals take only if they use it, a window of the theme, its tab bar, its scroll bar, a second pane and a terminal of every combination that it colors, drawn as the panes draw, and a plugin's panel of real controls, fields, toggles, a segmented control, a list, progresses and buttons, which take the pointer; a click on a part of them marks the tokens that it takes in the list.
- The Theme Editor edits your themes of Lokyr's format: a click on a color's row opens a field of its value as the file writes it, a color, or for an interface color an expression of other colors, as `accent / 40%`, written as you pause, press Return or leave the field, and shown at once; a value whose colors do not resolve is told under the field and not written, and Reset puts a color back as it was when the Editor showed the theme; the file keeps its comments and its order. A click on a color's swatch opens the system's color panel of its color, its opacity too for an interface color: the color shows at once as you move in it, the screen's eyedropper too, and is written into the file once the panel rests. A link at the foot of its colors opens the guide for theme authors.
- On the Themes page, a click on a theme of the appearance not in force, or its card's Use, asks whether to use it for its appearance alone or for both, a light theme for the dark appearance too.
- In the Theme Editor, a theme of the collection or a built-in one shows a lock beside its name, which makes a copy of it in your themes folder, its name asked, and shows the copy; an `.itermcolors` theme asks first whether to convert it to Lokyr's format, of the same name, the other appearance's theme of the same file too, its file moved to the Trash, from which you can put it back.
- The plugin API 0.1.4 reads every token of a theme as it resolves, each of its color and its origin, a file's path to Lokyr's own plugins alone and its name to the others, `cx.theme_tokens`, and tells a change of the theme in force, `theme-changed`; its read of the themes tells each one's format, `lokyr` or `itermcolors`; Lokyr's own plugins copy a theme, `cx.copy_theme`, make a new one, `cx.new_theme`, and convert an `.itermcolors` one, `cx.convert_theme`, each once its question is answered, and write a theme's colors, `cx.set_theme_value`, or show them before, `cx.preview_theme_value`; a plugin shows the system's color panel, `cx.show_color_panel`, of no capability, which one of Lokyr's own plugins keeps from the others while it shows it and which closes as its plugin stops, and hears its colors, `color-picked`; a plugin asks a question of buttons, `cx.ask_choice`, its answer `chosen`; `cx.set_theme` names a theme of either appearance; each color that the read of a theme gives tells its value as the theme gives it; a view's node, `themed`, draws its children in a theme of a name, whatever the window's theme.
- An HDR cursor, brighter than white on a display of extended dynamic range, as the MacBook Pro's: `[cursor] hdr` and `hdr-brightness`, from 1 to 4 times white; off by default, as the display then takes more power while the cursor shows.
- The cursor guide, a band across the cursor's row under the text, as iTerm2's: View › Show Cursor Guide, ⌥⌘;, or `[cursor] guide`; its color the theme's `cursor.guide`, or an `.itermcolors` file's Cursor Guide Color.
- `[text] faint-opacity`, from 0.1 to 1, 0.5 by default, the opacity of the dim text that programs write over its background, its underlines too.
- A theme's color can be an expression of other colors, as `accent / 40%`, `mix(background, foreground, 8%)` or `contrasting(list.selection.bg)`, which follows them as they change, as the authors' guide tells.
- New… on the Themes page makes a new theme of Lokyr's own light or dark values, as the page shows, every key written, the terminal's optional ones as comments of their defaults and the interface colors that it leaves to their rules as comments of the rules, so that the file shows every color: Lokyr asks its name, "New Dark Theme" first, writes it into the themes folder, uses it and shows it in the Theme Editor.
- View › Import Themes…, and Import… on the Themes page, beside Show in Finder, which opens the themes folder, imports `.itermcolors` and `.toml` themes, several chosen at once in the system's open panel: each is copied into the themes folder as it is, one question asking whether to replace those of a name there already; then one alert tells the themes imported, by their names, and the files that were not, as one that holds no theme.
- View › New Theme from Current… makes a new theme of every color of the theme in force, the interface colors that it leaves to their rules as comments of the rules: Lokyr asks its name, writes it into the themes folder, uses it and opens it in your editor; a theme's card on the Themes page has Use, Edit and Duplicate… in its context menu, Duplicate… making a copy of it, its name asked, used and its card chosen.
- The settings window's Keys page has a field at its top that filters its shortcuts as you type, by their commands' titles or names, their keys, as ⌘K or `cmd+k`, or their contexts.
- Window › Cycle Through Windows, ⌘`, brings forward your next window, the settings window too, and ⇧⌘` the previous one, of the key left of 1 by its place, as macOS's: ⌘\ on the Italian layout.

### Changed

- The settings window has a theme of its own, Lokyr Light in the light appearance and Lokyr Dark in the dark one, whatever theme your terminals use: a theme being edited no longer changes the page that edits it.
- A plugin's question names the plugin in its alert, unless it is one of Lokyr's own; its message and its detail are bounded, 256 and 1024 characters, of no character that hides or turns the text around them, and a plugin asks one question at a time, the next once the last is answered.
- A theme that extends Lokyr Light or Lokyr Dark and gives its own accent, text or background colors their interface colors that follow those, as the focus ring, the selected rows, a field's focus, the toggles and the cursor guide, of its own, where it kept Lokyr's before.
- What a theme colors nothing with no longer refuses it: a token's value that does not read, a name of no color, tokens that name each other and a token's name that Lokyr does not know are told in the card, of their line, the rest of the theme in force.
- A theme's interface colors follow the colors that it gives: a theme's `accent` colors the focus ring, a field's focus, the selected rows, the toggles, the chosen segments, the progress and the gutter of running commands too, and its text the fainter texts and the lines made of it, unless it gives them too; a theme of the terminal's colors alone looks as it did.
- The settings window's Keys are a page of their own, a tab between Settings and Themes, no longer a part of the Settings page's sidebar, whose search no longer lists the shortcuts.
- The settings window's pages draw anew faster, the Keys page above all, as the plugins' texts are read once a language rather than at each title and message; Lokyr starts a little sooner too.

### Fixed

- A block cursor on the cursor guide's row keeps its own color, rather than turning black as the band's color, of no opacity, read as too near it.
- A saved theme that changes the interface's colors alone, as its accent, colors the tab bar, the panels and the settings window at once, not once another theme was chosen and this one again; and the Themes page's preview of a theme in force follows its file as you save it.
- The settings' Keys page no longer lists ⌘W's Close Window, which Close Tab hides while the tabs run, nor a default that your config's binding of the same keys replaces, written another way, as `cmd+}` for ⇧⌘]; a new binding's shortcut tells the binding that runs it now, the last of its keys, and a binding that would do what its shortcut already does is none to add.
- `sleip check`, and the registry's checks, refuse a plugin's config schema larger than 64 KiB, the most that Lokyr reads of one; a schema of objects of nested defaults holds each default once.
- A part of a plugin's view no longer slides on its own as a part around it starts or stops sliding, as a dragged tab's close button did.
- A resize no longer clears a running command's output of a shell whose prompt marks its start alone: its prompt clears as the shell draws it again only where the shell tells its input or its commands too.
- The keyboard and a zoom stay on one pane: a pane that takes the keyboard, as Go to Next Agent or a notification's click brings it forward, ends its tab's zoom of another, and a pane zoomed takes the keyboard.
- A right click on a word, or ⌘ over it, no longer waits on a volume that does not answer, as a network's: a path there shows no link's underline, and a ⌘-click still opens it.
- A ⌘-click on a path of a volume that does not answer, as a network's, no longer holds Lokyr: the path resolves on a thread of its own, and opens as it answers.
- A binding of a character that the US layout types with Shift, as `cmd+}`, is its key with Shift, ⇧⌘], the same keys as `cmd+shift+]`: it runs as they are pressed, takes the place of a default of them, and `"cmd+}" = false` takes that default away.
- Two entries of `[keys]` of the same keys written two ways, as `opt+x` and `alt+x`, or `cmd++` and `cmd+shift+=`, read, the later of the file holding, and the notice's card tells them, where the whole config was refused.

## [1.1.0] - 2026-11-14

### Added

- A settings window, "Settings", of its own, which Lokyr › Open Settings Window opens, as does the command palette, or brings forward where it is open: its pages as tabs across the top, the page chosen below them; ⌘W, or Shell › Close Tab, closes it, no terminal's tab goes into it, and it shows none of the bars, buttons, tips, notices and panels of the terminals' windows, and stays opaque whatever `[window] background-opacity` says.
- A plugin's config schema may hold annotations of `x-`, as the text of a guide, which Lokyr checks no value against.
- `sleip check` checks a plugin's config schema as Lokyr reads it, at its line: a key twice in one object, which Lokyr now refuses, or the key `enabled`, which is Lokyr's.
- Themes color two more tokens of the interface, `text.success` and `text.warning`, the palette's green and yellow where a theme gives none.
- The settings window's Themes page shows every theme, the built-in ones, the collection's and yours, of a sidebar of the appearances, Dark and Light, each a card of a preview of its colors, a sample terminal that Lokyr draws, and its name, the theme in force marked: a click uses it for its appearance in your config's `[theme]`. The themes are read off the main thread, a progress showing meanwhile and each preview a placeholder until its colors come.
- Plugins' views show a theme's preview, a sample terminal in its colors that Lokyr draws, `theme-preview` of a theme's name; plugins hear `themes-changed` as the themes' list is read or changes, which Lokyr reads off its main thread, a read of the themes telling `loading` meanwhile.
- The settings window's first page, Settings, shows every key of the config and of the bundled plugins' sections, by section in a sidebar of an icon each, whose search field shows the sections and the keys that hold its words, of their names, what they do and their sections' titles, the bindings too by their keys or commands, each of what it does, its platforms where it does not apply everywhere, a More info link to its text in the config guide, and its value in force, and changes it: a toggle, a menu of the key's values, or a field, a number's stepped by the arrow keys, written as typing pauses, on Return and as it is left, writes the key into the config file, its comments kept, the Reset link taking it back to its default, and a value that the key does not take tells why; a list's values, as the agents' programs, are removed and added one by one; a font's family and style and a plugin's color are picked from menus of the system's fonts and the theme's colors, the terminal's family and styles showing in the panes as their items rest highlighted, by the arrows or the pointer, until the menu closes, a family chosen commenting out the config's styles that it lacks; its Keys part lists every binding of keys, Lokyr's defaults and your config's, by the categories of their commands, each of its command's title, arguments and context, whose keys a click lets you press anew, writing the new keys and `false` for a default's old ones, a shortcut that another binding takes told and taken by a second press, Remove taking a default away and Restore giving it back, a default whose keys other defaults share staying, and Add, a panel that records the shortcut that you press, ⌘ and the menus' shortcuts too, while the settings window is in front, and offers the commands by category, telling a shortcut that another binding takes, macOS's own shortcuts, as Quit's ⌘Q, staying as they are; and a config file that does not read shows its error, at its line, the page writing nothing until it reads.
- Lokyr › Plugins… opens the settings window at its Plugins page: every plugin, the installed ones and the bundled ones, with its state, active or disabled, and its capabilities, to disable or enable, or, of an installed one, to update or remove, without editing the config; and the registry's plugins, which a field filters as you type, each of its versions and capabilities, to install without restarting.
- View › Open Themes Folder, which the command palette runs too, opens the folder of your own themes in the Finder, making it first if it is not there, so that a theme's file can be dragged in.
- Plugins show a notice in Lokyr's notice card, as Lokyr tells a config that does not read, `cx.notify(text, level?, title?)`: of an icon and a color of its level, info, success, warning or error, of the plugin's title beside its name, or its name alone, under Lokyr's own notices, going by itself after 4 seconds, 8 of a warning or an error, unless the pointer rests on it.
- Plugins read a window's title as its titlebar shows it, `WindowInfo.title`, to draw it in the system's place; a plugin without the permission to read the processes reads its tab's title alone, of no folder nor program.

### Changed

- The config's texts have bounds: a name, as a font's family or a theme's, 256 characters at most, a path or an address 1024; a longer one is an error at its line, where it took any length before.

### Fixed

- Choosing in a font picker the family that the config names already, after previewing another, shows it again, where the family previewed stayed in the panes.
- A file of the themes' folder that is no regular file, as a pipe, or of more than 1 MiB, or an `.itermcolors` file whose property list nests or repeats its values past what a color preset holds, is refused as a theme that does not read, where it could hold Lokyr or make it quit.
- With the tab bar out of the titlebar, at the bottom, in a column or below a titled bar, and two tabs or more, the window's title stands centred, as with a single tab, and no longer lies over the plugins' buttons beside the window's buttons.
- A single tab's title in the titlebar is the window's title, the grid's size too, in the system title's look, bold, and dims as the window loses the focus, alike with one tab and with several.

## [1.0.0] - 2026-11-13

### Added

- AI agents on their tabs: a tab running Claude Code, Codex, Gemini CLI, Aider or another agent that `[agents] programs` lists shows whether the agent is working, waiting for you, or done with what you asked.
- A notification tells when an agent waits for you or is done while its window has no focus, and the Dock's icon counts the agents that wait, as `[agents] notify` and `dock-badge` say.
- An agent that waits for you tells what it asked in its tab's tooltip, as "Claude needs your permission to use Bash", and in a tab of several panes a ring goes round the pane that waits.
- Window › Go to Next Agent, ⌥⌘A, brings forward the next agent that waits for you, or is done, or works, across every window.
- Window › Show Agents…, ⇧⌥⌘A, lists the agents of every window in the palette, the most urgent first, each with what it asked or its tab, its state and how long it has been in it; Return brings it forward.
- Lokyr › Install Claude Code Integration… adds hooks to Claude Code's settings, asking of each of its configurations and keeping a copy of the file, so that Claude Code tells its tab exactly when it works, waits for you, is done, or stops on an error, a red mark of its own.
- Quick tips for the agents, Go to Next Agent, Show Agents and Claude Code's integration.
- The plugin API 0.1.2: a plugin lists the agents of every window and opens a file at a line in the editor, of the capabilities `process-read` and `open`.

- ⌘-click on a path's line, as `src/main.rs:42:7` that compilers, tests and AI agents print, opens the file at that line in the editor that `[links] editor` names: VS Code, Cursor, Windsurf, Zed, Sublime Text, TextMate or an address of your own.

- Edit › Copy Command Output, ⌥⇧⌘C, and Select Command Output, ⇧⌘A, copy or select the output of the last command, or, from the pane's context menu, of the command under the pointer, without its prompt and its command line.

- About Lokyr links to lokyr.dev, the project's site.
- Lokyr installs with Homebrew, `brew install --cask sephiroth74/tap/lokyr`, which links `lokyr` and `sleip` too and leaves the updates to Lokyr's own.
- Inline images: programs show pictures in the terminal, as `imgcat` sends them (iTerm2's protocol) and as `kitten icat`, `chafa`, `timg` and file managers' previews send them (kitty's graphics protocol, images sent in the output itself); the images scroll with their lines and stay within 64 MiB of memory.

### Changed

- The icon's variant that `[app] icon` chooses stays in the Dock after Lokyr quits, rather than going back to the default.
- A click on a notification, a program's, an agent's or a long command's, brings forward the pane that sent it, with its tab and its window, rather than the window alone.

### Fixed

- `sleip add` of a plugin no longer fails telling that another plugin has "one version" made for an older Lokyr when the other is kept at the version that `plugins.lock` holds: Sleip says so, and that `sleip update` of it takes a newer version.
- `sleip info` no longer counts a yanked version when it tells that this Lokyr takes none of a plugin's versions.
- `sleip dev` checks again a plugin saved while it was linking it.
- An agent that a shell starts with `exec` no longer misses its first request for you.
- A tab no longer keeps the title of a program that ended, as the remote shell's after you leave `ssh`: the title from before comes back, unless your shell sets its own at its prompt.

## [0.9.0] - 2026-11-12

### Added

- Plugins from the registry: `sleip search` lists the plugins on [pkg.lokyr.dev](https://pkg.lokyr.dev), each marked when it needs another version of Lokyr, `sleip info owner/name` tells one, and `sleip add owner/name` installs it, checked against the registry's signed index; `sleip update` takes the new versions. Lokyr loads a plugin as it is installed, without restarting.
- The first plugins: `sephiroth74/snap` makes an image of the terminal's text, of the selection or the screen, in a frame like a macOS window's, to copy or to save; `sephiroth74/awake` keeps the Mac awake from a button beside the window's buttons, always or while a tab runs a program; `sephiroth74/font-picker` chooses the terminal's font among the monospaced families, previewed as you move through them.
- `sleip publish` proposes a version of a plugin to the registry; a guide for plugin authors goes from `sleip new` to a published plugin, and a guide for theme authors tells every color of a theme.
- Notices as cards: an error or a warning comes down from the top right of the window in a card of two lines, whose buttons copy the whole message and close it; a failure goes by itself after a few seconds, unless the pointer rests on it.
- Quick tips: once a week at most, as Lokyr starts, a card at the window's bottom right tells a shortcut as your keymap binds it, or a feature that shows in no menu; More shows a long tip whole, Don't show again stops them, Help › Show a Tip shows one at once, and `[plugins.quick-tips] every` sets the days between two tips.
- Lokyr › Install Command Line Tools… links `lokyr` and `sleip` into `/usr/local/bin`, so that they run in any terminal.
- `[cursor] length`, from 0.1 to 1, the share of the cell that a bar or an underline cursor takes.

### Fixed

- Lokyr run from its disk image asks to be moved to Applications first, as its updates and its command line tools need.
- A tab shows its program running while any of its panes runs one.
- A plugin whose installed folder moved keeps running.
- A failed `sleip add` or `sleip update` leaves the installed plugins as they were.
- A `permissions.toml` that does not read keeps your answers, and its notice shows once.

### Security

- A file that a plugin saves through the save panel is marked as downloaded, as a browser's are, so that macOS checks it before it opens.

## [0.8.0] - 2026-11-09

### Added

- Plugins in Luau, each in a sandbox of its own, which reach only what you allowed them: the first time a plugin runs, Lokyr asks for the capabilities that it declares, and again for those that a new version adds; your answers are kept in `permissions.toml`, beside your config, and a plugin whose permission you take back there stops at once. A plugin that runs too long, takes too much memory or keeps failing is stopped, and Lokyr goes on.
- Sleip, the plugins' package manager, comes with Lokyr: `sleip add --path <folder>` or `sleip add --git <url> --tag <tag>` installs a plugin, `sleip update` takes its new version, `sleip remove` takes it away, and `sleip list` and `sleip doctor` tell what is installed and whether it is sound. Lokyr loads a plugin as it is installed or updated, without restarting.
- Writing a plugin: `sleip new owner/name` makes one that runs, with its test; `sleip check` checks its manifest, its translations and its code's types, `sleip test` runs its tests, and `sleip dev` loads it into Lokyr as you write it, again at each save.

### Changed

- The download grows from 9.0 MB to 14.7 MB, and the app from 16.5 MB to 35.3 MB, as it holds Sleip and Luau.

### Fixed

- With `quit-after-last-window-closed` on, the windows you had open come back at the next start.
- A window kept on a display that is no longer there opens on one that is, at a size that fits it.
- A script opened from the Finder runs with its own interpreter, as a Python or a Ruby script.
- Lokyr › Make Lokyr the Default Terminal follows another app taking the default while Lokyr runs.

### Security

- Lokyr asks before it runs a script that you downloaded.

## [0.7.0] - 2026-11-08

### Added

- Themes: about 280 themes of the iTerm2-Color-Schemes collection, a light one and a dark one in force, as `[theme]` names them, which follow macOS's appearance; View › Choose Theme… previews each as you move through them. Double-click an `.itermcolors` file, or drop it on Lokyr's icon in the Dock, to install it; your own themes go in `~/.config/lokyr/themes/`, and `minimum-contrast` keeps text readable on any of them.
- The command palette: ⇧⌘P finds any command by its name, and runs it.
- Shell integration for zsh, bash and fish, without touching your files: ⌘↑ and ⌘↓ jump between prompts, ⇧⌘↑ and ⇧⌘↓ by line; a bar beside each prompt tells whether its command succeeded; the command whose output you read stays at the pane's top as it scrolls; a notification tells when a command of 10 seconds or more ends while you look elsewhere, as `[notifications] long-command` says.
- Copy mode: ⇧⌘C moves a cursor through the screen and the scrollback with vi's keys; `v`, `V` and ⌃V select characters, lines or a block, and `y` copies.
- A scroll bar over each pane, as macOS shows its own, with the find bar's matches marked along it.
- Programs' progress, as build tools and Claude Code tell it, in a thin bar across the window's top, besides round its tab.
- Windows that open again as Lokyr starts, each where it was, with its tabs, splits and folders, as macOS's "Close windows when quitting an application" says, or as `[app] restore-windows` decides.
- New Lokyr Window Here and New Lokyr Tab Here on a folder in the Finder, and a folder dropped on Lokyr's icon in the Dock opens a window there.
- Lokyr › Make Lokyr the Default Terminal opens your `.command` scripts, `ssh://` links and man pages' links in Lokyr.
- Window › Float on Top keeps a window above the other apps'.
- A tab's color from its context menu, of macOS's colors.
- `[cursor] movement = "snappy"` slides the cursor to its new place.
- Editors such as Neovim and Helix learn whether the theme is light or dark, and follow its changes.
- Split Left and Split Up, Close Pane, Copy Link Address, New Window and New Tab at Folder, Search man Page Index, and Search the Web, of `[links] search-engine`, in the menus.

### Changed

- Translucent windows show the desktop through the tab bar too, with clean corners.
- The quick terminal's settings move from `[quick-terminal]` to `[plugins.quick-terminal]`: Lokyr tells you of an old `[quick-terminal]` section, and ignores it.
- ⇧⌘C starts the copy mode.

### Fixed

- The find bar keeps its matches through a resize and as the scrollback drops old lines, highlights wide characters whole, and follows text that a program rewrites in place.
- A find bar or the copy mode in a tab that you leave gives the keyboard back to the tab that you go to.
- The bell of a window whose tab bar sits in the titlebar marks the right tab.
- bash's integration works under `set -u`, and keeps its own `$_` and DEBUG trap.

## [0.5.1] - 2026-11-07

### Added

- Tabs: ⌘T opens a tab, ⌘1 to ⌘9 select one, ⇧⌘] and ⇧⌘[ the next and the previous. The tab bar sits in the titlebar by default, or at the bottom, or in a column on the left or the right, in three looks, `standard`, `unified` and `seamless`, as `[plugins.tabs]` says. Each tab shows its title, the keys that select it, its color and a close button, and tells what happens in it while you look elsewhere: new output, a bell, a program that runs, Secure Keyboard Entry, and a program's progress round its outline.
- Tabs with the mouse: a middle click closes a tab, a double click renames it, and its context menu renames it, closes it, or closes the others. Drag a tab to reorder it, to another window's bar, onto a pane, which it splits, or out of the window, into a window of its own; Escape puts it back.
- A single tab hides the bar, and the window shows its title and its marks in the titlebar.
- Split panes: ⌘D splits the pane to the right, ⇧⌘D down; ⌥⌘ and an arrow move to the pane beside, ⌘] and ⌘[ to the next and the previous; ⌃⌘ and an arrow resize, and ⇧⌘Return zooms a pane to the whole tab. Shell › Equalize Panes shares the room again, and a pane moves out to a tab or a window of its own. Drag a divider to resize, or double-click it to share half and half. `[plugins.splits]` sets how much the panes without the keyboard fade, shows each pane's title, and has the keyboard follow the pointer.
- Find: ⌘F opens a find bar at the top right of the pane, which searches the screen and the scrollback as you type, highlights every match that shows and counts them; Return and ⌘G go to the next match, ⇧Return and ⇧⌘G to the previous one. Its toggles match case, whole words and regular expressions, and its magnifying glass lists your recent searches.
- ⌥⌘1 to ⌥⌘9 bring a window forward, its number shown in its titlebar, which `[window] title-shortcut = false` hides.
- `[app] quit-after-last-window-closed = true` quits Lokyr as its last window closes.
- The default shortcuts are a plugin of their own, which `[plugins.keymap-macos] enabled = false` turns off, so that your `[keys]` bind every key.

### Changed

- ⌘W closes the tab, or its pane in a tab of several; ⇧⌘W closes the whole window.
- A program that follows the mouse keeps a drag in its pane until you release the button, wherever the pointer goes.
- Moving between panes tells their programs that they gained or lost the keyboard, and each tab shows the title of its pane that has the keyboard.

### Fixed

- The underline of a link under a still pointer, while you hold ⌘, goes as soon as its text changes or scrolls away.

## [0.5.0] - 2026-11-05

### Added

- Lokyr › About Lokyr shows the version and build, the copyright and license, and links to the source code and to the licenses of the software that Lokyr includes.
- `enabled = false` in a built-in plugin's section of the config, as `[plugins.links]` or `[plugins.core-actions]`, turns that plugin off as soon as you save the file, and `true` turns it on again; a wrong value of a plugin's key is told in the window at its line, and the rest of the config applies.

### Changed

- Resizing a window: the text follows the window's edge as you drag it, at the display's rate, 60 or 120 Hz, no longer stretched or held for seconds, and a prompt that the shell draws no longer flickers.
- When a built-in plugin stops on an error, a notice in the window tells it, and the rest of Lokyr goes on.

### Fixed

- VoiceOver follows the cursor when it moves without new output, as with the arrow keys at a shell's prompt.

## [0.4.0] - 2026-11-04

### Added

- VoiceOver reads the terminal's screen and follows its cursor as programs write.
- Edit › Emoji & Symbols, ⌃⌘Space, opens macOS's palette of emoji and symbols, and the character you choose goes to the program in the terminal.
- `[font] system-weight = true` draws the text as macOS draws its own, dark text on a light background thinner; off by default.
- When `[terminal] term` names a type that the Mac has no terminfo description of, Lokyr tells it in the window, and `lokyr +validate-config` tells it too.

### Changed

- The first window shows a refresh sooner, as it did before 0.3.0.
- Lokyr's log keeps its last lines when Lokyr quits or stops on an error, and starts a new file once it passes a megabyte during a session.

### Fixed

- A path written with escaped spaces, as `My\ File.txt`, is underlined again under ⌘, and a click opens it.
- A full disk, or an error writing the log, no longer ends Lokyr with every shell in its windows.
- When the config has an error, or names a font that is not installed, Lokyr no longer checks for updates against `[updates] check = false`.
- The notices of the licenses list only the software that Lokyr includes.

## [0.3.0] - 2026-11-02

### Added

- Links under ⌘: hold Command and move the pointer over a link, a web address, a file or a folder: Lokyr underlines it and shows the pointing hand, and a click opens it.
- Updates: Lokyr checks for a newer version once a day at most, and Lokyr › Check for Updates… checks at once; a new version shows with its notes, to install now or as Lokyr quits. `[updates] check = false` turns the daily check off.
- Lokyr comes as a disk image, signed and notarized by Apple.
- Lokyr writes what went wrong to its log, `~/Library/Logs/Lokyr/lokyr.log`, and Help › Report an Issue… opens a new issue with Lokyr's version and macOS's already written.
- The notices of the licenses of the software that Lokyr includes, in `Lokyr.app/Contents/Resources/licenses`.

### Fixed

- A `file:` link without a path no longer ends Lokyr as it opens.
- After output scrolls the window, the context menu's commands act on the text that was clicked, not on what came to its place.

## [0.2.0] - 2026-11-02

### Added

- The first numbered version, not distributed: the macOS app, a window of one pane, of Lokyr's own terminal core and Metal renderer, configured by `~/.config/lokyr/config.toml`.

[Unreleased]: https://github.com/sephiroth74/lokyr/compare/v1.5.0...HEAD
[1.5.0]: https://github.com/sephiroth74/lokyr/compare/v1.4.0...v1.5.0
[1.4.0]: https://github.com/sephiroth74/lokyr/compare/v1.3.0...v1.4.0
[1.3.0]: https://github.com/sephiroth74/lokyr/compare/v1.2.0...v1.3.0
[1.2.0]: https://github.com/sephiroth74/lokyr/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/sephiroth74/lokyr/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/sephiroth74/lokyr/compare/v0.9.0...v1.0.0
[0.9.0]: https://github.com/sephiroth74/lokyr/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/sephiroth74/lokyr/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/sephiroth74/lokyr/compare/v0.5.1...v0.7.0
[0.5.1]: https://github.com/sephiroth74/lokyr/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/sephiroth74/lokyr/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/sephiroth74/lokyr/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/sephiroth74/lokyr/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sephiroth74/lokyr/releases/tag/v0.2.0
