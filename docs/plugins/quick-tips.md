# `quick-tips`: a tip at the start

<sub>`lokyr/quick-tips` · bundled · [source](../../plugins/quick-tips)</sub>

A tip as Lokyr starts, once a week at most, or as often as [`every`](#config) says, in a card at the window's bottom right: a shortcut as your keymap binds it now, a feature that shows in no menu, or a command of a plugin that you installed.

## The card

- **It shows** two seconds after the start, in the focused window, so that the window and its prompt come first, and only while no notice of Lokyr's shows, as an error in the config.
- **It tells one tip**, under a lightbulb and "Did you know?": each tip shows once before any shows again, and the round starts over once all have shown.
- **Its text takes two lines**; a longer one ends in an ellipsis, and a More button beside Dismiss shows it whole, as a click anywhere on the card does, the card growing upwards to hold it.
- **A shortcut's tip** names the keys that run the command now, your own `[keys]` with them, and shows only while a key is bound to it; a tip of a plugin's command shows only while the plugin runs.
- **A command of an installed plugin** has a tip too: its keys, or, of none, the command palette's, to run it by its name.
- **It goes** after 12 seconds, held while the pointer rests on it, at the first key that you type into a pane, which still goes to the program, or by its Dismiss button.
- **Don't show again** stops the tips for good; the plugin keeps your choice in its state, `plugins/quick-tips.json` in the state's folder.

## Adding a tip

Each action, feature or shortcut that Lokyr gains has its tip, in the same change:

1. Its text in [`locales/en/quick-tips.ftl`](../../plugins/quick-tips/locales/en/quick-tips.ftl), the source, and in each other language's catalog: a message `tip-<name>`, of `{ $keys }` where it names a command's keys, one or two sentences, as two lines of the card show them before More.
2. Its entry in `CURATED`, in [`src/lib.rs`](../../plugins/quick-tips/src/lib.rs), the most useful first: `command(id, message, command, argument)` for a command's keys, which shows only while a key is bound to it, or `plain(id, message)` for a feature that needs no keys; the id, which the plugin's state keeps, never reused.
3. The plugin's tests check that every tip has its message in every catalog and that the ids are unique.

A plugin's own commands of no parameters have their tips by themselves, of their titles.

## Commands

| Command | Title | Keys | What it does |
|---|---|---|---|
| `quick-tips.show` | Show a Tip | | Shows the next tip at once, whatever the week and Don't show again |

## Config

```toml
[plugins.quick-tips]
every = 7
```

- **`every`**: the days between two tips as Lokyr starts, from 0 to 365, 7 by default; 0 shows one at every start, which helps to read them one after the other. It applies at the next start. The [configuration's guide](../CONFIGURATION.md#pluginsquick-tips) tells it whole.

## Turning it off

```toml
[plugins.quick-tips]
enabled = false
```

No tip shows, at the start nor by its command.

## Permissions

It reads the commands' names and titles and their keys, as the plugin API lets every plugin, and needs no capability.
