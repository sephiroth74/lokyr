# The titles of the plugin's commands, in English, the source of the other
# languages: `command-` and the command's name, with `-` for its `.`.
command-links-open = Open Link
command-links-copy = Copy Link Address
command-links-new-window = New Window at Folder
command-links-new-tab = New Tab at Folder
command-links-hints = Show Hints
command-links-hints-action = Next Hint Action

# The placeholder of the hints' field, which takes the letters typed.
hints-placeholder = Type a hint

# The hints' actions, which Tab moves through, shown before the field.
hints-action-open = Open
hints-action-copy = Copy
hints-action-type = Type
hints-action-select = Select

# The notice of the config's hints that are left out: a pattern that
# doesn't read, of its name and its error; a URL whose scheme doesn't
# open, of the schemes that do; an alphabet that isn't one.
hints-pattern-refused = The hint `{ $name }` has a pattern that doesn't read: { $error }
hints-url-refused = The hint `{ $name }` has a URL that doesn't open: only { $schemes } URLs open
hints-alphabet-refused = The hints' alphabet `{ $alphabet }` isn't lowercase letters and digits, each once: the hints keep the default

# The notice of a pick that failed, of the hint's text and the error.
hints-pick-failed = Couldn't use the hint `{ $hint }`: { $error }
