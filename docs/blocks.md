# Blocks

<sub>macOS · zsh, bash, fish · on by default · the `lokyr/blocks` plugin · [`[plugins.blocks]`](CONFIGURATION.md#pluginsblocks) · not released yet</sub>

Lokyr sees each command that your shell runs as a block: its prompt, the command line you typed, and the output that the command printed. When you scroll up through a long output, the block's command line stays at the top of the pane, so that you always know what you are reading. One click takes you to the start of the block, and another to its end.

## Try it

Run this command:

```sh
seq 1 500
```

Scroll up into the numbers. A header with `seq 1 500` appears across the top of the pane, with an arrow beside it. Click the arrow: the view moves to the end of the output, with 500 on the pane's last row. Then click `seq 1 500`: the view moves up to the command's prompt, and the header goes.

## What it gives you

- **The command you are reading**: while the pane's first row shows a command's output, and the command line has scrolled above the view, a pill with the command line sits on a thin line across the pane's top.
- **The block's start**: a click on the pill scrolls the command's prompt to the top of the pane.
- **The block's end**: while the end of the output is below the view, an arrow beside the pill scrolls to it, with the output's last row at the bottom of the pane. For a command that is still running, the arrow takes you to the bottom of the screen, which the view then follows as the output grows.
- **The command as you typed it**: zsh, and fish 4 or later, report the command line itself, so a right prompt on its row, such as zsh's `RPROMPT`, stays out of the pill.
- **Long lines stay short**: a command line longer than 80 characters is cut with an ellipsis, `…`. The pill's tooltip, and what VoiceOver reads, keep the whole line.

## Change the header's look

The header's colors are tokens of your theme, so they follow the light and the dark appearance. Its keys in [`[plugins.blocks]`](CONFIGURATION.md#pluginsblocks) name the tokens, and set the text's size and weight, the room around the text, and where the pill sits:

```toml
[plugins.blocks]
border = "accent"      # the theme's accent color around the pill
font-weight = "semibold"
align = "center"       # left | center | right
max-length = 60        # characters of the command line before the ellipsis
```

The changes apply as you save the file, to a header that shows too.

## Turn blocks off

```toml
[plugins.blocks]
enabled = false
```

You can also turn off Blocks on the Plugins page of the settings window. Lokyr then shows the plain [command header](plugins/command-header.md): the same pill, without the arrow, whose keys are in [`[plugins.command-header]`](CONFIGURATION.md#pluginscommand-header).

## Settings

- [`background`, `text`, `border`, `line` and `hover`](CONFIGURATION.md#background-text-border-line-hover-1): the theme's tokens of the pill's background, its text, its border, the line across the pane, and the color under the pointer.
- [`font-size`](CONFIGURATION.md#font-size-2) and [`font-weight`](CONFIGURATION.md#font-weight-1): the text's size and weight, in the terminal's font.
- [`padding-x` and `padding-y`](CONFIGURATION.md#padding-x-padding-y-2): the room around the text in the pill.
- [`align`](CONFIGURATION.md#align-1): where the pill sits along the line.
- [`max-length`](CONFIGURATION.md#max-length-1): the most characters of the command line that the pill shows.

## How it works

Your shell marks each command with OSC 133, the shell integration's sequences: where the prompt starts, where the command line starts, where the output starts, and where the command ended. Lokyr keeps the marks with the rows of the scrollback, and the Blocks plugin reads them as the view moves.

The header lies over the pane's first row, and takes none of the grid's rows, so the program's grid keeps its size as the header comes and goes. A scroll over the header scrolls the pane, and the header takes no keys.

Blocks replaces the command header: while Blocks runs, the command header is off, and it comes back when you turn Blocks off. Keys that you left in `[plugins.command-header]` apply only then. While Blocks runs, a notice reports them, and the settings window shows the same note above them.

## Limits

- Blocks needs your shell to mark its commands, which Lokyr's [shell integration](shell-integration.md) does for zsh, bash and fish.
- A shell on another host, such as one that you reach over ssh, marks no commands, so its output shows no header.
- A program's own screen, such as vim's or less's, shows no header.
- bash, and fish before 4, report no command line, so the pill shows the text of the command's row, a right prompt included.

## Troubleshooting

### The header never shows

**Why**: your shell doesn't mark its commands. Shell integration is off, or the shell is one that Lokyr doesn't set up.

**Fix**: turn shell integration on, or set it up by hand, as [shell integration](shell-integration.md#set-it-up-by-hand) explains.

**Check**: run `seq 1 500` and scroll up. The header with `seq 1 500` appears at the pane's top.

### The arrow isn't there

**Why**: the arrow shows only while the end of the block is below the view. When the output's last row is on the pane, you are already at the block's end.

**Fix**: scroll up further into the output, and the arrow comes back.

## See also

- [Shell integration](shell-integration.md)
- [The command header](plugins/command-header.md)
- [`[plugins.blocks]` in the configuration guide](CONFIGURATION.md#pluginsblocks)
