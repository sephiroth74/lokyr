# The app's texts, in English, the source of the other languages.

# The titles of the app's commands, which its menus show.
command-window-new = New Window
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
