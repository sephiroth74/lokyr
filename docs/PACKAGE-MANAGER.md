# Sleip and the registry

Sleip (`sleip`) is Lokyr's plugin package manager. The registry is the online service Sleip downloads from and that authors publish to ([ADR 0008](adr/0008-separate-package-manager.md)).

## Role

The split is the same as cargo and rustc: the app runs plugins, and Sleip provides them.

| Sleip | The app (`lokyr`) |
|---|---|
| Resolves versions, downloads, checks signatures and checksums, extracts | Reads `plugins.lock` and the store from disk |
| Writes `plugins.lock` atomically | Watches the config directory and loads or unloads plugins |
| Publishes to the registry, of a pull request in v0; its tokens and owners in v1 | Asks the user to confirm capabilities, compiles, activates |
| Contains the HTTP/TLS clients | **Contains no networking code** |

- **How it ships**:
  - Inside the app: `Lokyr.app/Contents/MacOS/sleip` on macOS, signed on its own and notarized with the app, its crates' licenses among the bundle's notices and in the release's bill of materials; next to the binary on Linux (F7).
  - On the `PATH` of Lokyr's panes, whose end is the bundle's `Contents/MacOS`; elsewhere, of its path or of a link to it in a folder of the `PATH`, until an "Install command line tools" command puts it there ([B-645](tasks/BACKLOG.md)).
  - It is also packaged on its own: Homebrew, and Linux packages from F7.
- **In-app UI**: provided by the first-party `plugin-manager` plugin, which calls `sleip --json` underneath.

## Commands

| Command | What it does |
|---|---|
| `sleip new <name>` | Scaffolds a plugin of Luau that works, as `owner/name`, in a folder of its name after the owner: its manifest, `src/init.luau` of a command that shows a notice, its texts in English, a test, the API's type definitions and luau-lsp's settings |
| `sleip check` | Checks the plugin of the current folder as the registry's CI checks it: its manifest, of its capabilities, of the ones that the app refuses before 1.0.0, and its `[uses]`; the plugin API's version against Sleip's; its catalogs, each that parses, and its commands' titles in English; its package's layout and sizes; and its sources' types, `src/**/*.luau` in strict mode, against the API's definitions that Sleip holds, of Luau's own type checker, of the solver that luau-lsp takes by default, without the globals that the app takes away, as `print` and `loadstring`; each problem of its file, line and column, and a failure of any |
| `sleip test` | Runs the plugin's tests, each function of the table that each file `tests/*.luau` returns, of a `TestHost` of its own, the plugin made anew on the SDK's fake host; a failure tells its file and line, and fails the command |
| `sleip dev` | Checks the plugin and links its folder into the app's plugins, a dev entry of `plugins.lock` that names the folder, of no hash, which the app watches and reloads at each change, and checks it again at each change, telling its problems; Ctrl-C ends it, the entry taken back, the entry of an installed plugin of its name, whose place it took meanwhile, given back, and a lockfile that it made, of no plugin left, taken away ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) |
| `sleip add [<plugin>] --path <folder>` / `--git <url> (--tag <tag> \| --rev <commit>)` | Adds a plugin to `plugins.toml`, edited in place, of a folder or a git repository's tag or commit, its name its manifest's; fetches the list's plugins that the lockfile lacks, resolves them all against the app's plugin API, stores each package and writes the lockfile whole, which the app reloads; a failure changes nothing, the packages that it stored taken away. The registry's plugins come with registry v0 |
| `sleip remove <plugin>` / `sleip update [<plugin>]` | Removes or updates, always through the lockfile, `update` fetching the folders and the repositories of the list, or the one given, again; a plugin removed takes out the plugins that it needed and that no other plugin of `plugins.toml`, nor of a `sleip dev` folder, needs |
| `sleip list` / `search <text>` / `info <plugin>` | Installed plugins, catalog, details (capabilities, compatibility, versions) |
| `sleip publish` | Opens the registry's pull request of a plugin's tagged version |
| `sleip login` / `logout` / `yank` / `owner` (registry v1) | Registry accounts, which registry v0, of GitHub's accounts and pull requests, has none of ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) |
| `sleip gc` | Removes unreferenced packages from the store; refused without a lockfile in the config's folder while the store holds packages, as every config folder shares the store |
| `sleip doctor` | Diagnoses the installation, the cache, and compatibility with the app |
| `sleip registry` (hidden) | The registry's index generated and signed, which the registry's CI runs, hidden from the help |
| `sleip audit` (F7) | Checks installed plugins against security advisories |

