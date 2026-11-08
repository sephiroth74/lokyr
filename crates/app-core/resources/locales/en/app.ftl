# The app's texts, in English, the source of the other languages.

# The titles of the app's commands, which its menus show.
command-window-new = New Window
command-window-close = Close Window
command-window-set-title = Set Title…
command-window-goto = Go to Window
command-window-toggle-float-on-top = Float on Top
command-app-about = About Lokyr
command-app-quit = Quit Lokyr
command-app-check-for-updates = Check for Updates…
command-app-settings = Settings…
command-app-report-issue = Report an Issue…
command-app-secure-keyboard-entry = Secure Keyboard Entry
command-app-make-default-terminal = Make Lokyr the Default Terminal
command-app-performance = Show Performance
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

# The credits of the About panel, under the version and the copyright: the
# license, and the links to the source code and to the licenses' notices of
# the software that Lokyr includes.
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

# The notice's bar across a window's top: what it is about, before the
# error's own text, which the log has too.
notice-config = The config file does not apply:
notice-font = A font is missing:
notice-global-keys = A global key does not work:
notice-term = The terminal type is unknown:
notice-theme = A theme does not apply:
notice-plugin-config = A plugin's section of the config:
notice-kept-windows = The windows of the last run do not open again:
notice-failed = A command failed:
notice-fault = A plugin stopped:

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

script-downloaded-message = “{ $name }” was downloaded from the internet.
script-downloaded-detail = Running a script from the internet can harm your Mac. Run it only if you trust where it came from.
script-downloaded-run = Run
script-downloaded-cancel = Cancel
