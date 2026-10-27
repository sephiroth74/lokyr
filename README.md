# Lokyr

A fast, lightweight terminal emulator for macOS, written in Rust and built around its own plugin system: tabs, splits, search and themes are all plugins. Third-party plugins are Luau scripts that run in a sandbox, installed by Sleip, a separate package manager.

> Lokyr is the terminal, and Sleip its package manager ([ADR 0018](docs/adr/0018-names-lokyr-and-sleip.md)). The project is in its third phase, F2: the headless terminal core is done, and the macOS app, with a window of one pane, is under way.

To learn more, start with the [documentation](docs/README.md), then read the [roadmap](docs/ROADMAP.md) and the [task breakdown](docs/tasks/README.md). The settings are in the [configuration guide](docs/CONFIGURATION.md).

## Contributing

Contributions are welcome: [CONTRIBUTING.md](CONTRIBUTING.md) tells how to set up the project and what a pull request needs. Everyone who takes part follows the [code of conduct](CODE_OF_CONDUCT.md).

## License

[MIT](LICENSE)
