# Changelog

All notable changes to Lokyr are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
from 1.0.0; before it, a minor version may change what a previous one did.

## [Unreleased]

### Added

- A settings window, "Settings", of its own, which Lokyr › Open Settings Window opens, as does the command palette, or brings forward where it is open: its pages as tabs across the top, the page chosen below them; ⌘W, or Shell › Close Tab, closes it, no terminal's tab goes into it, and it shows none of the bars, buttons, tips and panels of the terminals' windows.
- Themes color two more tokens of the interface, `text.success` and `text.warning`, the palette's green and yellow where a theme gives none.
- Lokyr › Plugins… opens the settings window at its Plugins page: every plugin, the installed ones and the bundled ones, with its state, active or disabled, and its capabilities, to disable or enable, or, of an installed one, to update or remove, without editing the config; and the registry's plugins, which a field filters as you type, each of its versions and capabilities, to install without restarting.
- View › Open Themes Folder, which the command palette runs too, opens the folder of your own themes in the Finder, making it first if it is not there, so that a theme's file can be dragged in.
- Plugins read a window's title as its titlebar shows it, `WindowInfo.title`, to draw it in the system's place.

### Fixed

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

[Unreleased]: https://github.com/sephiroth74/lokyr/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/sephiroth74/lokyr/compare/v0.9.0...v1.0.0
[0.9.0]: https://github.com/sephiroth74/lokyr/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/sephiroth74/lokyr/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/sephiroth74/lokyr/compare/v0.5.1...v0.7.0
[0.5.1]: https://github.com/sephiroth74/lokyr/compare/v0.5.0...v0.5.1
[0.5.0]: https://github.com/sephiroth74/lokyr/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/sephiroth74/lokyr/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/sephiroth74/lokyr/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/sephiroth74/lokyr/releases/tag/v0.2.0
