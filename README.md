# Bolt

A fast, lightweight terminal emulator for macOS, written in Rust and built around its own plugin system: tabs, splits, search and themes are all plugins. Third-party plugins are Luau scripts that run in a sandbox, installed by Crossbow, a separate package manager.

> Bolt is the terminal, and Crossbow its package manager: a crossbow shoots bolts ([ADR 0014](docs/adr/0014-names-bolt-and-crossbow.md)). The project is in its third phase, F2: the headless terminal core is done, and the macOS app, with a window of one pane, is under way.

To learn more, start with the [documentation](docs/README.md), then read the [roadmap](docs/ROADMAP.md) and the [task breakdown](docs/tasks/README.md). The settings are in the [configuration guide](docs/CONFIGURATION.md).

## Contributing

Contributions are welcome: [CONTRIBUTING.md](CONTRIBUTING.md) tells how to set up the project and what a pull request needs. Everyone who takes part follows the [code of conduct](CODE_OF_CONDUCT.md).

## License

[MIT](LICENSE)
