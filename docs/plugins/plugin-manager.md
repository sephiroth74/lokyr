# `plugin-manager`: the plugins, in the settings tab

<sub>`lokyr/plugin-manager` · bundled · [source](../../plugins/plugin-manager)</sub>

The Plugins page of the settings tab: every plugin that Lokyr runs, the ones that Sleip installed and the bundled ones, each with its state and what it may do, to turn off, on or remove without editing the config.

## The page

- **Lokyr › Plugins…** opens the settings tab at the page, as does Plugins… in the command palette; the tab's sidebar shows it too, as Plugins.
- **The installed plugins** come first, each of its name, its version, what it does, its state, and its capabilities with what you decided for each, allowed, refused or not asked yet; then **the bundled ones**.
- **Its state** tells whether the plugin runs, waits for what starts it, is turned off, or was stopped by a fault, and which.
- **Turn Off** and **Turn On** write `enabled` in the plugin's section of your config, [`[plugins.<namespace>]`](../CONFIGURATION.md#pluginsnamespace), keeping the rest of the file and its comments: the plugin stops at once, or starts again.
- **Remove**, of an installed plugin, runs `sleip remove` of it; the row tells as it goes, and why, if Sleip refuses, as of a plugin that another needs. Lokyr unloads it as it goes, without restarting.
- **The page follows the plugins** as they change: a plugin installed, removed, turned off or on elsewhere, or one that faults, shows at once.

To install a plugin, use Sleip, as [the plugins' index](README.md#third-party-plugins) tells.

## Commands

| Command | Menu | Default keys | What it does |
|---|---|---|---|
| `plugin-manager.open` | Lokyr › Plugins… | | Opens the settings tab at the Plugins page |

## Configuration

It has no keys of its own.

## Turning it off

```toml
[plugins.plugin-manager]
enabled = false
```

The Plugins page goes, and Lokyr › Plugins… too; Sleip's command line still lists, installs and removes plugins, and your config turns them off.

## Permissions

It asks for the packages' service, `packages`, which Lokyr's own plugins alone may hold: it lists the plugins, runs Sleip, and turns plugins off and on in the config.
