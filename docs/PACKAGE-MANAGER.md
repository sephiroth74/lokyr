# Sleip and the registry

Sleip (`sleip`) is Lokyr's plugin package manager. The registry is the online service Sleip downloads from and that authors publish to ([ADR 0008](adr/0008-separate-package-manager.md)).

## Role

The split is the same as cargo and rustc: the app runs plugins, and Sleip provides them.

| Sleip | The app (`lokyr`) |
|---|---|
| Resolves versions, downloads, checks signatures and checksums, extracts | Reads `plugins.lock` and the store from disk |
| Writes `plugins.lock` atomically | Watches the config directory and loads or unloads plugins |
| Publishes to the registry, manages tokens and owners | Asks the user to confirm capabilities, compiles, activates |
| Contains the HTTP/TLS clients | **Contains no networking code** |

- **How it ships**:
  - Inside the app: `Lokyr.app/Contents/MacOS/sleip` on macOS; next to the binary on Linux (F7).
  - An "Install command line tools" command puts it on the `PATH`.
  - It is also packaged on its own: Homebrew, and Linux packages from F7.
- **In-app UI**: provided by the first-party `plugin-manager` plugin, which calls `sleip --json` underneath.

## Commands

| Command | What it does |
|---|---|
| `sleip new <name>` | Scaffolds a plugin: manifest, Luau sources, the API's type definitions, tests |
| `sleip check` | Type-checks the Luau sources against the API's definitions, and checks the manifest and the API version |
| `sleip test` | Runs the plugin in a headless host with simulated events |
| `sleip dev` | Checks the plugin, reloads it into the running app (over IPC), and repeats on every change |
| `sleip add <plugin>` | Adds a plugin to `plugins.toml` (from the registry, `--path` or `--git`), then resolves and installs |
| `sleip remove <plugin>` / `sleip update [<plugin>]` | Removes or updates, always through the lockfile |
| `sleip list` / `search <text>` / `info <plugin>` | Installed plugins, catalog, details (capabilities, compatibility, versions) |
| `sleip login` / `logout` / `publish` / `yank` / `owner` | Registry account and publishing |
| `sleip gc` | Removes unreferenced packages from the store |
| `sleip doctor` | Diagnoses the installation, the cache, and compatibility with the app |
| `sleip audit` (F7) | Checks installed plugins against security advisories |

**Common flags**
- `--json`: emits NDJSON events on stdout, consumed by the app.
- `--locked`: fails if the lockfile would need to change.
- `--offline`: uses only the local cache and store.
- `--registry <name>`: selects a registry.

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
| `activation.on` | for `luau` | Activation events: `startup`, `command:<glob>`, `event:<name>`, `slot:<name>`, `osc:<number>`; a `builtin` plugin's is `startup` by default |
| `capabilities.*` | no | Requested capabilities, each `true` or `false`, but `net`, `fs-read`, `fs-write` and `env-read`, which list their hosts, paths and variables, as `net = ["api.github.com"]` |
| `uses.commands` | no | The commands that the plugin runs, by name or namespace (`tabs.*`), which Sleip shows at install, by category, with the capabilities they need ([ADR 0019](adr/0019-commands-errors-and-permissions.md)) |
| `contributes.*` | no | Static contributions (commands, keybindings, slots, themes, config schema), used by the palette, the registry and validation |
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
```

### `.tar.zst` archive

```
plugin.toml
src/                     # kind = "luau" only: the sources, entry point src/init.luau; never bytecode
themes/                  # kind = "themes" only
schema.json              # config schema, if any
README.md · LICENSE · CHANGELOG.md
assets/                  # icons and screenshots for the registry
```

Archive rules:
- regular files only, no symlinks or hard links;
- no absolute paths and no `..`;
- normalized permissions;
- at most 20 MB compressed and 100 MB extracted, with a compression-ratio check against zstd bombs.

## Resolution

- **Algorithm**: PubGrub (`pubgrub` crate), for readable conflict messages.
- **Constraints**: semver, caret by default, as in Cargo.
- **App compatibility**: each plugin's `api` constraint must include the installed app's API version. Sleip reads that version from `lokyr --api-version` or from the `api-version` file in the app's resources.
- **One version per plugin**: at runtime each plugin is a single instance.
- **Yanked versions** are excluded, unless they are already in the lockfile.

## Installation

1. Take an exclusive lock on the store (`flock`), so two Sleip processes never collide.
2. Resolve, then download any missing archives into a temporary directory.
3. Verify the SHA-256 checksum, from the index, whose signature Sleip checked as it read it.
4. Extract with the archive checks, then atomically rename into `store/<sha256>/`. The store is **content-addressed**: an installed package never changes.
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
- **Owner**: in v0, the PR author (a GitHub user or organization); in v1, the verified account.
