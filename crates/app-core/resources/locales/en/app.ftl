# The app's texts, in English, the source of the other languages.

# The titles of the app's commands, which its menus show.
command-window-new = New Window
command-window-close = Close Window
command-window-set-title = Set Title…
command-window-goto = Go to Window
command-window-next-agent = Go to Next Agent
command-window-toggle-float-on-top = Float on Top
command-app-about = About Lokyr
command-app-quit = Quit Lokyr
command-app-check-for-updates = Check for Updates…
command-app-settings = Settings…
command-app-report-issue = Report an Issue…
command-app-secure-keyboard-entry = Secure Keyboard Entry
command-app-make-default-terminal = Make Lokyr the Default Terminal
command-app-install-command-line-tools = Install Command Line Tools…
command-app-install-claude-code-integration = Install Claude Code Integration…
command-app-performance = Show Performance
command-app-open-themes-folder = Open Themes Folder
command-app-settings-tab = Open Settings Tab
performance-measuring = Measuring…
performance-frames = { $rate } FPS
performance-cpu = CPU { $median } ms, at most { $longest }
performance-wait = Display wait { $median } ms, at most { $longest }
performance-gpu = GPU { $mean } ms, at most { $longest }
performance-wakeups = { $wakeups } wakeups/s, { $idle } from idle
performance-memory = Memory { $memory } MB
performance-output = Output { $output } MB/s
command-appearance-system = Automatic Appearance
command-appearance-light = Light Appearance
command-appearance-dark = Dark Appearance
command-pane-look-up = Look Up
command-pane-man-page = Open man Page
command-pane-man-search = Search man Page Index
command-pane-search-web = Search the Web

# The notice of a plugin that a fault stopped, by its name.
plugin-stopped = The plugin { $name } stopped, and its views show as they were, taking no clicks: { $fault }
plugin-capability-withheld = The plugin { $name } asks for { $capability }, which no plugin has yet: it runs without it.
plugin-capability-first-party = The plugin { $name } asks for { $capability }, which Lokyr's own plugins alone have: it runs without it.
plugin-not-loaded = The plugin { $name } does not load: { $why }
plugin-other-api = The plugin { $name } takes the plugin API { $api }, and this Lokyr has { $version }: it does not load until it or Lokyr is updated.
plugins-lock-unread = The installed plugins do not load, as plugins.lock does not read: { $error }

# The question of the capabilities of a plugin that activates, which the user
# allows or not: a line of each capability, what it allows and its risk.
consent-message = Allow { $name } to use these capabilities?
consent-detail = Lokyr asks once: the answer is kept in permissions.toml, beside the config file, where it can be changed.
consent-allow = Allow
consent-refuse = Don't Allow
consent-line = • { $what }: { $risk }
consent-risk-very-high = very high risk
consent-risk-high = high risk
consent-risk-medium = medium risk
consent-risk-low = low risk
consent-capability-terminal-read = read the text of the screen and of the scrollback, and the selection
consent-capability-pty-write = type into the panes' programs, as into the shell
consent-capability-clipboard-read = read the clipboard
consent-capability-clipboard-write = write to the clipboard
consent-capability-notifications = show notifications
consent-capability-open = open web pages and files with their apps
consent-capability-process-spawn = start programs
consent-capability-net = connect to { $parameter }
consent-capability-fs-read = read { $parameter } and what it holds
consent-capability-fs-write = write to { $parameter } and to what it holds
consent-capability-global-hotkey = take keys in every app, and slide quick windows over them
consent-capability-env-read = read the environment variable { $parameter }
consent-capability-env-read-any = read every environment variable
consent-capability-input-read = see the keys typed before the programs do
consent-capability-history-read = read the commands that the shell ran, and their output
consent-capability-process-read = see the panes' programs, their command lines and folders
consent-capability-user-commands = run any command as you would, typing into the shell among them
consent-capability-workspace-write = open, close and move windows, tabs and panes, and quit the app
consent-capability-pane-view = scroll and select in the panes, clear them, and change the tabs' titles and colors
consent-capability-decorate = draw over the panes' text
consent-capability-pointer-read = see where the pointer is over the panes
consent-capability-keep-awake = keep the Mac and its display awake, which keeps the screen from locking and uses the battery
consent-capability-appearance-write = preview themes and font families, and name the ones chosen in the config file
consent-capability-packages = install, update and remove plugins, which run code of their own
permissions-unread = The decisions on the plugins' capabilities do not read, and none applies: { $error }

# The credits of the About panel, under the version and the copyright: the
# license, and the links to the source code and to the licenses' notices of
# the software that Lokyr includes.
about-site = lokyr.dev
about-license = Released under the MIT License.
about-repository = Source code on GitHub
about-licenses = Licenses of the included software

