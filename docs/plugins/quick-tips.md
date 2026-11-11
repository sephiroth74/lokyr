# `quick-tips`: a tip at the start

<sub>`lokyr/quick-tips` · bundled · [source](../../plugins/quick-tips)</sub>

A tip as Lokyr starts, once a week at most, in a card at the window's bottom right: a shortcut as your keymap binds it now, a feature that shows in no menu, or a command of a plugin that you installed.

## The card

- **It shows** two seconds after the start, in the focused window, so that the window and its prompt come first, and only while no notice of Lokyr's shows, as an error in the config.
- **It tells one tip**, under a lightbulb and "Did you know?": each tip shows once before any shows again, and the round starts over once all have shown.
- **A shortcut's tip** names the keys that run the command now, your own `[keys]` with them, and shows only while a key is bound to it; a tip of a plugin's command shows only while the plugin runs.
- **A command of an installed plugin** has a tip too: its keys, or, of none, the command palette's, to run it by its name.
- **It goes** after 12 seconds, held while the pointer rests on it, at the first key that you type into a pane, which still goes to the program, or by its Dismiss button.
- **Don't show again** stops the tips for good; the plugin keeps your choice in its state, `plugins/quick-tips.json` in the state's folder.

## Commands

| Command | Title | Keys | What it does |
|---|---|---|---|
| `quick-tips.show` | Show a Tip | | Shows the next tip at once, whatever the week and Don't show again |

## Turning it off

```toml
[plugins.quick-tips]
enabled = false
```

No tip shows, at the start nor by its command.

## Permissions

It reads the commands' names and titles and their keys, as the plugin API lets every plugin, and needs no capability.