**Common flags**
- `--json`: emits NDJSON events on stdout, consumed by the app.
- `--locked`: fails if the lockfile would need to change.
- `--offline`: uses only the local cache and store.
- `--registry <name>`: selects a registry.

**Output, exit codes and places**
- Text for a person by default; with `--json`, an event of NDJSON a line on the standard output, each a JSON object of its `event`: `created` of `new`, of `name`, `folder` and `files`; `plugin` of `list`, of `name`, `version`, `kind`, `source` and `capabilities`; `removed` of `remove`, of `name`; `collected` of `gc`, of a package's `digest`, or a `leftover` of an install that stopped, and its `bytes`, then `done` of `collected` and `bytes`; `check` of `doctor`, of `name`, `level`, `ok`, `note` or `error`, and `detail`, then `done` of `problems`; `test` of `test`, of `file`, `name`, `ok`, and a failure's `line` and `message`, then `done` of `passed` and `failed`; `installed` of `add` and `update`, of `name`, `version`, `source` and `capabilities`, of each plugin that the lockfile holds anew, then `done` of `changed`, and `note` of no Lokyr found, whose plugin API is then Sleip's; `checked` of `dev`, of `problems`, after its `problem` and `note` events, at its start and at each save, `linked`, of `name`, `folder` and the version that it `replaced`, if it took an installed plugin's place, and `unlinked`, of `name`, as it ends; `problem` of `check`, of `file`, `line` and `column`, both 0 of a problem of a whole file, `kind`, as `Manifest`, `Capability`, `Api`, `Catalog`, `Package`, `Source`, `SyntaxError`, `TypeError` or `InternalError`, and `message`, and `note`, of `file` and `message`, of what is not wrong but worth knowing, then `done` of `problems`; `waiting`, of what a command waits for, as the store's lock; and `error`, of its `message`.
- Exit codes: 0, done; 1, a failure, as a plugin that another needs, a problem that `doctor` or `check` found, or a test that failed; 2, a command line that is wrong, as the help tells.
- The places are the app's: the config's folder, `~/.config/lokyr/` or `$XDG_CONFIG_HOME/lokyr/`, of `plugins.toml`, `plugins.lock` and `permissions.toml`; the store, `~/Library/Application Support/lokyr/store/` on macOS, `$XDG_DATA_HOME/lokyr/store/` elsewhere; the cache, `~/Library/Caches/lokyr/sleip/` on macOS, `$XDG_CACHE_HOME/lokyr/sleip/` elsewhere.
- The app's plugin API, which the plugins are resolved against, of the app that `SLEIP_LOKYR` names, a bundle, its resources' `api-version` file or its binary; of the bundle that Sleip's binary is in, through the links to it; of `Lokyr.app` in the Applications folders; or of `lokyr --api-version` on the `PATH`.
- **The store's lock**: a command that changes the store or the lockfile, `remove` and `gc` so far, takes `store.lock`, beside the store rather than in it, so that the app's watch of the store takes no lock for a change; a second Sleip waits for the first, as it tells.
- `remove` takes the plugin out of `plugins.toml` in place, its comments and order kept, and out of `plugins.lock`, written whole; it refuses a plugin that another needs, and, of `--locked`, a change of the lockfile; the package stays until `gc`, the decisions on its capabilities in `permissions.toml`. `gc` refuses a lockfile that does not read, whose packages it could not tell.

