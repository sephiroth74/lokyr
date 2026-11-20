# The quick tips' texts, in English, the source of the other languages.

# The title of its command, which the menus and the palette show.
command-quick-tips-show = Show a Tip

# The card: its title, its buttons, the one that dismisses it and the one
# that shows its whole text, and its checkbox, which stops the tips.
card-title = Did you know?
card-dismiss = Dismiss
card-more = More
card-never = Don't show again

# The tips of a command's keys, as the keymap binds them now: `$keys`.
tip-palette = Press { $keys } for the command palette, which runs any command by its name.
tip-search = Press { $keys } to find text in the pane, its scrollback with it.
tip-split = Press { $keys } to split the pane; drag the divider to resize it, double-click it to even the halves.
tip-prompts = Press { $keys } to jump back to the previous command's prompt.
tip-copy-mode = Press { $keys } for copy mode: vi's keys move and select, without the mouse.
tip-zoom = Press { $keys } to zoom the pane to the whole tab, and again to put it back.
tip-focus-pane = Press { $keys }, or the same with another arrow, to move the keyboard to the pane beside.
tip-goto-tab = Press { $keys } and the next numbers to go straight to a tab by its place.
tip-quick-terminal = Press { $keys } from any app to slide the quick terminal down.
tip-double-tap = Set [plugins.quick-terminal] double-tap = "ctrl" in the config, then press Control twice, alone, to show the quick terminal from any app.
tip-clear = Press { $keys } to clear the scrollback and the screen, the prompt kept.
tip-font-size = Press { $keys } to make the text bigger, in every window.
tip-switch-tab = Press { $keys } to go back to the tab you used before. Keep Control held and press Tab again to go further back.
tip-reopen-tab = Closed a tab by mistake? Press { $keys } right away to bring it back as you left it, its scrollback too.
tip-next-agent = Press { $keys } to go to the next AI agent that waits for you, in any window.
tip-cycle-windows = Press { $keys } to go to your next window, the settings window too, as Window › Cycle Through Windows does.
tip-claude-code = Lokyr › Install Claude Code Integration… lets Claude Code tell its tab exactly when it works, waits for you or is done.
tip-agents-list = Press { $keys } to list every AI agent of every window, the ones that wait for you first.

# The tips of features that need no keys.
tip-agents = A tab marks the AI agent in it, as Claude Code: a sparkle while it works, a dot while it waits for you, its request in the tab's tooltip, a tick once it is done.
tip-agent-status = Agents that tell their status, as Claude Code and Codex, show it exactly on their tab: what they ask for, their message and their progress, whether the config lists them or not.
tip-links = Hold ⌘ over a link to underline it, then click to open it.
tip-prompt-click = Click in the command line to move the cursor there, in zsh, bash and fish; select part of it and press Backspace, or type, to replace it.
tip-copy-output = Press { $keys } to copy the output of the last command, without its prompt; its context menu copies any command's.
tip-editor-line = Set editor under [links] to "vscode", "cursor" or "zed", and ⌘-click on src/main.rs:42 opens your editor at line 42.
tip-block-header = Scrolled up through a long output? Click the command's header at the pane's top to go to its prompt, or its arrow to go to the output's end.
tip-block-copy = Right-click a command's output to copy the command, or the command and its output, or to select its whole block.
tip-block-details = Right-click a command's output and choose Block Details to see its exit status, when it started, how long it ran and the folder where it ran.
tip-select-block = Click a command's block, or press { $keys } for the last one, to select it: ↑ and ↓ move to the others, ⌘C copies it, and Escape ends the selection.
tip-block-rerun = Right-click a command's output and choose Run Command Again to run it once more at the prompt.
tip-tab-drag = Drag a tab out of the bar for a window of its own, or onto a pane to split beside it.
tip-tab-menu = Right-click a tab to give it a color, or double-click it to rename it.
tip-pin-tab = Right-click a tab and choose Pin Tab to keep it narrow at the start of the bar. Neither ⌘W nor Close Other Tabs will close it.
tip-tab-groups = Right-click a tab and choose New Group with Tab to gather tabs under a name and a color. Click the group's pill to fold it, or drag a tab right after it to put it in.
tip-hints = Press { $keys } to label the links, paths and hashes on the screen, then type a label to open it; a capital copies it.
tip-themes = View › Choose Theme… previews each theme as you move through the list.
tip-themes-folder = Put a .toml or .itermcolors theme in the folder that View › Open Themes Folder shows in the Finder, and Choose Theme… lists it.
tip-new-theme = New… on the Themes page makes a theme of every key of Lokyr's own; View › New Theme from Current… one of the theme in force, and a card's context menu duplicates any theme.
tip-import-themes = View › Import Themes…, or Import… on the Themes page, copies .itermcolors and .toml themes into your themes folder, several at once.
tip-long-command = A long command tells you as it ends while Lokyr is in the background: [notifications] long-command in the config sets how long.
tip-look-up = Right-click a word to look it up, or to search the web or the man pages for it.
tip-services = In the Finder, right-click a folder and choose Services › New Lokyr Tab Here for a terminal there.
tip-itermcolors = Double-click an .itermcolors file, or drop it on Lokyr's Dock icon, to install its theme.
tip-float = Window › Float on Top keeps the window above every other app's.
tip-cursor-guide = Press { $keys }, or View › Show Cursor Guide, for a band across the cursor's row that shows where it is.
tip-transparency = Press { $keys }, or View › Use Transparency, to show a translucent window opaque while you read, and again to see through it; your config stays as it is.
tip-sleip = Plugins come from the registry: `sleip search` lists them, and `sleip add` installs one.
tip-settings-window = Lokyr › Open Settings Window shows every setting by its section, the plugins' too, with what it does, and changes it in your config file, its comments kept.
tip-settings-keys = On the settings window's Keys page, type in its filter to find a shortcut, click its keys and press the new ones, or Add a shortcut of your own: every default is there, and Restore gives one back.
tip-settings-themes = The settings window's Themes page shows every theme of a preview of its colors: a click uses it.
tip-theme-editor = Editor, on the settings window's Themes page, shows every color of a theme and where it comes from, and edits your own: click a color's row to type its value, or its swatch to pick it in the color panel. Edit, in a theme card's context menu, shows that theme in it.
tip-settings-search = In the settings window, type in the search field at the sidebar's top to find a setting by its name, what it does, or a shortcut's keys.
tip-plugins = Lokyr › Plugins… lists every plugin, its state and what it may do, disables, updates or removes one, and installs those of the registry.

# The tips of an installed plugin's command, `$title`: of its keys, or of
# none, of the command palette's.
tip-plugin-keys = Press { $keys } for “{ $title }”, a command of an installed plugin.
tip-plugin-palette = “{ $title }”, a command of an installed plugin, runs from the command palette: press { $keys } and type its name.
