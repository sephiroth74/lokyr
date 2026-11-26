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
tip-palette = Press { $keys } for the command palette, which runs any command or menu item by its name, as Split Right.
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
tip-block-header = Scrolled up through a long output? The command's prompt stays at the pane's top: click it to go back to the command.
tip-block-copy = Right-click a command's output and open Block to copy the command, its output or both, or to select its output or its whole block. Right-click its pinned rows at the top for the same menu.
tip-block-details = Right-click a command's output and choose Block › Block Details to see its exit status, when it started, how long it ran and the folder where it ran.
tip-select-block = Click a command's block, or press { $keys } for the last one, to select it: ↑ and ↓ move to the others, ⌘C copies it, and Escape ends the selection.
tip-block-rerun = Right-click a command's output and choose Block › Run Command Again to run it once more at the prompt.
tip-block-spacing = Want room between your commands? Set spacing = true in [plugins.blocks] for a blank row before each prompt.
tip-tab-drag = Drag a tab out of the bar for a window of its own, or onto a pane to split beside it.
tip-tab-menu = Right-click a tab to give it a color, or double-click it to rename it.
tip-pin-tab = Right-click a tab and choose Pin Tab to keep it narrow at the start of the bar. Neither ⌘W nor Close Other Tabs will close it.
tip-tab-groups = Right-click a tab and choose New Group with Tab to gather tabs under a name and a color. Click the group's pill to fold it, or drag a tab right after it to put it in.
tip-restore-scrollback = Set restore-windows = "always" and restore-scrollback = true in your config's [app], and your windows open again as you left them, each pane showing what it showed when you quit, its commands too.
tip-hints = Press { $keys } to label the links, paths and hashes on the screen, then type a label to open it; a capital copies it.
tip-themes = View › Choose Theme… previews each theme as you move through the list.
tip-themes-folder = Put a .toml or .itermcolors theme in the folder that View › Open Themes Folder shows in the Finder, and Choose Theme… lists it.
tip-new-theme = New… on the Themes page makes a theme of every key of Lokyr's own; View › New Theme from Current… one of the theme in force, and a card's context menu duplicates any theme.
tip-import-themes = View › Import Themes…, or Import… on the Themes page, copies .itermcolors and .toml themes into your themes folder, several at once.
tip-long-command = A long command tells you as it ends while Lokyr is in the background: [notifications] long-command in the config sets how long.
tip-look-up = Right-click a word to look it up, or to search the web or the man pages for it.
tip-services = In the Finder, right-click a folder and choose Services › New Lokyr Tab Here for a terminal there.
tip-shortcuts = The Shortcuts app has Lokyr's actions: open a window or a tab that runs a command, switch to a session, or get a terminal's text, from a shortcut or Spotlight.
tip-applescript = Scripts can drive Lokyr: osascript -e 'tell application "Lokyr" to create window in folder "~" with command "ls"' opens a window that runs ls.
tip-itermcolors = Double-click an .itermcolors file, or drop it on Lokyr's Dock icon, to install its theme.
tip-float = Window › Float on Top keeps the window above every other app's.
tip-sessions = Window › Sessions › New Session opens a session of its own, a window of a new shell, and hides your other windows, their programs running, until you switch back.
tip-layouts = Window › Sessions › Save Session as Layout… saves your windows, tabs and splits to a file; add a command to a pane there, and the switcher opens the layout as a new session that runs it.
tip-session-switcher = Press { $keys } to switch to a session, or to open one in a project's folder: add folders = ["~/src/*"] to a [sessions] section of your config to list your projects.
tip-last-session = Press { $keys } to switch to the session that you used before, its windows where you left them.
tip-floating = Press { $keys } for a pane that floats over the tab, such as a scratch shell; drag its title to move it, and press ⌥⌘F to hide it and show it again.
tip-popup = Bind a key to `pane.popup` with a command, such as lazygit, to run it in a popup over the tab that closes as the command ends.
tip-broadcast = Press { $keys } to type into every pane of the tab at once, such as the same command on several servers; a red ring marks each pane, and the same keys stop it.
tip-smooth-scroll = Set smooth-scroll = true under [window] in the config, and the trackpad scrolls the text by pixels, gliding on as you lift your fingers; a mouse wheel's step glides too.
tip-cursor-guide = Press { $keys }, or View › Show Cursor Guide, for a band across the cursor's row that shows where it is.
tip-transparency = Press { $keys }, or View › Use Transparency, to show a translucent window opaque while you read, and again to see through it; your config stays as it is.
tip-sleip = Plugins come from the registry: `sleip search` lists them, and `sleip add` installs one.
tip-settings-window = Lokyr › Open Settings Window shows every setting by its section, the plugins' too, with what it does, and changes it in your config file, its comments kept.
tip-settings-keys = On the settings window's Keys page, type in its filter to find a shortcut, click its keys and press the new ones, or Add a shortcut of your own: every default is there, and Restore gives one back.
tip-settings-themes = The settings window's Themes page shows every theme of a preview of its colors: a click uses it.
tip-theme-editor = Editor, on the settings window's Themes page, shows every color of a theme and where it comes from, and edits your own: click a color's row to type its value, or its swatch to pick it in the color panel. Edit, in a theme card's context menu, shows that theme in it.
tip-settings-search = In the settings window, type in the search field at the sidebar's top to find a setting by its name, what it does, or a shortcut's keys.
tip-settings-plugin-on = Turn a bundled plugin, such as Blocks, on or off with the Enabled switch at the top of its section in Settings.
tip-plugins = Lokyr › Plugins… lists every plugin, its state and what it may do, disables, updates or removes one, and installs those of the registry.

# The tips of an installed plugin's command, `$title`: of its keys, or of
# none, of the command palette's.
tip-plugin-keys = Press { $keys } for “{ $title }”, a command of an installed plugin.
tip-plugin-palette = “{ $title }”, a command of an installed plugin, runs from the command palette: press { $keys } and type its name.
