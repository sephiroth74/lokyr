# The plugin manager's page of the settings window.
page-plugins = Plugins
command-plugin-manager-open = Plugins…

# The sidebar's parts.
section-installed = Installed
section-registry = Registry

# The installed plugins.
plugins-installed = Installed
plugins-built-in = Built In
plugins-none-installed = No plugin is installed yet: the Registry lists the plugins of pkg.lokyr.dev.
plugin-state-active = Active
plugin-state-disabled = Disabled
plugin-fault = Stopped by a fault: { $fault }
plugin-capabilities = Capabilities: { $capabilities }
plugin-decision-granted = { $capability }, allowed
plugin-decision-refused = { $capability }, refused
plugin-decision-undecided = { $capability }, not asked yet
plugin-disable = Disable
plugin-enable = Enable
plugin-remove = Remove
plugin-removing = Removing…
plugin-change-failed = It did not change: { $message }
plugin-no-reason = Sleip stopped without telling why.

# The registry's plugins.
registry-title = Registry
registry-intro = The plugins of pkg.lokyr.dev, each checked against the registry's signed index as Sleip installs it. Lokyr asks you for a plugin's capabilities the first time that it starts.
registry-filter = Name or description
registry-search = Search
registry-reading = Reading the registry…
registry-failed = The registry could not be read: { $message }
registry-none = No plugin matches.
plugin-install = Install
plugin-installing = Installing…
plugin-update = Update to { $version }
plugin-updating = Updating…
plugin-installed = Installed
plugin-cancel = Cancel
plugin-details = Details
plugin-asks = Asks for: { $capabilities }
plugin-asks-nothing = Asks for no capability.

# A plugin's details.
plugin-back = Registry
plugin-reading = Reading its versions…
plugin-info-failed = Its versions could not be read: { $message }
plugin-uses = Runs: { $commands }
plugin-versions = Versions
version-yanked = Yanked
version-not-taken = Needs another version of Lokyr
