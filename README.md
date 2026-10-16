# Bolt

A fast, lightweight terminal emulator for macOS, written in Rust and built around its own plugin system: tabs, splits, search and themes are all plugins. Third-party plugins are Luau scripts that run in a sandbox, installed by Crossbow, a separate package manager.

> Bolt is the terminal, and Crossbow its package manager: a crossbow shoots bolts ([ADR 0014](docs/adr/0014-names-bolt-and-crossbow.md)). The project is in its second phase, the headless terminal core: the parser, the grid, the input encoders, the PTY and the session exist, and the window comes in F2.

To learn more, start with the [documentation](docs/README.md), then read the [roadmap](docs/ROADMAP.md) and the [task breakdown](docs/tasks/README.md).

## License

[MIT](LICENSE)
