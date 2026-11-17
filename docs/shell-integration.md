# Shell integration

<sub>macOS · zsh, bash, fish · on by default · [`[terminal] shell-integration`](CONFIGURATION.md#shell-integration)</sub>

Shell integration lets your shell report to Lokyr where each prompt starts, where your command starts and ends, whether it succeeded, and the folder it works in. With it, you can jump between your commands, see at a glance which ones failed, copy a command's output, and click in the command line to move the cursor. Lokyr turns it on for zsh, bash and fish without changing your files.

## Try it

Run this command:

```sh
ls /nonexistent
```

Then this one:

```sh
seq 1 200
```

A red bar appears at the left of the prompt of `ls`, which failed, and a gray one at the left of the prompt of `seq`, each on its prompt's first line of text. While you read the numbers, with the line `seq 1 200` scrolled away, the command shows in a pill at the pane's top. Press ⌘↑: the view jumps up to the prompt of `seq`. Press ⌘↑ again to reach the prompt of `ls`, and ⌘↓ to come back down.

## What it gives you

- **Jump between prompts**: ⌘↑ and ⌘↓ (`pane.prompt-previous`, `pane.prompt-next`), as [copy, paste and scrolling](plugins/core-actions.md) lists them.
- **A mark for each command**: a bar in the gutter at the left of its prompt's first line of text, gray when it succeeded, red when it failed, and in the accent color while it runs. [`[window] gutter`](CONFIGURATION.md#gutter) turns the gutter off.
- **The command you're reading**: when a command's line scrolls away, the [command header](plugins/command-header.md) shows it at the pane's top.
- **Copy a command's output**: ⌥⇧⌘C (`pane.copy-output`) copies the output of the last command that ended, without its prompt; ⇧⌘A (`pane.select-output`) selects it.
- **Hear when a long command ends**: a notification tells you when a command that ran for 10 seconds or more ends while its window is in the background. [`[notifications] long-command`](CONFIGURATION.md#long-command) sets how long.
- **New tabs in the same folder**: a new tab or a split starts in the folder that its pane's shell reported last, as [`[window] new-directory`](CONFIGURATION.md#new-directory) says.
- **Click in the command line** to move the shell's cursor there, like in a text field. Select part of the command line and press Backspace or Delete to remove it, or type to replace it. [`[cursor] click-to-move`](CONFIGURATION.md#click-to-move) turns both off. · not released yet
- **Tabs that know what runs**: when a command starts, its tab's title and marks follow the program that it runs, however late it starts.

## Turn it off

```toml
[terminal]
shell-integration = false
```

The change applies to the shells that start after you save the file. The shells that are already open keep what they started with.

## Which shells get it

Lokyr loads the integration into your login shell, the shell that every new pane starts.

| Shell | What Lokyr does |
|---|---|
| zsh | Loads it through `ZDOTDIR`, which it sets back to yours at once, so that your `.zshenv`, `.zprofile`, `.zshrc` and `.zlogin` load as usual |
| bash | Starts bash in POSIX mode, which reads only `ENV`. The integration leaves POSIX mode, then reads your `.bash_profile` or `.bashrc` itself |
| fish | Adds its folder to `XDG_DATA_DIRS`, whose `vendor_conf.d` fish reads, and takes it away again as fish starts. fish 4 and later report their prompts themselves |

A shell that reports its prompts on its own, such as Nushell, works too, with nothing to set up.

## Set it up by hand

macOS's own bash, `/bin/bash`, is version 3.2: it reads your files but not `ENV` in POSIX mode, so it gets no integration. A later bash, such as Homebrew's, gets it. To add it to `/bin/bash`, add this line to your `.bashrc`:

```sh
[ -n "$LOKYR_RESOURCES_DIR" ] && source "$LOKYR_RESOURCES_DIR/shell-integration/bash/lokyr.bash"
```

A shell that you start from another one, such as `bash` typed in a zsh pane, gets no integration either. To have it there too, source your shell's script from its startup file. For zsh, in `~/.zshrc`:

```sh
[ -n "$LOKYR_RESOURCES_DIR" ] && source "$LOKYR_RESOURCES_DIR/shell-integration/zsh/lokyr-integration"
```

For bash, the line above, in `~/.bashrc`. For fish 3, in `~/.config/fish/config.fish`; fish 4 and later need nothing:

```fish
set -q LOKYR_RESOURCES_DIR; and source "$LOKYR_RESOURCES_DIR/shell-integration/fish/vendor_conf.d/lokyr-shell-integration.fish"
```

Loading a script twice does no harm, so a shell that Lokyr already set up can keep the line.

## How it works

At each prompt and around each command, your shell prints escape codes that Lokyr reads and never shows:

- **OSC 133**, [semantic prompts](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md): `A` where the prompt starts, `B` where your input starts, `C` where the command's output starts, and `D` with the exit status where it ends. zsh adds the command line as you typed it to `C`, so that the header and the notification show it without a right prompt's text.
- **OSC 7**: the shell's folder, as a `file` URL of this Mac.

In bash 4.4 and later, the output's mark comes from `PS0`, so the integration sets no trap: your own DEBUG trap, `$_` and the commands that your keys run through `bind -x`, such as fzf's, stay as they are. An older bash marks the output from a DEBUG trap, unless you have one of your own.

The integration adds its marks after your prompt is drawn, at each prompt, so a prompt theme that sets `PS1` keeps working.

## Limits

- **macOS's `/bin/bash`** gets no integration unless you add it by hand.
- **Remote shells** over `ssh` get no integration: their commands get no marks.
- **A bash older than 4.4 with a DEBUG trap of its own** gets no output marks, and a click in its command line moves nothing.
- **A program that takes the whole screen**, such as an editor, shows no prompts and gets no marks while it runs.

## Troubleshooting

### Commands get no marks

**Why**: the shell isn't one that Lokyr loaded the integration into, such as `/bin/bash` or a shell started from another, or the integration is off.

**Fix**: check that [`shell-integration`](CONFIGURATION.md#shell-integration) is `true`, open a new tab, or add the integration by hand as [above](#set-it-up-by-hand).

**Check**: in zsh or bash, run:

```sh
echo "${_lokyr_integration-none}"
```

You should see `1`. In fish, `echo $version` should print 4 or later, or `functions -q __lokyr_prompt_start; and echo 1` should print `1`.

### ⌘↑ jumps past your last commands to an older prompt

**Why**: the commands ran in a shell that you started from another one, such as `bash` typed in a zsh pane, which has no integration: to Lokyr, its whole session is the output of the command that started it, so ⌘↑ goes to that command's prompt.

**Fix**: source your shell's script in that shell, as [above](#set-it-up-by-hand), or make it your login shell.

**Check**: in that shell, `echo "${_lokyr_integration-none}"` prints `none` without the integration, and `1` once you source it.

### A click in the command line doesn't move the cursor

**Why**: [`[cursor] click-to-move`](CONFIGURATION.md#click-to-move) is `false`, the shell has no integration, or a bash older than 4.4 has a DEBUG trap of its own.

**Fix**: set `click-to-move` to `true`, or use a bash of version 4.4 or later, such as Homebrew's.

**Check**: type `echo hello world`, click on the `w`, then type `X`: the line reads `echo hello Xworld`.

### New tabs open in your home folder

**Why**: the shell doesn't report its folder, because it has no integration, or the folder belongs to another machine, such as over `ssh`.

**Fix**: as for [commands that get no marks](#commands-get-no-marks).

## For shell authors

To have Lokyr understand your shell or your prompt, print these codes. Each ends with the string terminator, `\033\\`; the bell character, `\a`, works too.

At the prompt's start, `cl=line` if your line editor moves its cursor with the left and right arrows within the line:

```sh
printf '\033]133;A;cl=line\033\\'
```

Where the input starts, after the prompt:

```sh
printf '\033]133;B\033\\'
```

As the command starts, with the command line percent-encoded in `cmdline_url` if you have it:

```sh
printf '\033]133;C;cmdline_url=ls%%20-l\033\\'
```

As the command ends, with its exit status:

```sh
printf '\033]133;D;%s\033\\' "$?"
```

The shell's folder, as a `file` URL, percent-encoded:

```sh
printf '\033]7;file://%s%s\033\\' "$(hostname)" "$PWD"
```

Other options of `A`:

- **`cl=m`** for a line editor that moves across the lines of a command of several lines with the arrows; Lokyr then counts each line's end as one arrow. · not released yet
- **`click_events=1`** hands Lokyr's clicks in the command line to the shell itself, in the mouse's SGR encoding, at their row on the screen; `click_events=2` counts the row from the prompt's first, as the [click events' specification](https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers) describes. · not released yet

A right prompt, such as zsh's `RPROMPT`, starts with `OSC 133 ; P ; k=r` and ends with `B`, so that its text is no part of the command line that a click counts.

## See also

- [The command header](plugins/command-header.md)
- [AI agents](agents.md), where a program's reported state ends when the shell's next prompt starts
- [The configuration guide](CONFIGURATION.md): [`shell-integration`](CONFIGURATION.md#shell-integration), [`click-to-move`](CONFIGURATION.md#click-to-move), [`gutter`](CONFIGURATION.md#gutter), [`long-command`](CONFIGURATION.md#long-command), [`new-directory`](CONFIGURATION.md#new-directory)
