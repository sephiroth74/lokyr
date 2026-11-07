# `progress`: the programs' progress

<sub>`lokyr/progress` · bundled · [source](../../plugins/progress)</sub>

The progress that a program tells, as Gradle's, in a thin bar across the window's top, under the titlebar or the tab bar: of the tab that the window shows. The [`tabs`](tabs.md) plugin shows it round the tab's outline too.

## The bar

- **It fills** from the left as far as the percentage, in the accent's color; a failed task's is red, and a paused one's yellow, full where the program tells no percentage.
- **A task of no known end** has a part that sweeps across.
- **It lies over the panes**, taking none of their rows, so that the program's grid keeps its size as the bar comes and goes.
- **It slides** to a new percentage, and sweeps, as the window server moves it, so that a program that tells many percentages a second costs Lokyr no frames.
- **It goes** as the program ends its task, and changes as another tab shows.

The programs tell it with OSC 9;4. Some, as Gradle, cargo and Claude Code, send it only to the terminals that they know: the README's [Programs' progress](../../README.md#programs-progress) tells how to turn each on.

## Turning it off

```toml
[plugins.progress]
enabled = false
```

The bar goes from every window; the tabs still show the progress round their outlines.

## Permissions

It reads the tabs and their programs' progress, as the plugin API lets every plugin, and needs no capability.
