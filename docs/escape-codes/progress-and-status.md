# Progress and status codes

<sub>macOS · for program authors</sub>

A program can report how far along its work is, and whether it works, waits for you or is done, so that its tab shows it while you look elsewhere. This page describes the three codes that Lokyr reads for that, in the notation of [Escape codes](../escape-codes.md#how-the-codes-are-written).

## Try it

Run this command:

```sh
for p in 20 40 60 80 100; do printf '\033]9;4;1;%d\033\\' "$p"; sleep 0.5; done; printf '\033]9;4;0\033\\'
```

A thin bar across the window's top fills to the end in two and a half seconds, then goes. With two tabs or more, the tab's outline fills too.

## OSC 9;4, progress

<sub>Platforms: macOS · Since 0.5.1 · [Specification](https://learn.microsoft.com/en-us/windows/terminal/tutorials/progress-bar-sequences)</sub>

```text
OSC 9 ; 4 ; state ; value ST
```

| Parameter | Values | Meaning |
|---|---|---|
| `state` | `0` remove, `1` normal, `2` error, `3` indeterminate, `4` paused | What the task is doing |
| `value` | `0` to `100` | How far along it is, in percent. Lokyr reads a larger number of up to nine digits as `100`, a longer one as no value, and ignores it for `0` and `3` |

**What Lokyr does**: it shows the progress round the outline of the program's tab, in the [`progress`](../plugins/progress.md) bar across the window's top, and in the ring of a single tab's title. A task that failed shows red and a paused one yellow, full when it reports no percentage. An indeterminate task shows a part that sweeps across. A `state` other than these is ignored.

**Bounds**: the progress stays until the program sends `0`. A program that ends without sending it, such as one stopped with ⌃C, leaves its progress until another program sends `0`.

**Try it**: a task that fails at 60%, which clears after two seconds:

```sh
printf '\033]9;4;2;60\033\\'; sleep 2; printf '\033]9;4;0\033\\'
```

The bar fills to 60% in red, then goes.

**See**: [the `progress` plugin](../plugins/progress.md), [the `tabs` plugin](../plugins/tabs.md), and [Progress and status](../progress-and-status.md), which shows how to get the progress of programs that send it only to terminals that they recognize.

## OSC 7501, a program's status

<sub>Platforms: macOS · not released yet · [Specification](https://control-codes.page/proposals/osc-7501/), revision 0.4</sub>

```text
OSC 7501 ; key=value : key=value … ST
OSC 7501 ; ? ST
```

A report is a list of `key=value` pairs separated by `:`.

| Key | Values | Meaning |
|---|---|---|
| `state` | `idle`, `working`, `blocked`, `done`, `error`, `clear` | Where the program is. `blocked` waits for you; `clear` takes the task away, and the tasks under it |
| `id` | Up to 8 segments separated by `/`, such as `main/tests` | The task, in a tree of the program's tasks. Without it, the report is about the program itself |
| `kind` | `permission`, `question`, `auth` | What a `blocked` program waits for |
| `progress` | `0` to `100` | How far along a `working` or `blocked` task is |
| `app` | Letters, digits and `_ . + -`, such as `claude-code` | The program's name, which a task without one takes from the nearest task above it |
| `title` | Base64 of UTF-8 text | A short label of the task |
| `msg` | Base64 of UTF-8 text | What the task does, asks or ended with, such as "Allow Bash: npm test?" |

A value other than `title` and `msg` holds letters, digits and `_ . , + / = -` only. Lokyr ignores a key that it doesn't know, a pair without `=`, and every value of a repeated key but its last.

**Lokyr answers** `OSC 7501 ; ?` with the states and kinds that it takes, ended as the query was:

```text
OSC 7501 ; ? : states=idle,working,done,blocked,error : kinds=permission,question,auth ST
```

**What Lokyr does**: the program becomes an AI agent of its tab, whether [`[agents] programs`](../CONFIGURATION.md#programs) lists it or not, and its tab shows the most urgent of its tasks, as [AI agents](../agents.md#programs-that-tell-their-status) describes: `working` at work, with its progress in the tab's ring; `blocked` waiting for you, its message in the tab's tooltip and in a notification while its window is in the background; `done` and `error` until you look at the tab or type into its pane; `idle` no mark. What the program reported of its work, its waiting and its idle goes when it leaves the foreground or the shell's next prompt starts. An empty `[agents] programs` turns this off.

**Bounds**: a report of more than 4096 bytes, of a `state` that Lokyr doesn't know, or of a value past its bound is ignored whole: a key of 16 bytes, an `app` of 32, an `id` of 128 and its segments of 32, a `title` of 192 bytes of text, 256 in base64, and a `msg` of 2048, 2732 in base64. Lokyr keeps 256 tasks; past them, the one updated longest ago goes.

**Try it**: open a second tab, then come back to this one and run this command:

```sh
( printf '\033]7501;state=working:progress=40:app=demo\033\\'; sleep 4; printf '\033]7501;state=done:app=demo\033\\'; sleep 1 )
```

Within four seconds, switch to the other tab: the first tab shows its agent at work and 40% in its ring, then that it's done, which it shows until you go back to it.

Ask Lokyr what it takes:

```sh
printf '\033]7501;?\033\\'; IFS= read -rs -t 1 -d '\' reply; reply=${reply#*]}; printf '%s\n' "${reply%$'\033'}"
```

You should see:

```text
7501;?:states=idle,working,done,blocked,error:kinds=permission,question,auth
```

**See**: [AI agents](../agents.md#programs-that-tell-their-status), and [`[agents]`](../CONFIGURATION.md#agents) in the configuration guide.

## OSC 21337, a session's status

<sub>Platforms: macOS · not released yet · [Specification](https://iterm2.com/documentation-session-status.html)</sub>

```text
OSC 21337 ; key=value ; key=value … ST
```

| Key | Values | Meaning |
|---|---|---|
| `status` | Text, up to 256 bytes | A short status, such as `Working` |
| `indicator` | A color, `#rrggbb` or `rgb:rr/gg/bb` | The color of the dot that marks the tab |
| `status-color` | A color, as `indicator` | The dot's color where `indicator` gives none |
| `detail` | Text, up to 1024 bytes | A line of what the program does |

Write a `;` in a value as `\;` and a backslash as `\\`. A key that a report leaves out keeps its value; an empty value clears it.

**What Lokyr does**: the program's tab shows a dot of the `indicator`'s color, unless it shows an AI agent's mark, and the status and its detail in its tooltip. It all goes when the program exits. Lokyr ignores a key that it doesn't know, and a value that doesn't read or is past its bound.

**Try it**: with two tabs or more, run this command:

```sh
( printf '\033]21337;status=Working;indicator=#00a0ff;detail=Compiling the docs\033\\'; sleep 5 )
```

For five seconds, the tab shows a blue dot, and resting the pointer on the tab shows "Working" and "Compiling the docs". Then the dot goes.

**See**: [the `tabs` plugin](../plugins/tabs.md).

## See also

- [Escape codes](../escape-codes.md), every code that Lokyr reads
- [Notification codes](notifications.md)
- [AI agents](../agents.md)
