# Progress and status

<sub>macOS · on by default · the [`tabs`](plugins/tabs.md) and [`progress`](plugins/progress.md) plugins</sub>

Programs such as Gradle, cargo and AI agents can report how far along their work is, and whether they work, wait for you or are done. Lokyr shows it on the program's tab and across the window's top, so that you can follow a build in one tab while you work in another.

## Try it

Run this command:

```sh
for p in 20 40 60 80 100; do printf '\033]9;4;1;%d\033\\' "$p"; sleep 0.5; done; printf '\033]9;4;0\033\\'
```

A thin bar across the window's top fills to the end in two and a half seconds, then goes. Open a second tab with ⌘T (`cmd+t`) and run it again: the tab's outline fills too.

## What it gives you

- **A ring round the tab**: the tab's outline fills as the progress grows, in the accent color, red for a task that failed and yellow for a paused one. A task of no known end shows a part that sweeps round.
- **A bar across the window's top**: the [`progress`](plugins/progress.md) plugin shows the progress of the tab that the window shows, over the panes, so that the program's grid keeps its size.
- **A single tab's title**: while the window has one tab, which hides the tab bar, a ring beside the title in the titlebar shows the progress.
- **A spinner** on the tab of a program that prints on its own, even when it reports no progress.
- **An AI agent's state**: an agent at work, waiting for you, done or failed, which [AI agents](agents.md) describes, and its progress in the ring when it reports one.
- **A session's status**: a dot in the color that a program gives its session's status, and the status's text in the tab's tooltip. · not released yet

## Get the progress of Gradle, Claude Code and cargo

Some programs send their progress only to the terminals that they recognize by their environment's variables, and don't recognize Lokyr yet. Until they do, turn their progress on:

| Program | To get its progress in Lokyr |
|---|---|
| Gradle 9.8 | `export ConEmuPID=1` in your shell's profile, as Gradle has no setting of its own |
| Claude Code 2.1 | `export ConEmuPID=1` in your shell's profile, while its `terminalProgressBarEnabled` setting is on, as it is by default |
| cargo 1.99 | `term.progress.term-integration = true` in `~/.cargo/config.toml`, or `CARGO_TERM_PROGRESS_TERM_INTEGRATION=true` in the environment |

Programs that read `ConEmuPID` take it for a terminal of Windows. That's rarely a problem on macOS, but if a program behaves oddly, remove the variable first. Homebrew, npm, pip, uv, Go and Maven send no progress, in any terminal. Checked in November 2026: Gradle 9.8.2, Claude Code 2.1.318 and cargo 1.99.1.

## Turn the bar off

```toml
[plugins.progress]
enabled = false
```

The bar goes from every window. The tabs still show the progress round their outlines.

## Settings

- [`[plugins.progress] enabled`](plugins/progress.md#turning-it-off) turns the bar across the window's top off.
- [`[agents] programs`](CONFIGURATION.md#programs) lists the AI agents' programs; an empty list turns the agents' marks off.
- With an empty `programs`, the statuses that programs report don't show either. · not released yet
- [`[plugins.tabs] hide-when-single`](CONFIGURATION.md#hide-when-single) chooses whether a single tab hides the bar, its progress then in the titlebar.

## How it works

A program prints escape codes that Lokyr reads and never shows: OSC 9;4 for its progress. [Progress and status codes](escape-codes/progress-and-status.md) describes each, with what to print to send it.

A program can also report its status with OSC 7501, and its session's status with OSC 21337. · not released yet

A program that changes its progress many times a second costs Lokyr no frames: the window server moves the bar and the ring.

## Limits

- **A progress stays until the program clears it.** A program stopped with ⌃C, or one that crashes, leaves its progress on its tab until a program sends the code that clears it.
- **Programs that send no progress**, such as Homebrew and npm, show only a spinner while they print.

## Troubleshooting

### A program's progress doesn't show

**Why**: the program sends its progress only to terminals that it recognizes, or sends none.

**Fix**: turn it on as [the table above](#get-the-progress-of-gradle-claude-code-and-cargo) shows, then open a new tab, whose shell reads your profile again.

**Check**: run the command of [Try it](#try-it). If the bar fills, Lokyr shows progress, and the program sends none.

### The ring stays after the program ended

**Why**: the program ended without clearing its progress, such as when you stopped it with ⌃C.

**Fix**: clear it by hand:

```sh
printf '\033]9;4;0\033\\'
```

**Check**: the ring and the bar go.

## See also

- [The `progress` plugin](plugins/progress.md) and [the `tabs` plugin](plugins/tabs.md)
- [AI agents](agents.md)
- [Progress and status codes](escape-codes/progress-and-status.md)
- [Notifications](notifications.md)
