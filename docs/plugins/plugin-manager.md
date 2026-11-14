# `plugin-manager`: the plugins, in the settings window

<sub>`lokyr/plugin-manager` · bundled · [source](../../plugins/plugin-manager)</sub>

The Plugins page of the settings window: every plugin that Lokyr runs, the ones that Sleip installed and the bundled ones, each with its state and what it may do, to disable, enable, update or remove, and the registry's plugins, to install, without editing the config nor opening a terminal.

## The page

- **Lokyr › Plugins…** opens the settings window at the page, or brings it forward there, as does Plugins… in the command palette; the window's tab Plugins shows it too.
- **Its sidebar** has two parts, **Installed** and **Registry**, which a click or the arrows choose; the page opens at Installed.

### Installed

- **The installed plugins** come first, each of its name, its version, what it does, its state, and its capabilities with what you decided for each, allowed, refused or not asked yet; then **the bundled ones**.
- **Its state** is Active, in green, as the plugin runs or waits for what starts it, or Disabled, in yellow, as it is turned off or a fault stopped it, which a line tells; both of the theme's colors, its `text.success` and `text.warning`.
- **Disable** writes `enabled = false` in the plugin's section of your config, [`[plugins.<namespace>]`](../CONFIGURATION.md#pluginsnamespace), keeping the rest of the file and its comments, and the plugin stops at once; **Enable** takes that key away, and the section with it if nothing else is left of it, and the plugin starts again.
- **Remove**, of an installed plugin, runs `sleip remove` of it; the row tells as it goes, and why, if Sleip refuses, as of a plugin that another needs. Lokyr unloads it as it goes, without restarting, and takes its key `enabled` out of your config.
- **Update to**, of an installed plugin of a newer version in the registry that this Lokyr takes, installs that version, as `sleip add` of the plugin does.
- **The page follows the plugins** as they change: a plugin installed, removed, turned off or on elsewhere, or one that faults, shows at once.

### Registry

- **The plugins of [pkg.lokyr.dev](https://pkg.lokyr.dev)**, each of its newest version, what it does and the capabilities that it asks for; one that needs another version of Lokyr says so.
- **A loader** shows while Sleip reads them.
- **The field** filters them as you type, of their names and what they do; **Search**, or Return in the field, reads the registry again, for the plugins published since.
- **Install** runs `sleip add` of the plugin, which checks its archive against the registry's signed index; Lokyr loads it without restarting, and asks you for its capabilities the first time that it starts. A plugin installed already shows as such, or of its update.
- **Details** shows a plugin's versions, newest first, each of what it asks for, a yanked one or one that this Lokyr does not take marked, and the commands of other plugins that it runs.
- **A change under way**, an install, an update or a removal, shows as it goes, of a **Cancel** button; one that fails tells Sleip's reason.
- **Sleip reads the registry's signed list** as the page first shows in a run of Lokyr, one request to pkg.lokyr.dev, which answers of no content where the list has not changed since Sleip's copy, and again at each search. Lokyr itself never goes on the network: Sleip does, at these times alone, and nothing at rest.

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
