# `settings`: the config, in the settings window

<sub>`lokyr/settings` · bundled · [source](../../plugins/settings)</sub>

The Settings page of the settings window: every key of your config, Lokyr's own and the bundled plugins', by its section, of what it does, the platforms where it applies and its value in force.

## The page

- **Lokyr › Open Settings Window** opens the settings window at the page, its first; the window's tab Settings shows it too.
- **Its sidebar** lists the config's sections, as Font, Window or Theme, then the sections of the plugins that have settings, as Tabs or Find; a click or the arrows choose one, and the page opens at the first.
- **Each key** shows its name, the platforms where it applies, what it does, and at its right its value in force, as your config gives it or as its default, marked Default where it is; a key of no default that your config leaves out shows as Not set.
- **The page follows your config**: saving the file shows the new values at once.
- The keys, their texts and their values are those of [the config guide](../CONFIGURATION.md), which the same schema of the config makes. To change a key, open the file with Lokyr › Settings… (⌘,).

## Commands

It has no command: Lokyr › Open Settings Window opens its page.

## Configuration

It has no keys of its own.

## Turning it off

```toml
[plugins.settings]
enabled = false
```

The Settings page goes; the settings window shows its other pages.

## Permissions

It asks for the config's settings, `config`, which Lokyr's own plugins alone may hold: it reads every key of the config and of the plugins' sections, with their schemas.