## Package kinds

| `kind` | Contents | Capabilities |
|---|---|---|
| `luau` | Luau sources plus assets | As declared |
| `themes` | Theme files only (`.itermcolors`, `.toml`) | None: contains no code |
| `builtin` | Manifests of the first-party plugins in the repo | Not distributed |

## File formats

### `plugin.toml`

```toml
[plugin]
name = "sephiroth74/search-plus"
version = "0.1.0"
kind = "luau"
api = "^0.1"                # host API version, like pubspec's `sdk` constraint
license = "MIT"
description = "Advanced search with filters and history"
repository = "https://github.com/sephiroth74/search-plus"
keywords = ["search", "productivity"]

[activation]
on = ["command:search-plus.*"]

[capabilities]
terminal-read = true
clipboard-write = true

[uses]
commands = ["pane.copy", "tabs.*"]   # the commands that the plugin runs

[contributes]
commands = [{ name = "search-plus.open", title = "Advanced search" }]
keybindings = [{ key = "cmd+shift+f", command = "search-plus.open" }]
slots = ["status-bar"]
config-schema = "schema.json"

[dependencies]              # other plugins required at runtime
"lokyr/themes" = "^0.3"
```

| Field | Required | Meaning |
|---|---|---|
| `plugin.name` | yes | `owner/name`: lowercase letters, digits and `-` |
| `plugin.version` | yes, but for `builtin` | Semver; a `builtin` plugin's is the app's |
| `plugin.kind` | yes | `luau`, `themes` or `builtin` |
| `plugin.api` | for `luau` | Constraint on the host API version |
| `plugin.license` | yes | SPDX expression |
| `plugin.description`, `repository`, `homepage`, `keywords`, `authors` | no | Metadata for the registry |
| `plugin.platforms` | no | Restricts the OSes (e.g. `["macos"]`); all by default |
| `plugin.thread` | no | `main`, by default, or `worker`, a thread of the host's for the plugin alone ([PLUGIN-SYSTEM.md](PLUGIN-SYSTEM.md#execution-model)) |
| `activation.on` | for `luau` | Activation events: `startup`, `command:<command>` or `command:<namespace>.*` of the plugin's own namespace, `event:<name>`, `slot:<name>`, `osc:<number>`; a `builtin` plugin's is `startup` by default |
| `capabilities.*` | no | Requested capabilities, each `true` or `false`, but `net`, `fs-read`, `fs-write` and `env-read`, which list their hosts, paths and variables, as `net = ["api.github.com"]` |
| `uses.commands` | no | The commands that the plugin runs, by name or namespace (`tabs.*`), which Sleip shows at install, by category, with the capabilities they need ([ADR 0019](adr/0019-commands-errors-and-permissions.md)) |
| `contributes.*` | no | Static contributions (commands, keybindings, slots, themes, config schema), used by the palette, the registry and validation; a command's `name`, `title` and `category`, `view` if none, which the menus and the palette show before a plugin of a `command:` activation activates; `config-schema` names the JSON Schema file of the plugin's section of the config, `[plugins.<namespace>]`, of the keywords that the app checks ([ADR 0032](adr/0032-plugins-config-and-state.md)) |
| `dependencies` | no | Other plugins required at runtime; plugins talk through kernel commands, events and services |

`pkg-model`'s `manifest::Manifest::parse` reads the manifest; one that does not read, as a capability misspelt or a field that the format lacks, tells its line and column. The first-party plugins' manifests, `plugins/*/plugin.toml`, of the kind `builtin`, sit beside their crates, which embed them; `app-core`'s build checks them, and fails with the file, the line and the column of one that does not read.

### `plugins.toml`

The plugins the user has declared, like the dependencies in `Cargo.toml` or `pubspec.yaml`.

