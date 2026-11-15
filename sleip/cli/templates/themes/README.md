# acme/harbor

Themes of [Lokyr](https://lokyr.dev), a package of Sleip's.

## Install

```sh
sleip add acme/harbor
```

Then choose a theme in View › Choose Theme…, or in the settings window's Themes page, or name it in Lokyr's config:

```toml
[theme]
dark = "Harbor"
```

## Develop

Each file of `themes/`, a `.toml` of Lokyr's format or an `.itermcolors` of iTerm2's, is a theme, its name its file's. `sleip check` checks the package; `sleip add --path .` installs it from this folder, and `sleip publish` proposes it to the registry. The guide for theme authors tells every key: https://lokyr.dev/docs/themes/authoring.html

## License

MIT.
