# The texts of the commands' blocks, in English, the source of the other
# languages.

# What an accessibility client, as VoiceOver, calls the command's header,
# and its tooltip, of what a click does, each of the command's whole line.
header-label = Command: { $line }
header-tooltip = Scroll to { $line }
# The header's button to the block's end: its tooltip, and what an
# accessibility client calls it.
header-end = Scroll to the end of { $line }

# The titles of the plugin's commands: `command-` and the command's name,
# with `-` for its `.`.
command-blocks-copy-command = Copy Command
command-blocks-copy-block = Copy Command and Output
command-blocks-select-block = Select Block
command-blocks-details = Block Details
command-blocks-rerun = Run Command Again
command-blocks-select-previous = Select Previous Block
command-blocks-select-next = Select Next Block
command-blocks-deselect = Deselect Block

# The block's menu, of its header's context click, and the pane's context
# menu's submenu of its actions: its title, and the titles of the commands
# of a command's output, which Lokyr's own actions give.
menu-block = Block
menu-copy-output = Copy Command Output
menu-select-output = Select Command Output

# A block's details: what an accessibility client calls the panel, of the
# command's whole line, and the names of its parts.
details-label = Details of { $line }
details-line = Command
details-status = Status
details-started = Started
details-duration = Duration
details-folder = Folder
# The tooltips of the parts' buttons, which copy them, and what an
# accessibility client calls them.
details-copy-line = Copy Command
details-copy-status = Copy Status
details-copy-started = Copy Start Time
details-copy-duration = Copy Duration
details-copy-folder = Copy Folder
# The block's state: a command that runs; one that ended of the exit status
# 0, of another, or of none that the shell told.
details-running = Running
details-succeeded = { $status } · Succeeded
details-failed = { $status } · Failed
details-ended = Ended
# A command's duration: below a second; below a minute, of its seconds and
# tenths; below an hour; and past it.
details-milliseconds = { $count } ms
details-seconds = { $whole }.{ $tenths } s
details-minutes = { $minutes } min { $seconds } s
details-hours = { $hours } h { $minutes } min

# Why a block's command does not run again: the notice's title, then its
# text of a block of no command line, of a shell that is not at its prompt,
# and of a command line that holds text.
rerun-title = Can't Run the Command Again
rerun-no-line = This block has no command line to run.
rerun-busy = The shell isn't at its prompt: a command is still running. Run it again once the command ends.
rerun-typed = The command line holds text. Clear it, then run the command again.
