# `plugin-manager`: the plugins, in the settings window

<sub>`lokyr/plugin-manager` · bundled · [source](../../plugins/plugin-manager)</sub>

The Plugins page of the settings window: every plugin that Lokyr runs, the ones that Sleip installed and the bundled ones, each with its state and what it may do, to disable, enable or remove without editing the config.

## The page

- **Lokyr › Plugins…** opens the settings window at the page, or brings it forward there, as does Plugins… in the command palette; the window's tab Plugins shows it too.
- **The installed plugins** come first, each of its name, its version, what it does, its state, and its capabilities with what you decided for each, allowed, refused or not asked yet; then **the bundled ones**.
- **Its state** is Active, in green, as the plugin runs or waits for what starts it, or Disabled, in yellow, as it is turned off or a fault stopped it, which a line tells; both of the theme's colors, its `text.success` and `text.warning`.
- **Disable** writes `enabled = false` in the plugin's section of your config, [`[plugins.<namespace>]`](../CONFIGURATION.md#pluginsnamespace), keeping the rest of the file and its comments, and the plugin stops at once; **Enable** takes that key away, and the section with it if nothing else is left of it, and the plugin starts again.
- **Remove**, of an installed plugin, runs `sleip remove` of it; the row tells as it goes, and why, if Sleip refuses, as of a plugin that another needs. Lokyr unloads it as it goes, without restarting, and takes its key `enabled` out of your config.
- **The page follows the plugins** as they change: a plugin installed, removed, turned off or on elsewhere, or one that faults, shows at once.

To install a plugin, use Sleip, as [the plugins' index](README.md#third-party-plugins) tells.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `plugin-manager.open` | Lokyr › Plugins… | | Opens the settings window at the Plugins page |

## Configuration

It has no keys of its own.

## Turning it off

```toml
[plugins.plugin-manager]
enabled = false
```

The Plugins page goes, and Lokyr › Plugins… too; Sleip's command line still lists, installs and removes plugins, and your config disables them.

## Permissions

It asks for the packages' service, `packages`, which Lokyr's own plugins alone may hold: it lists the plugins, runs Sleip, and disables and enables plugins in the config.
