# The app's texts, in English, the source of the other languages.

# The titles of the app's commands, which its menus show.
command-window-new = New Window
command-window-close = Close Window
command-window-set-title = Set Title…
command-window-toggle-quick-terminal = Quick Terminal
command-app-quit = Quit Lokyr
command-app-check-for-updates = Check for Updates…
command-app-settings = Settings…
command-app-report-issue = Report an Issue…
command-app-secure-keyboard-entry = Secure Keyboard Entry
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

# The question of a window's title, which a field holds, and its buttons,
# the default one first.
set-title-message = The window's title
set-title-detail = Leave it empty for the window's own: the program's title, or its folder, then the program that runs.
set-title-set = Set
set-title-cancel = Cancel

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
notice-failed = A command failed:
notice-fault = A plugin stopped:

# What an accessibility client, as VoiceOver, calls a pane.
access-pane = Terminal
