# Blocks

<sub>macOS · zsh, bash, fish · off by default · the `lokyr/blocks` plugin · [`[plugins.blocks]`](CONFIGURATION.md#pluginsblocks) · not released yet</sub>

Lokyr sees each command that your shell runs as a block: its prompt, the command line you typed, and the output that the command printed. Blocks is off until you turn it on. When you scroll up through a long output, the block's prompt and command line stay at the top of the pane, so that you always know what you are reading. One click takes you back to the start of the block.

## Try it

Turn Blocks on, in your config file or on the Plugins page of the settings window, as [Turn blocks on](#turn-blocks-on) shows. Then run this command:

```sh
seq 1 500
```

Scroll up into the numbers. The prompt and `seq 1 500` stay at the top of the pane, with a thin line under them, over the numbers that scroll. Click them: the view moves up to the command's prompt, and the pinned rows go.

## What it gives you

- **The command you are reading**: as soon as the first row of a block's prompt scrolls above the view, the block's prompt rows stay pinned at the pane's top, over the rows that scroll under them: its prompt and its command line as your shell drew them, three rows at most, with a thin line under them. When the block's last rows reach the top, the next block pushes them up, so that they never cover its prompt · not released yet.
- **The block's start**: a click on the pinned rows scrolls the command's prompt to the top of the pane.
- **A pill instead**: with `header = "pill"`, the command line shows in a pill on a thin line across the pane's top, as the [command header](plugins/command-header.md) shows it. A click on the pill goes to the prompt, and an arrow beside it goes to the end of the output, with its last row at the bottom of the pane, or, for a command that is still running, to the bottom of the screen, which the view then follows as the output grows.
- **The command as you typed it**: zsh, and fish 4 or later, report the command line itself, so a right prompt on its row, such as zsh's `RPROMPT`, stays out of the pill.
- **Long lines stay short**: in the pill, a command line longer than 80 characters is cut with an ellipsis, `…`. The pill's tooltip, and what VoiceOver reads, keep the whole line.
- **Copy a block's parts**: right-click a block and open **Block** to copy its command line, its output, or both, or to select its output or the whole block · not released yet.
- **A block's details**: right-click a block and open **Block** to see its exit status, when it started, how long it ran and the folder where it ran · not released yet.
- **Run a command again**: right-click a block and open **Block** to run its command line again at the prompt · not released yet.
- **The header's menu**: right-click the pinned prompt rows, or the pill, for a menu of the block's actions alone · not released yet.
- **Blocks you can tell apart**: a thin line divides each block from the one before, and the gutter's bar of each command runs along its whole block · not released yet.
- **Room between the blocks**: with `spacing = true`, a blank row comes before each prompt that follows a command, with the line between the blocks through its middle · not released yet.
- **Select a block**: a click or ⌘↑ selects a block, ↑ and ↓ move to the block before or after, and ⌘C copies it · not released yet.

## Copy a command or its output

Right-click anywhere in a block, on its prompt, its command line or its output, open **Block**, and choose:

- **Copy Command** to copy the command line, as you typed it.
- **Copy Command Output** to copy the output alone.
- **Copy Command and Output** to copy the command line, then its output on the next lines, without the prompt.
- **Select Command Output** to select the output alone.
- **Select Block** to select the whole block, from its prompt to the end of its output, as a drag would.

The **Block** submenu holds every action on the block, its details and Run Command Again too, so that the context menu stays short. While Blocks is off, Copy Command Output and Select Command Output show in the context menu itself.

### From the block's header

Right-click the prompt rows pinned at the top of the pane, or the pill with `header = "pill"`, or ⌃-click them, for a menu of the same actions on that block alone. A click on them still takes you to the block's prompt. Block Details opens its panel at the top of the pane.

The same commands are in the Edit menu and the command palette, where they act on the last command: Copy Command takes the last command that started, even one that still runs, and the other two the last command that ended. A command that still runs has no output to copy yet, so Copy Command and Output and Select Block are dimmed in its block.

A block whose prompt has left the scrollback still copies and selects the rows that the pane holds.

## See a block's details

Right-click anywhere in a block and choose **Block › Block Details**. A panel opens below the pointer with:

- **the command line**, as you typed it;
- **the status**: the exit status, 0 when the command succeeded and another number when it failed, or Running while the command still runs;
- **the start**: the time of day when the command started, with the date when it wasn't today;
- **the duration**: how long the command ran, once it ended, such as `1.2 s` or `3 min 5 s`;
- **the folder** where the command ran, as your shell reported it. When the shell runs on another machine, such as over ssh, the machine's name comes first, as `server:/srv/app`.

Click a line, or its copy button, to copy what it shows: the button turns into a tick. For the status, the copy is the exit status alone. Press Escape, or click outside the panel, to close it. Tab moves between the copy buttons, and Space copies.

Edit › Block Details, with no click, shows the last command that started, below the cursor. While the command runs, the panel says Running, and it shows the status and the duration as soon as the command ends.

## Run a command again

Right-click a block and choose **Block › Run Command Again**. Lokyr types the block's command line at your shell's prompt and presses Return for you.

It runs only while your shell waits at its prompt with an empty command line. Otherwise Lokyr sends nothing, and a notice tells you why:

- a command is still running, or a program such as vim shows its own screen;
- the command line holds text that you typed, which the command would be added to: clear it first;
- the block has no command line.

The command line goes in as a paste, so a command of several lines stays whole, and Lokyr doesn't ask you to confirm it, as it can for a paste of several lines. Edit › Run Command Again, with no click, runs the last command that started.

## Select a block

Click a block to select it: its rows are tinted, across the pane. A click at the prompt where you type, or below the last block, selects none.

Press ⌘↑ to select the last block from the keyboard. Press ↑ and ↓ to select the block before or after: the view follows, with the selected block's prompt at the top. Below the last block, ↓ selects none and takes you back to the prompt where you type.

When you have scrolled up, ⌘↑ and ⌘↓ jump to the prompt above or below the top of the view, as they do without Blocks, and select its block.

While a block is selected:

- **⌘C** copies its command line and its output, unless you selected some text, which ⌘C copies as usual;
- **Copy Command**, **Select Block**, **Block Details** and **Run Command Again**, from the Edit menu or the command palette, act on it;
- **Escape**, any key that you type, a drag that selects text, or a click at the prompt where you type ends the selection; a click on another block selects it.

The tint is your theme's `blocks.selected.bg`, a shade of the accent by default.

## Dividers and the gutter's bar

While Blocks runs, a thin line divides each block from the one before, at the top of its prompt, across the pane. The prompt where you type has one too, so the last command's output stays apart from what you type.

The gutter's bar of each command runs along its whole block, from its prompt to the end of its output: gray when the command succeeded, red when it failed, and in the accent color while it runs, as long as the output grows.

A failed block shows its red bar alone. To tint its rows too, in a faint red, turn on `failed-tint`:

```toml
[plugins.blocks]
dividers = true       # the line between the blocks
failed-tint = true    # a failed block's rows tinted
```

The colors are your theme's: `blocks.divider` for the line, `blocks.failed` for a failed block, `blocks.selected.bg` for the selected one, and the gutter's colors for the bars. The Theme Editor shows them in its terminal's preview.

## Room between the blocks

To put a blank row between the blocks, turn on `spacing`:

```toml
[plugins.blocks]
spacing = true    # a blank row before each prompt that follows a command
```

From the next prompt, a blank row comes before each prompt that follows a command, and the line between the blocks runs through its middle. A selected block's tint, and a failed block's, reach the line, so the two blocks meet there.

Lokyr puts the blank row there itself, as your shell starts its prompt, so your shell's config needs nothing, and it works in any shell that marks its prompts with OSC 133. No blank row comes before the first prompt, after you press Return on an empty command line, or at the top of the pane after `clear`.

## Change the header's look

By default, the header is the block's own prompt rows, in your terminal's colors, with the theme's `blocks.divider` line under them. To show the command line in a pill instead:

```toml
[plugins.blocks]
header = "pill"        # rows | pill
```

The pill's colors are tokens of your theme, so they follow the light and the dark appearance. Its keys in [`[plugins.blocks]`](CONFIGURATION.md#pluginsblocks) name the tokens, and set the text's size and weight, the room around the text, and where the pill sits:

```toml
[plugins.blocks]
header = "pill"
border = "accent"      # the theme's accent color around the pill
font-weight = "semibold"
align = "center"       # left | center | right
max-length = 60        # characters of the command line before the ellipsis
```

The changes apply as you save the file, to a header that shows too.

> [!TIP]
> If you changed the command header's look before Blocks came, move its keys from `[plugins.command-header]` to `[plugins.blocks]`: the command header's keys apply only while Blocks is off.

## Turn blocks on

Blocks is off by default, and Lokyr shows the plain [command header](plugins/command-header.md) in its place: the same pill, without the arrow, whose keys are in [`[plugins.command-header]`](CONFIGURATION.md#pluginscommand-header). To turn Blocks on:

```toml
[plugins.blocks]
enabled = true
```

You can also click Enable next to Blocks on the Plugins page of the settings window, which writes the same key. Remove the key, or click Disable, to turn Blocks off again: the command header comes back.

## Settings

- [`header`](CONFIGURATION.md#header): the block's prompt rows pinned at the pane's top, or a pill of its command line · not released yet.
- [`background`, `text`, `border`, `line` and `hover`](CONFIGURATION.md#background-text-border-line-hover-1): the theme's tokens of the pill's background, its text, its border, the line across the pane, and the color under the pointer.
- [`font-size`](CONFIGURATION.md#font-size-2) and [`font-weight`](CONFIGURATION.md#font-weight-1): the text's size and weight, in the terminal's font.
- [`padding-x` and `padding-y`](CONFIGURATION.md#padding-x-padding-y-2): the room around the text in the pill.
- [`align`](CONFIGURATION.md#align-1): where the pill sits along the line.
- [`max-length`](CONFIGURATION.md#max-length-1): the most characters of the command line that the pill shows.
- [`dividers`](CONFIGURATION.md#dividers): the line between the blocks · not released yet.
- [`failed-tint`](CONFIGURATION.md#failed-tint): a failed block's rows tinted · not released yet.
- [`spacing`](CONFIGURATION.md#spacing): a blank row before each prompt that follows a command · not released yet.

## Commands and keys

<!-- commands: lokyr/blocks -->

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `blocks.copy-command` | Edit › Copy Command | | Copies the command line of the block under the pointer, from the pane's context menu, or of the last command that started |
| `blocks.copy-block` | Edit › Copy Command and Output | | Copies the command line and the output of the block under the pointer, or of the last command that ended, its wrapped rows joined, 100 000 rows at most |
| `blocks.select-block` | Edit › Select Block | | Selects that block, from its prompt to the end of its output |
| `blocks.select-previous` | View › Select Previous Block | ⌘↑, and ↑ while a block is selected | Selects the block before the selected one, or the last block, or, of a view scrolled up, the block of the prompt above its top |
| `blocks.select-next` | View › Select Next Block | ⌘↓, and ↓ while a block is selected | Selects the block after the selected one, or none past the last, or, of a view scrolled up, the block of the prompt below its top |
| `blocks.deselect` | View › Deselect Block | Escape, while a block is selected | Selects no block |
| `blocks.rerun` | Edit › Run Command Again | | Runs the command line of the block under the pointer, or of the last command that started, at the shell's prompt, while its command line is empty |
| `blocks.details` | Edit › Block Details | | Shows the details of the block under the pointer, or of the last command that started: its command line, exit status, start, duration and folder, each copied by a click |

<!-- commands end -->

The commands of the Edit menu have no default keys: bind them in [`[keys]`](CONFIGURATION.md#keys), such as `"cmd+alt+shift+b" = "blocks.copy-block"`. They act on the selected block when you run them with no click. ⌘↑ and ⌘↓ jump between the prompts again while Blocks is off.

## How it works

Your shell marks each command with OSC 133, the shell integration's sequences: where the prompt starts, where the command line starts, where the output starts, and where the command ended. Lokyr keeps the marks with the rows of the scrollback, and the Blocks plugin reads them as the view moves.

Lokyr draws the dividers, the bars and the tints from the same marks, for the blocks that the pane shows: they cost nothing for the commands kept in the scrollback. It pins the prompt rows of the block at the pane's top from the same marks too, copying them again only when another block reaches the top.

With `spacing`, the blank row comes when your shell marks the start of its prompt after a command that ended: Lokyr moves the cursor down a row, so that the prompt starts a row lower than where the shell began to draw it. Shells draw their prompts from where the cursor is, and move within them from there, so they draw the command line and redraw the prompt in the right place. The blank row is part of no block.

When a command's output starts, Lokyr notes the time and the folder that your shell reported last with OSC 7, which the shell integration sends before each prompt. When the command ends, Lokyr notes the time again. The duration is the time between the two, by the Mac's clock.

The header lies over the pane's first row, and takes none of the grid's rows, so the program's grid keeps its size as the header comes and goes. A scroll over the header scrolls the pane, and the header takes no keys.

Blocks replaces the command header: while Blocks runs, the command header is off, and it comes back when you turn Blocks off. Keys that you left in `[plugins.command-header]` apply only then. While Blocks runs, the settings window shows a note above them, naming Blocks.

## Limits

- Blocks needs your shell to mark its commands, which Lokyr's [shell integration](shell-integration.md) does for zsh, bash and fish.
- A shell on another host, such as one that you reach over ssh, marks no commands, so its output shows no header.
- A program's own screen, such as vim's or less's, shows no header.
- bash, and fish before 4, report no command line, so the pill shows the text of the command's row, a right prompt included.
- The pinned rows are the prompt as your shell drew it when you ran the command: a prompt that the shell draws again as you press Return, such as a transient prompt, pins as it was drawn last. A prompt of more than three rows pins its first three · not released yet.
- The blank rows of `spacing` are rows of the scrollback, which a copy of several blocks includes, and they stay when you turn `spacing` off. A full row is more room than a line between the blocks needs: rows all have the same height. A shell whose own config already prints an empty line before each prompt gets two · not released yet.
- The times are those at which the shell's marks reach Lokyr, so a command's duration also counts the time the shell took to print its marks, a few milliseconds.
- A command's folder is the one where it started. A command that changes folder, such as `cd`, shows the folder that it left.
- Run Command Again and Copy Command take the command line as your shell reported it. bash, and fish before 4, report none, so Lokyr reads it from the block's first row: a command that you typed over several lines, such as a loop, runs again or is copied from its first line alone. Run it again from the shell's history instead, with the Up arrow.
- In zsh, with a plugin that switches the history list as you type, such as Oh My Zsh's per-directory-history, zsh reports the command as it runs it: its aliases expanded, and a loop laid out as zsh writes it.

## Troubleshooting

### The header never shows

**Why**: your shell doesn't mark its commands. Shell integration is off, or the shell is one that Lokyr doesn't set up.

**Fix**: turn shell integration on, or set it up by hand, as [shell integration](shell-integration.md#set-it-up-by-hand) explains.

**Check**: run `seq 1 500` and scroll up. The header with `seq 1 500` appears at the pane's top.

### Block Details shows no folder

**Why**: your shell doesn't report its folder with OSC 7, which Lokyr's shell integration does for zsh, bash and fish.

**Fix**: turn shell integration on, or set it up by hand, as [shell integration](shell-integration.md#set-it-up-by-hand) explains.

**Check**: run `ls`, then right-click its output and choose Block › Block Details. The Folder line shows the folder where you ran it.

### The arrow isn't there

**Why**: the arrow belongs to the pill, which shows only with `header = "pill"`; by default the header is the prompt's pinned rows, which have none. With the pill, the arrow shows only while the end of the block is below the view: when the output's last row is on the pane, you are already at the block's end.

**Fix**: set `header = "pill"` in `[plugins.blocks]`, or scroll up further into the output, and the arrow comes back.

## See also

- [Shell integration](shell-integration.md)
- [The command header](plugins/command-header.md)
- [`[plugins.blocks]` in the configuration guide](CONFIGURATION.md#pluginsblocks)
