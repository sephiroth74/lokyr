# Notification codes

<sub>macOS · for program authors</sub>

A program can ask Lokyr to show a desktop notification, such as when a long build ends, so that you hear of it while you work in another app. This page describes the three codes that Lokyr reads for that, in the notation of [Escape codes](../escape-codes.md#how-the-codes-are-written).

## Try it

Run this command, then switch to another app within three seconds:

```sh
sleep 3; printf '\033]777;notify;Build;Finished in 42 seconds\033\\'
```

A notification shows "Build" and "Finished in 42 seconds". Click it to bring the pane that sent it forward.

## How Lokyr shows them

Each code below ends in the same notification, as macOS shows it:

- **Only while the window is in the background**: a notification from a window that has the focus shows nothing, as you see the program already.
- **One every two seconds at most** for each window: a notification within two seconds of the window's last shows nothing.
- **Its title** is the one that the program sends, or else the window's title. A title past 256 characters, or a body past 2048, shows its start.
- **A click** brings forward the pane that sent it, with its tab and its window.
- **An AI agent's notification** also marks its tab as waiting for you, as [AI agents](../agents.md) describes.

macOS shows them once you allow Lokyr's notifications in System Settings › Notifications.

## OSC 9, a notification

<sub>Platforms: macOS · Since 0.2</sub>

```text
OSC 9 ; body ST
```

| Parameter | Values | Meaning |
|---|---|---|
| `body` | Text | The notification's text |

**What Lokyr does**: it shows a notification of `body`, titled with the window's title. `OSC 9 ; 4` is a [progress](progress-and-status.md#osc-94-progress), not a notification. Any other text after `9 ;` is the body, other terminals' commands that start with a number among them.

**Try it**: run this command, then switch to another app within three seconds:

```sh
sleep 3; printf '\033]9;The tests passed\033\\'
```

A notification shows "The tests passed", titled with the window's title.

## OSC 777, a notification with a title

<sub>Platforms: macOS · Since 0.2</sub>

```text
OSC 777 ; notify ; title ; body ST
```

| Parameter | Values | Meaning |
|---|---|---|
| `title` | Text, without `;` | The notification's title |
| `body` | Text, `;` included | The notification's text |

**What Lokyr does**: it shows a notification of `title` and `body`. An empty `title` takes the window's. Lokyr ignores an OSC 777 other than `notify`.

**Try it**: the command of [Try it](#try-it) above.

## OSC 99, a notification in parts

<sub>Platforms: macOS · not released yet · [Specification](https://sw.kovidgoyal.net/kitty/desktop-notifications/)</sub>

```text
OSC 99 ; metadata ; payload ST
```

The metadata is a list of `key=value` pairs separated by `:`, which can be empty, but the two `;` are required.

| Key | Values | Meaning |
|---|---|---|
| `i` | Letters, digits and `_ - + .`, up to 64 | The notification's id, which ties its parts together |
| `p` | `title`, `body`, `icon`, `buttons`, `?` | What the payload is: `title` by default, and `?` a query of what Lokyr supports |
| `e` | `0`, `1` | `1` when the payload is in base64 |
| `d` | `0`, `1` | `0` when more parts of the notification follow; `1`, the default, shows it |

**What Lokyr does**: it gathers the title's and the body's parts of each `i`, and shows the notification when a part of `d=1` comes, its body alone when it has no title. It takes no icon, buttons or actions: their parts count only for `d`. It ignores `p=close` and `p=alive`, and a part of a `p` that it doesn't know.

**Lokyr answers** a query, `p=?`, with what it supports, ended as the query was: a click brings the pane forward, notifications show while the window is in the background, and they hold a title and a body.

```text
OSC 99 ; i=id : p=? ; a=focus : o=unfocused : p=title,body ST
```

**Bounds**: a part of more than 4096 bytes is ignored. A title gathers up to 1024 bytes and a body up to 4096. Lokyr holds the parts of 16 notifications at most; past them, the oldest goes.

**Try it**: a title and a body in two parts. Run this command, then switch to another app within three seconds:

```sh
sleep 3; printf '\033]99;i=1:d=0;Build\033\\'; printf '\033]99;i=1:p=body;Finished in 42 seconds\033\\'
```

A notification shows "Build" and "Finished in 42 seconds".

Ask Lokyr what it supports:

```sh
printf '\033]99;i=q:p=?;\033\\'; IFS= read -rs -t 1 -d '\' reply; reply=${reply#*]}; printf '%s\n' "${reply%$'\033'}"
```

You should see:

```text
99;i=q:p=?;a=focus:o=unfocused:p=title,body
```

## See also

- [Escape codes](../escape-codes.md), every code that Lokyr reads
- [Progress and status codes](progress-and-status.md)
- [Shell integration](../shell-integration.md), and [`[notifications] long-command`](../CONFIGURATION.md#long-command) for a notification when a long command ends
