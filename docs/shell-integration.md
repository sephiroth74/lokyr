# Shell integration

<sub>macOS · zsh, bash, fish · on by default · [`[terminal] shell-integration`](CONFIGURATION.md#shell-integration)</sub>

Shell integration lets your shell report to Lokyr where each prompt starts, where your command starts and ends, whether it succeeded, and the folder it works in. With it, you can jump between your commands, see at a glance which ones failed, and copy a command's output. Lokyr turns it on for zsh, bash and fish without changing your files.

## Try it

Run this command:

```sh
ls /nonexistent
```

Then this one:

```sh
seq 1 200
```

A red bar appears at the left of the prompt of `ls`, which failed, and a gray one at the left of the prompt of `seq`, each on its prompt's first line of text. While you read the numbers, with the line `seq 1 200` scrolled away, the command shows in a pill at the pane's top. Press ⌘↑: the view jumps up to the prompt of `seq`. Press ⌘↑ again to reach the prompt of `ls`, and ⌘↓ to come back down. With [Blocks](blocks.md#select-a-block-with-the-keyboard), ⌘↑ selects the block of `seq` too, and ↑ and ↓ move from block to block · not released yet.

## What it gives you

- **Jump between prompts**: ⌘↑ and ⌘↓ (`pane.prompt-previous`, `pane.prompt-next`), as [copy, paste and scrolling](plugins/core-actions.md) lists them. While Blocks runs, they select the [block](blocks.md#select-a-block-with-the-keyboard) of the prompt that they jump to · not released yet.
- **A mark for each command**: a bar in the gutter at the left of its prompt's first line of text, gray when it succeeded, red when it failed, and in the accent color while it runs. With [Blocks](blocks.md#dividers-and-the-gutters-bar), the bar runs along the whole block, and a thin line divides the blocks · not released yet. [`[window] gutter`](CONFIGURATION.md#gutter) turns the gutter off.
- **The command you're reading**: when a command's line scrolls away, the header of its [block](blocks.md) shows it at the pane's top, with an arrow to the end of its output · not released yet.
- **Copy a command's output**: ⌥⇧⌘C (`pane.copy-output`) copies the output of the last command that ended, without its prompt; ⇧⌘A (`pane.select-output`) selects it. Right-click a [block](blocks.md#copy-a-command-or-its-output) to copy its command line too, or to select it whole · not released yet.
- **A command's details**: right-click a [block](blocks.md#see-a-blocks-details) to see its exit status, when it started, how long it ran and the folder where it ran · not released yet.
- **Run a command again**: right-click a [block](blocks.md#run-a-command-again) to run its command line again at the prompt · not released yet.
- **Hear when a long command ends**: a notification lets you know when a command that ran for 10 seconds or more ends while its window is in the background. [`[notifications] long-command`](CONFIGURATION.md#long-command) sets how long.
- **New tabs in the same folder**: a new tab or a split starts in the folder that its pane's shell reported last, as [`[window] new-directory`](CONFIGURATION.md#new-directory) says.
- **Click in the command line** to move the shell's cursor there, like in a text field. Select part of the command line and press Delete or Forward Delete to remove it, or type to replace it. Both work while the shell is the program in front of the pane, in zsh, in bash, in fish 3 and in fish 4.1 or later. [`[cursor] click-to-move`](CONFIGURATION.md#click-to-move) turns both off. · Since 1.3
- **Tabs that know what runs**: when a command starts, its tab's title and marks follow the program that it runs. Lokyr looks for the program from the command's start until 4 seconds later, and again when the program prints something.

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
| bash | Starts bash in POSIX mode, which reads only `ENV`. The integration leaves POSIX mode, then reads your startup files itself, as a login bash does: `/etc/profile`, then the first of `.bash_profile`, `.bash_login` and `.profile` |
| fish | Adds its folder to `XDG_DATA_DIRS`, whose `vendor_conf.d` fish reads, and takes it away again as fish starts. fish 4 and later report their prompts themselves |

A shell that reports its prompts on its own, such as Nushell, works too, with nothing to set up.

## Set it up by hand

macOS's own bash, `/bin/bash`, is version 3.2: it reads your files but not `ENV` in POSIX mode, so it gets no integration. A later bash, such as Homebrew's, gets it. To add it to `/bin/bash`, which starts as a login shell in a new pane and reads `~/.bash_profile`, add this line to `~/.bash_profile`:

```sh
[ -n "$LOKYR_RESOURCES_DIR" ] && source "$LOKYR_RESOURCES_DIR/shell-integration/bash/lokyr.bash"
```

A shell that you start from another one, such as `bash` typed in a zsh pane, gets no integration either. To have it there too, source your shell's script from its startup file. For zsh, in `~/.zshrc`:

```sh
[ -n "$LOKYR_RESOURCES_DIR" ] && source "$LOKYR_RESOURCES_DIR/shell-integration/zsh/lokyr-integration"
```

For bash, the line above, in `~/.bashrc`, which a bash that you start from another shell reads. For fish 3, in `~/.config/fish/config.fish`; fish 4 and later need nothing:

```fish
set -q LOKYR_RESOURCES_DIR; and source "$LOKYR_RESOURCES_DIR/shell-integration/fish/vendor_conf.d/lokyr-shell-integration.fish"
```

Loading a script twice does no harm, so a shell that Lokyr already set up can keep the line.

## How it works

At each prompt and around each command, your shell prints escape codes that Lokyr reads and never shows:

- **OSC 133**, [semantic prompts](https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md): `A` where the prompt starts, `B` where your input starts, `C` where the command's output starts, and `D` with the exit status where it ends. zsh and fish 4 add the command line as you typed it to `C`, so that the header and the notification show it without a right prompt's text.
- **OSC 7**: the shell's folder, as a `file` URL of this Mac.

In bash 4.4 and later, the output's mark comes from `PS0`, so the integration sets no trap: your own DEBUG trap, `$_` and the commands that your keys run through `bind -x`, such as fzf's, stay as they are. An older bash marks the output from a DEBUG trap, unless you have one of your own.

At each prompt, after your own prompt commands run and before the shell draws the prompt, the integration puts its marks around your prompt, so a prompt theme that sets `PS1` keeps working.

## Limits

- **macOS's `/bin/bash`** gets no integration unless you add it by hand.
- **Remote shells** over `ssh` get no integration: their commands get no marks.
- **A bash older than 4.4 with a DEBUG trap of its own** gets no output marks.
- **A click in the command line** moves nothing in a remote shell over `ssh`, as `ssh` is the program in front, nor in a bash older than 4.4 with a DEBUG trap of its own. A program that a shell runs, such as `cat` of a file or a REPL, gets no keys from a click or a selection, whatever prompts' marks its output holds: Lokyr sends them to a shell in front alone. · Since 1.3
- **fish 4.0** reports its prompts without saying that it takes clicks, so a click in its command line moves nothing. fish 3, with the integration, and fish 4.1 and later take them. · Since 1.3
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

**Check**: in zsh or bash, `echo "${_lokyr_integration-none}"` prints `none` without the integration, and `1` once you source it. In fish 3, `functions -q __lokyr_prompt_start; and echo 1` prints `1` once you source it.

### A click in the command line doesn't move the cursor

<sub>Platforms: macOS · Since 1.3</sub>

**Why**: [`[cursor] click-to-move`](CONFIGURATION.md#click-to-move) is `false`, the shell has no integration, the program in front is not a shell, such as `ssh`, the shell is fish 4.0, or a bash older than 4.4 has a DEBUG trap of its own.

**Fix**: set `click-to-move` to `true`, or use bash 4.4 or later, such as Homebrew's, or fish 4.1 or later.

**Check**: type `echo hello world`, click on the `w`, then type `X`: the line reads `echo hello Xworld`.

### New tabs open in your home folder

**Why**: the shell doesn't report its folder, because it has no integration, or the folder belongs to another machine, such as over `ssh`.

**Fix**: as for [commands that get no marks](#commands-get-no-marks).

## For shell authors

To have Lokyr understand your shell or your prompt, print these codes. Each ends with the string terminator, `\033\\`; the bell character, `\a`, works too.

At the prompt's start:

```sh
printf '\033]133;A\033\\'
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

The shell's folder, as a `file` URL, its path percent-encoded but for its letters, digits and `/._~-`. In bash:

```bash
lokyr_report_folder() {
    local LC_ALL=C url= byte i
    for ((i = 0; i < ${#PWD}; i++)); do
        byte=${PWD:i:1}
        case $byte in
            [A-Za-z0-9/._~-]) url+=$byte ;;
            *) printf -v byte '%%%02X' "'$byte"; url+=$byte ;;
        esac
    done
    printf '\033]7;file://%s%s\033\\' "$HOSTNAME" "$url"
}
```

Other options of `A`, and the right prompt:

- **`cl=line`** for a line editor that moves its cursor with the left and right arrows within the line, so that a click in the command line moves the cursor. · Since 1.3
- **`cl=m`** for a line editor that moves across the lines of a command of several lines with the arrows; Lokyr then counts each line's end as one arrow. · Since 1.3
- **`click_events=1`** hands Lokyr's clicks in the command line to the shell itself, in the mouse's SGR encoding, at their row on the screen; `click_events=2` counts the row from the row of the prompt's `A`, as the [click events' specification](https://sw.kovidgoyal.net/kitty/shell-integration/#notes-for-shell-developers) describes. · Since 1.3
- **A right prompt**, such as zsh's `RPROMPT`, starts with `OSC 133 ; P ; k=r` and ends with `B`, so that its text is no part of the command line that a click counts. · Since 1.3

## See also

- [Blocks](blocks.md)
- [The command header](plugins/command-header.md)
- [AI agents](agents.md), where a program's reported state ends when the shell's next prompt starts
- [The configuration guide](CONFIGURATION.md): [`shell-integration`](CONFIGURATION.md#shell-integration), [`click-to-move`](CONFIGURATION.md#click-to-move), [`gutter`](CONFIGURATION.md#gutter), [`long-command`](CONFIGURATION.md#long-command), [`new-directory`](CONFIGURATION.md#new-directory)
