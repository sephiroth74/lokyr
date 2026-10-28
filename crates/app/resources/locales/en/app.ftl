# The app's texts, in English, the source of the other languages.

# The titles of the app's commands, which its menus show.
command-window-new = New Window
command-window-close = Close Window
command-app-quit = Quit Lokyr

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