# The menus of the menu bar, and the system's own items in them.
menu-shell = Shell
menu-edit = Edit
menu-view = View
menu-window = Window
menu-help = Help
menu-hide = Hide Lokyr
menu-hide-others = Hide Others
menu-show-all = Show All
menu-minimize = Minimize
menu-zoom = Zoom
menu-bring-all-to-front = Bring All to Front
menu-emoji-symbols = Emoji & Symbols
menu-appearance = Appearance
menu-appearance-system = Automatic
menu-appearance-light = Light
menu-appearance-dark = Dark
# The titlebar button that lists, in a menu, the titlebar buttons that find no room.
title-buttons-more = More
# The message over the save panel that a plugin shows, which names the plugin that saves the file.
save-panel-message = { $name } saves this file.
# A notice: a file that a plugin saved could not be written where the user chose.
save-failed = { $path } could not be saved: { $error }
# The context menu's Look Up, which names the selection, shortened.
menu-look-up = Look Up “{ $word }”
# The context menu's man page, which names the page.
menu-man-page = Open man Page for “{ $page }”
# The context menu's search of the man pages, which names the words.
menu-man-search = Search man Page Index for “{ $words }”
# The context menu's search of the web, which names the selection,
# shortened.
menu-search-web = Search the Web for “{ $text }”

# The alert of a program's read of the clipboard, and its buttons, the
# default one first.
clipboard-read-message = A program in this window wants to read the clipboard.
clipboard-read-detail = It would get the text that you copied last, which can hold a password. Allow it only if you trust the program, and the computer that it runs on.
clipboard-read-deny = Don't Allow
clipboard-read-allow = Allow

# The alert of a paste whose line breaks would reach a program as Return,
# with the text's first lines after the detail, and its buttons, the
# default one first.
paste-lines-message = { $lines ->
    [one] Paste a line with its line break?
   *[other] Paste { $lines } lines?
}
paste-lines-detail = The program in this window takes pasted text as typed: each line break reaches it as Return, which can run a command.
paste-lines-paste = Paste
paste-lines-one-line = Paste as One Line
paste-lines-cancel = Cancel
paste-lines-dont-ask = Don't ask again

# The questions before closing a window, or quitting, while a program other
# than a shell runs, which that ends, and their buttons, the default one
# first. A program whose name cannot be read is "a program".
close-running-message = Close this window?
close-running-detail = This window is running { $program }, and closing the window ends it.
close-running-close = Close
close-running-cancel = Cancel
close-tab-running-message = Close this tab?
close-tab-running-detail = This tab is running { $program }, and closing the tab ends it.
close-pane-running-message = Close this pane?
close-pane-running-detail = This pane is running { $program }, and closing the pane ends it.

# The question of a window's title, which a field holds, and its buttons,
# the default one first.
set-title-message = The window's title
set-title-detail = Leave it empty for the window's own: the program's title, or its folder, then the program that runs.
set-title-set = Set
set-title-cancel = Cancel
ask-text-ok = OK
ask-text-cancel = Cancel

# The alert of Settings… that opens nothing: what went wrong, as the
# system tells it, under the message.
settings-failed-message = Lokyr cannot open its config file
settings-no-home = Without a home directory, there is no config file: the defaults apply.

# The alert of Open Themes Folder that opens nothing: what went wrong, as
# the system tells it, under the message.
themes-folder-failed-message = Lokyr cannot open its themes' folder

# The settings tab: its title, as its tab shows it, and the page that tells
# that no page is there yet.
settings-tab-title = Settings
settings-tab-empty = No page yet: Lokyr's plugins add theirs.

# The alert of a plugin that the plugin manager turns on or off, as the
# config cannot be written: what went wrong, under the message.
plugin-enabled-failed-message = Lokyr cannot turn the plugin on or off
quit-running-message = Quit Lokyr?
quit-running-detail = { $windows ->
    [one] A window is running { $programs }, and quitting ends it.
   *[other] { $windows } windows are running programs: { $programs }. Quitting ends them.
}
quit-running-quit = Quit
quit-running-cancel = Cancel
program-unknown = a program

# The question before opening a link, a URL or a file, with the URL or the
# path in full under it, and its buttons, the default one first; a file can
# show in the Finder instead.
open-url-message = Open this link?
open-file-message = Open this file?
open-open = Open
open-reveal = Show in Finder
open-cancel = Cancel
open-url-dont-ask = Don't ask again for links
open-file-dont-ask = Don't ask again for files

# The notice's card at the panes' top right: its title, what it is about,
# above the error's own text, which the log has too; its buttons' tooltips.
notice-config = The config file does not apply
notice-font = A font is missing
notice-global-keys = A global key does not work
notice-term = The terminal type is unknown
notice-theme = A theme does not apply
notice-plugin-config = A plugin's section of the config does not apply
notice-kept-windows = The windows of the last run do not open again
notice-permissions = The plugins' permissions do not read
notice-installed-plugins = The installed plugins do not load
notice-failed = A command failed
notice-fault = A plugin stopped
notice-copy = Copy the details
notice-close = Close