```toml
# ~/.config/lokyr/plugins.toml
[plugins]
"sephiroth74/search-plus" = "^0.1"
"acme/nord-themes" = "1"
"me/my-plugin" = { path = "~/dev/my-plugin" }
"acme/git-status" = { git = "https://github.com/acme/git-status", tag = "v0.2.0" }

[registries]
default = "https://pkg.lokyr.dev"
```

- **A plugin** is of a version's constraint of the registry, as `"^0.1"`; of a folder, `path`, absolute or from `~`; or of a git repository, `git`, of `https://`, `ssh://`, `git@host:path` or `file://`, never `http://` nor `git://`, which do not authenticate their server, at a `tag` or a commit, `rev`, of 7 to 40 hexadecimal digits, one of them.
- **A registry** is of `https://`, or `file://` of a folder, as a mirror of the tests; `default` is `https://pkg.lokyr.dev` unless the file names another.
- `pkg-model`'s `list::List` reads it, each mistake at its line and column, and writes it whole, the plugins in their names' order.

### `plugins.lock`

```toml
# Generated by sleip. Do not edit by hand.
version = 1
api = "0.1.3"   # app API version used for resolution

[[package]]
name = "sephiroth74/search-plus"
version = "0.1.2"
kind = "luau"
source = "registry+https://pkg.lokyr.dev"
checksum = "sha256:9f2c…"
capabilities = ["terminal-read", "clipboard-write"]
dependencies = ["lokyr/themes 0.3.1"]

[[package]]
name = "me/my-plugin"
version = "0.0.1"
kind = "luau"
source = "dev+/Users/me/dev/my-plugin"
capabilities = []
dependencies = []
```

- **`version`**, the format's: a lockfile of another version is refused, as a newer Sleip wrote it.
- **`source`**: `registry+<address>`; `path+<path>`, a folder copied into the store; `git+<address>#<commit>`, of the commit's whole hash; or `dev+<path>`, a folder that `sleip dev` links, which the app reads in place and watches. Its paths are absolute.
- **`checksum`**: the digest of the plugin's package in the store, which names its folder there, and which the app checks as it loads the plugin; a `dev+` plugin has none, as its files change. The archive's own checksum is the index's `cksum`, which Sleip checks as it downloads it.
- **`capabilities`**: each as its manifest's text, `net:<host>` and the others of a parameter after a `:`; read in their order, each once.
- **`dependencies`**: `owner/name version`, each of the lockfile at that version, one version of each plugin.
- `pkg-model`'s `lock::Lock` reads it, each mistake at its line and column, and writes it whole: to a file beside it, flushed to the disk, which takes its place at once, so that the app, which watches it, reads the old lockfile or the new one, never a part.

### `.tar.zst` archive

```
plugin.toml
src/                     # kind = "luau" only: the sources, `.luau`, entry point src/init.luau; never bytecode
themes/                  # kind = "themes" only: `.toml` and `.itermcolors`
schema.json              # config schema, if any, or the file that `config-schema` names
locales/<language>/      # the catalogs, Project Fluent's `.ftl`, as `locales/it/hello.ftl`
README.md · LICENSE · CHANGELOG.md
assets/                  # icons and screenshots for the registry
```

Archive rules:
- regular files only, no symlinks or hard links;
- no absolute paths, no `..` and no empty part; no two paths that differ by their case alone, nor a file within a file, as the Mac's file system takes them for one;
- each file of the layout of its kind: a package of Luau holds `src/init.luau`, one of themes holds `themes/`, and the config's schema that the manifest names is there;
- normalized permissions: the store's, whatever the archive says;
- at most 20 MB compressed and 100 MB extracted, 10 000 files, and, past 1 MB extracted, 100 times its compressed size at most, against zstd bombs; frames of zstd's windows of 8 MiB at most.

`pkg-model`'s `archive`, of its feature `archive`, which Sleip and the registry's tools use and the app does not, packs a package's folder, the files of its layout alone, in their paths' order, of no time and no owner, so that a package packs alike wherever it is packed; and checks an archive as it unpacks it into the store, zstd of Rust alone, `ruzstd`.

