# `settings`: the config, in the settings window

<sub>`lokyr/settings` · bundled · [source](../../plugins/settings)</sub>

The Settings page of the settings window: every key of your config, Lokyr's own and the bundled plugins', by its section, of what it does, the platforms where it applies and its value in force, which you change there, Lokyr writing it into your config file.

## The page

- **Lokyr › Open Settings Window** opens the settings window at the page, its first; the window's tab Settings shows it too.
- **Its sidebar** lists the config's sections, as Font, Window or Theme, each of an icon of its own, then the sections of the plugins that have settings, as Tabs or Find; a click or the arrows choose one, and the page opens at the first.
- **Its search field**, at the sidebar's top, which has the keyboard as the window opens, shows the sections, and in each the keys, that hold every word typed, whatever its case, in a key's name, in what it does or in its section's title; in the Keys part, a binding's keys, as `cmd+k` or ⌘K, or its command's name or title. The section chosen stays while it holds them, else the first that does; the button at the field's end empties it.
- **Each key** shows its name, the platforms where it applies where it does not apply on every one, what it does, **More info**, which opens its text in [the config guide](../CONFIGURATION.md) in your browser, of your version of Lokyr, and at its right a control of its value in force, as your config gives it or as its default:
  - a boolean, a toggle, On or Off;
  - a key of a few values, a button of its value, whose menu lists the values, the one in force ticked;
  - a font's family, a menu of the system's families, the monospaced ones of the text's own family, every one of the text past ASCII's and of the interface's; a font's style, a menu of the styles of the section's family, or of the family in force;
  - a theme, a menu of the themes of its appearance, light or dark; a plugin's color, a menu of the theme's colors by their tokens, of a swatch of the color beside it;
  - a number, a field, whose value ↑ and ↓ step, of the key's step, within its range;
  - a text, a field, empty where your config leaves out a key of no default;
  - a list, as `[agents] programs`, its values under what it does, each of a **Remove** link, and a field that adds one as you press Return;
  - a table, its value as the config writes it, which you change in the file.
- **A change** writes the key into your config file, keeping the rest of it and its comments, and applies at once, as saving the file would: a toggle or a menu's value as you choose it, a field's value as your typing pauses, as you press Return and as you leave the field. An empty field, or **Reset**, the link at the row's bottom right, dimmed where the key is at its default, takes the key out of the file, its default in force again.
- **A value that the key does not take**, past its range, not a number, or a whole number's fraction, is not written, and the page tells why under the key, until a value is written.
- **Keys**, after the config's sections, lists every binding of keys in force, Lokyr's defaults and your config's `[keys]`, by the categories of their commands, as Windows, Tabs or Edit, each of its keys as macOS writes them, as ⇧⌘N, of the title of the command that it runs, of its name, its arguments and the context where it holds, and of "your config" where it is yours; keys that your config takes away show as such, of the default's title. A click on a binding's keys shows them in a field, as `cmd+shift+n`: Return, or leaving the field, writes the command at the new keys and, of a default, `false` at its old keys, so that the shortcut moves; keys that do not read say why under the row. **Remove** takes a default away, writing `false` at its keys, or takes your own binding out of the file; **Restore** gives a default back, taking its `false` out. A new binding takes its keys, as `cmd+shift+n`, and its command's name, as `window.new`, and Return writes it, of a command that Lokyr has.
- **A config file that does not read** shows its error over the page, of its line and its column: the page shows the settings that applied last, and writes nothing until the file reads again.
- **The page follows your config**: saving the file shows the new values at once, the fields too.
- The keys, their texts and their values are those of [the config guide](../CONFIGURATION.md), which the same schema of the config makes. Lokyr › Settings… (⌘,) opens the file itself, for the keys that the page does not change.

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

It asks for the config's settings, `config`, which Lokyr's own plugins alone may hold: it reads every key of the config and of the plugins' sections, with their schemas, and writes the keys that you change into the config file.