# What an accessibility client, as VoiceOver, calls a pane.
access-pane = Terminal

# The question of a theme's file that the Finder opens with Lokyr, or that
# is dropped on its icon, and its buttons, the default one first; and the
# alert of a file that cannot be installed.
theme-install-message = Install the theme “{ $name }”?
theme-install-detail = Lokyr copies it into its themes' folder, { $folder }, where the config's [theme] can name it.
theme-install-replace = A theme of its name is there already, which this one replaces.
theme-install-use = Install and Use
theme-install-only = Install
theme-install-cancel = Cancel
theme-install-variant = { $variant ->
    [light] It is a light theme, which Lokyr uses in the light appearance, not the one in force now.
   *[dark] It is a dark theme, which Lokyr uses in the dark appearance, not the one in force now.
}
theme-install-switch = { $variant ->
    [light] Switch Lokyr to the light appearance
   *[dark] Switch Lokyr to the dark appearance
}
theme-install-failed = The theme cannot be installed.
theme-install-no-folder = Lokyr has no config folder to hold its themes.
theme-install-not-copied = It cannot be copied into { $folder }: { $error }

# The notification of a long command's end, under the command as its title:
# whether it succeeded, or its exit status, or neither as the shell told
# none; and how long it ran, of seconds, of minutes and seconds, or of hours
# and minutes.
command-succeeded = Succeeded after { $took }
command-failed = Failed with status { $status } after { $took }
command-ended = Ended after { $took }
command-took-seconds = { $seconds } s
command-took-minutes = { $minutes } min { $seconds } s
command-took-hours = { $hours } h { $minutes } min

# The notification of an AI agent, under its tab's title: its program's
# name, as `claude`, waits for the user, or is done with what it was asked.
agent-waiting = { $agent } is waiting for you
agent-done = { $agent } is done
agent-failed = { $agent } stopped on an error

# Claude Code's integration: the question of each of its configurations,
# the file that does not read or write, and what was done.
claude-code-ask = Add Lokyr's hooks to { $path }?
claude-code-ask-detail = Claude Code then tells Lokyr exactly when it works, waits for you, is done or stops on an error, while it runs in Lokyr; in another terminal they do nothing. The rest of the file stays as it is, and a copy of it is kept beside it, as settings.json.lokyr-backup.
claude-code-add = Add Hooks
claude-code-skip = Skip
claude-code-unreadable = { $path } is not Claude Code's settings as Lokyr reads them
claude-code-failed = Lokyr could not write { $path }
claude-code-added-message = Claude Code's integration is installed
claude-code-added-detail = The Claude Code sessions that start from now on tell Lokyr their state, of the hooks in:
    { $places }
claude-code-none-message = No settings changed
claude-code-none-detail = Claude Code's settings have Lokyr's hooks already, or you skipped them.

script-downloaded-message = “{ $name }” was downloaded from the internet.
script-downloaded-detail = Running a script from the internet can harm your Mac. Run it only if you trust where it came from.
script-downloaded-run = Run
script-downloaded-cancel = Cancel

# The command line tools, `lokyr` and `sleip`, linked into a folder of the
# shells' PATH: $value is a folder, a path or why it failed, $folder the
# folder, $path a link's path, $target the file that a link points to. The
# reason shows in the system's question of an administrator's password.
tools-reason = Lokyr wants to link lokyr and sleip into { $folder }.
tools-linked-message = The command line tools are installed
tools-linked-detail = Other terminals and scripts now run lokyr and sleip, linked in { $value }.
tools-there-message = The command line tools are installed already
tools-there-detail = lokyr and sleip in { $value } are this Lokyr's.
tools-replace-message = Replace what is in { $folder }?
tools-replace-link = { $path } is a link to { $target }.
tools-replace-file = { $path } is a file of another program.
tools-replace-replace = Replace
tools-replace-cancel = Cancel
tools-folder-message = { $value } is a folder
tools-folder-detail = Lokyr leaves it as it is: move it away, then install the command line tools again.
tools-missing-message = The command line tools are not beside Lokyr
tools-missing-detail = { $value } is not there, as when Lokyr runs outside its bundle.
tools-temporary-message = Move Lokyr to the Applications folder first
tools-temporary-detail = Lokyr runs from its disk image, or from a copy that macOS keeps aside until it is moved: the links would lead nowhere once it goes. Move Lokyr to the Applications folder, open it from there, then install the command line tools.
tools-failed-message = The command line tools could not be installed
tools-failed-detail = { $value }