### The store

`~/Library/Application Support/lokyr/store/` on macOS, `$XDG_DATA_HOME/lokyr/store/` elsewhere: a folder for each package, which Sleip unpacks beside the packages, its files flushed to the disk, and renames into `store/<digest>/` at once, and never changes; one whose files are no longer those of its digest, as of a `.DS_Store` of the Finder or a power cut, the same package added again takes the place of. The digest is the SHA-256 of the package's files alone, in the byte order of their paths, of `/` between their folders: each its path, a zero byte, its size as eight bytes, most significant first, and its bytes; its folders, its files' permissions and times count for nothing, so that the app checks a package of its folder as it loads it, and a package unpacked anew keeps its digest. `pkg-model`'s `store::Store` holds it.

## Resolution

- **Algorithm**: PubGrub (`pubgrub` crate, of MPL-2.0, which `deny.toml` takes for it and its two crates of the same license, `version-ranges` and `priority-queue`, Sleip's alone), for readable conflict messages: each plugin's versions its candidates, each of its `api` constraint, as a need of Lokyr's plugin API at the app's version, and of its dependencies; a folder's and a repository's plugin one version, its manifest's; a conflict told as "Because acme/top depends on acme/base >=2.0.0, <3.0.0 …, your plugins cannot all be installed.", and a plugin that no source holds named.
- **Versions**: [Semantic Versioning 2.0.0](https://semver.org/), of numbers of no leading zero, a pre-release after `-` and a build after `+`, which the manifest's check holds a version to.
- **Constraints**: semver, caret by default, as in Cargo: an operator, `^`, `~`, `=`, `>=`, `<=`, `>` or `<`, of a version or its first numbers, as `^0.3`, or `*`.
- **App compatibility**: each plugin's `api` constraint must include the installed app's API version, as Cargo reads a constraint; the app refuses one that does not, as a notice tells. Sleip reads that version from `lokyr --api-version` or from the `api-version` file in the app's resources.
- **One version per plugin**: at runtime each plugin is a single instance.
- **Yanked versions** are excluded, unless they are already in the lockfile.

## Installation

1. Take an exclusive lock on the store (`flock`), so two Sleip processes never collide.
2. Resolve, then download any missing archives into a temporary directory.
3. Verify the SHA-256 checksum, from the index, whose signature Sleip checked as it read it.
4. Extract with the archive checks, then atomically rename into `store/<digest>/`, of the digest of its files. The store is **content-addressed**: an installed package never changes.
5. Write the new `plugins.lock` to a temporary file, `fsync` it, and rename it atomically.
6. The app, which watches the directory, sees the change and computes the diff. It re-checks the hash of each plugin it loads, asks the user to confirm new capabilities, then compiles and activates. No restart is needed.

## Index (sparse protocol)

Like the crates.io sparse index: one file per package, which plain static hosting can serve ([ADR 0031](adr/0031-registry-v0.md)).

`GET /config.json`:

```json
{
  "dl": "https://github.com/sephiroth74/sleip-registry/releases/download/{owner}.{name}.{version}/{name}-{version}.tar.zst",
  "api": "https://pkg.lokyr.dev/api/v1"
}
```

`GET /keys.json` lists the index keys, which sign the index, signed by the registry's root key, whose public part Sleip holds in its binary:

```json
{"v":1,"keys":["ed25519:…"],"generated":"2026-11-04T10:00:00Z","sig":"ed25519:…"}
```

`GET /index/{owner}/{name}` returns NDJSON, one line per version, then a last line, the file's signature, of the lines before it and of its time, by one of the index keys. The lines of the versions are only ever appended; the only field that changes is `yanked`.

```json
{"v":1,"name":"sephiroth74/search-plus","vers":"0.1.2","kind":"luau","api":"^0.1","deps":[{"name":"lokyr/themes","req":"^0.3"}],"caps":["terminal-read","clipboard-write"],"uses":["pane.copy","tabs.*"],"cksum":"sha256:9f2c…","yanked":false}
{"generated":"2026-11-04T10:00:00Z","key":"ed25519:…","sig":"ed25519:…"}
```

Sleip refuses a file signed by a key that `keys.json` lacks, or older than the one that it holds. HTTP caching uses `ETag`/`If-None-Match`. Sleip keeps a local copy of the index for `--offline`.

## Signatures

- **Phase 1 (F4)**, as [ADR 0031](adr/0031-registry-v0.md) decided:
  - The registry's root key, offline, the maintainer's, whose public part Sleip holds in its binary, signs `keys.json`, the list of the index keys; a new root comes with a new Sleip.
  - An index key, which the registry's CI holds in a GitHub environment of `main` alone, signs each index file whole, after each merge; a stolen index key is taken off `keys.json`, which the Sleips already installed follow.
  - Sleip checks the index's signature, which covers each version's checksum.
  - A publisher is its GitHub account, which its pull request carries, as the user chose on 4 November 2026: the registry's CI builds each archive from its tag, so that no publisher could sign it, and a publisher's own signature comes with trusted publishing, in phase 2.
- **Phase 2 (F7)**: trusted publishing via OIDC (GitHub Actions) and Sigstore signatures, as PyPI and crates.io do.

## Registry

### v0: static (end of F4)

As [ADR 0031](adr/0031-registry-v0.md) decided, on 4 November 2026:
- **Repo**: `sephiroth74/sleip-registry` holds a file of each plugin, `plugins/<owner>/<name>.toml`, of its versions, each with its repository's URL, its tag and the tag's commit.
- **Publishing = opening a PR**, which `sleip publish` can open. CI then:
  - checks that the PR's author is the plugin's owner, the GitHub user or a member of the organization of its name, and that the tag names its commit;
  - packages the plugin **from the tagged sources** in a clean container;
  - checks it as `sleip check` does, runs its tests, and validates the manifest, the capabilities, `[uses] commands`, the names and the sizes.
- **Merging the PR** publishes the version: the CI of `main` uploads the archive to a release of the registry's repository, generates the index, signs it, and deploys it with the site.
  - A new plugin is merged by the maintainer, after a review.
  - A version of a plugin already published merges by itself if it asks for nothing new: no capability, no host, path or variable of one, and no command in `[uses] commands`; one that asks for more is reviewed.
- **Hosting**: the index and the site on GitHub Pages, `pkg.lokyr.dev` a CNAME of it; the archives in the releases, where a version published stays as it was, whatever its author does to their repository.
- **The site**: static, generated from the index; each plugin's page, with its README, versions, API, capabilities and commands, and a search in the browser, as Pagefind's.
- **Benefits**: there's no server to run, and every artifact can be traced to a commit. This is the model of Zed's extensions repository, and of Homebrew's.

### v1: service (F7)

Built once there are enough external plugins to justify it.
- **Stack**: axum, Postgres, S3-compatible storage, CDN. The sparse protocol stays the same, so Sleip doesn't change.
- **Accounts**: GitHub OAuth, scoped tokens, owner management.
- **API**: `PUT /api/v1/packages/new`, `DELETE …/{version}/yank`, `PUT …/{version}/unyank`, `GET /api/v1/search?q=`.
- **pub.dev-style web UI**: README, screenshots, requested capabilities shown prominently, API compatibility, download counts, verified publishers.
- **Security**: security advisories (checked by `sleip audit`) and abuse reports.

### Naming

- **Format**: `owner/name`, using lowercase letters, digits and `-`, at most 64 characters per part.
- **Reserved**: `lokyr/*` belongs to first-party plugins.
- **Unique across owners**: the namespace, which the app and the config know a plugin by, is the name after the owner, so that the registry's CI refuses a name that another owner holds, and `acme/x` and `bob/x` never meet in one app.
- **Owner**: in v0, the PR author (a GitHub user or organization); in v1, the verified account.
