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
  - On the `PATH` of Lokyr's panes, whose end is the bundle's `Contents/MacOS`; elsewhere, once Lokyr › Install Command Line Tools… links it and `lokyr` into `/usr/local/bin`, asking for an administrator's password where the user may not write there ([UI-AND-FEATURES.md](UI-AND-FEATURES.md#the-command-line-tools)), or of its path or a link of the user's own.
  - It is also packaged on its own: Homebrew, and Linux packages from F7.
- **In-app UI**: provided by the first-party `plugin-manager` plugin, which calls `sleip --json` underneath.

## Commands

| Command | What it does |
|---|---|
| `sleip new <name>` | Scaffolds a plugin of Luau that works, as `owner/name`, in a folder of its name after the owner: its manifest, `src/init.luau` of a command that shows a notice, its texts in English, a test, the API's type definitions and luau-lsp's settings |
| `sleip check` | Checks the plugin of the current folder as the registry's CI checks it: its manifest, of its capabilities, of the ones that the app refuses before 1.0.0, and its `[uses]`; the plugin API's version against Sleip's; its catalogs, each that parses, and its commands' titles in English; its package's layout and sizes; and its sources' types, `src/**/*.luau` in strict mode, against the API's definitions that Sleip holds, of Luau's own type checker, of the solver that luau-lsp takes by default, without the globals that the app takes away, as `print` and `loadstring`; each problem of its file, line and column, and a failure of any |
| `sleip test` | Runs the plugin's tests, each function of the table that each file `tests/*.luau` returns, of a `TestHost` of its own, the plugin made anew on the SDK's fake host as the app sets it up: of its manifest's capabilities, `[uses]` and slots, as the app reads them, of its package's `src/` alone, beside the app's commands and its first-party plugins' (`luau/commands.json`, which `just luau-commands` writes of the app's registry and its tests check); its commands run as the registry runs them, their arguments checked and their defaults given, and the events reach it of its subscriptions, as the app delivers them; a failure tells its file and line, and fails the command, as a file of no test does |
| `sleip dev` | Checks the plugin and links its folder into the app's plugins, a dev entry of `plugins.lock` that names the folder, of no hash, which the app watches and reloads at each change, and checks it again at each change, telling its problems; Ctrl-C ends it, the entry taken back, the entry of an installed plugin of its name, whose place it took meanwhile, given back, and a lockfile that it made, of no plugin left, taken away ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) |
| `sleip add <plugin>[@<constraint>]` | Adds a plugin of the registry to `plugins.toml`, as owner/name, of the constraint given, as `acme/hello@^0.2`, or of the newest version that the app's plugin API takes, written as `^<version>`: its versions of the registry's signed index, yanked ones left out but one that the lockfile holds, resolved with the rest; the archive of each version picked downloaded, or of Sleip's cache, its SHA-256 checked against the index before the store takes it; the lockfile's source `registry+<address>`; the capabilities that it asks for told |
| `sleip add [<plugin>] --path <folder>` / `--git <url> (--tag <tag> \| --rev <commit>)` | Adds a plugin to `plugins.toml`, edited in place, of a folder or a git repository's tag or commit, its name its manifest's; fetches the list's plugins that the lockfile lacks, resolves them all against the app's plugin API, stores each package and writes the lockfile whole, which the app reloads; a failure changes nothing, the packages new to the store taken away and those that it held already kept. The registry's plugins come with registry v0 |
| `sleip remove <plugin>` / `sleip update [<plugin>]` | Removes or updates, always through the lockfile, `update` taking the registry's plugins of the list, or the one given, to the newest versions that their constraints and the app's plugin API take, and fetching the folders and the repositories again; a plugin removed takes out the plugins that it needed and that no other plugin of `plugins.toml`, nor of a `sleip dev` folder, needs |
| `sleip list` / `search <text>` / `info <plugin>` | Installed plugins, catalog, details (capabilities, compatibility, versions) |
| `sleip publish` | Opens the registry's pull request of a plugin's tagged version |
| `sleip login` / `logout` / `yank` / `owner` (registry v1) | Registry accounts, which registry v0, of GitHub's accounts and pull requests, has none of ([ADR 0038](adr/0038-plugin-ecosystem-scope.md)) |
| `sleip gc` | Removes unreferenced packages from the store; refused without a lockfile in the config's folder while the store holds packages, as every config folder shares the store |
| `sleip doctor` | Diagnoses the installation, the cache, and compatibility with the app |
| `sleip search <words>` | Searches the registry's plugins, each that every word finds in its name or its description, of the registry's list, `/list`, signed as an index file is; `--offline` of the cache |
| `sleip info <plugin>` | Tells a plugin of the registry: its version shown, description, capabilities, the commands it runs, its dependencies, and its versions, newest first, of their checksums, of its index file; `--offline` of the cache |
| `sleip publish [--tag <tag>] [--registry <owner/repository>] [--link]` | Proposes the version of the plugin of the current folder to the registry: checks and tests it as the registry's CI does, finds its tag, `v<version>` or the one given, of `plugin.toml`'s version at that commit, which the plugin's repository holds; then the registry's file of the plugin, of the version added at its end or made of its first, goes to a pull request: through `gh`, of the author's fork, or of the registry itself for its owner, its branch and file made through GitHub's API; without `gh`, or with `--link`, a link that proposes it on GitHub; of a registry of git that is not GitHub's, a branch pushed to it. Tells what the registry's CI checks |
| `sleip registry` (hidden) | The registry's tools, which its CI runs, hidden from the help: `verify`, a pull request of the registry checked, as [below](#v0-static-end-of-f4); `build`, a version's archive packaged again from its commit, alike; `versions`, every version of the registry, of its release's tag and archive; `keygen` and `sign-keys`, the registry's keys and its `keys.json`, offline; `index`, the index generated and signed; `site`, the registry's site of it; and `readme`, the README's list of the plugins |
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
- The app's plugin API, which the plugins are resolved against, of the app that `SLEIP_LOKYR` names, a bundle, its resources' `api-version` file or its binary, alone where it names one; else of the bundle that Sleip's binary is in, through the links to it; of `Lokyr.app` in the Applications folders; or of `lokyr --api-version` on the `PATH`.
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
| `plugin.description`, `repository`, `homepage`, `keywords`, `authors` | no | Metadata for the registry: texts of a line, of no control character, C0 or C1, nor change of direction (U+061C, U+200E, U+200F, U+202A to U+202E, U+2066 to U+2069); the description of 300 characters at most, the license of 100, an address of 2048, a keyword of 50, an author of 200; 20 keywords and 20 authors at most |
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
- **A registry** is of `https://`, `file://` of a folder, as a mirror, or `http://` of the machine itself, as the tests serve one; `default` is `https://pkg.lokyr.dev` unless the file names another.
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
{"v":1,"dl":"https://github.com/sephiroth74/sleip-registry/releases/download/{owner}.{name}.{version}/{name}-{version}.tar.zst"}
```

Registry v1 adds its API's address, `api`, beside `dl`.

`GET /keys.json` lists the index keys, which sign the index, signed by the registry's root key, whose public part Sleip holds in its binary:

```json
{"v":1,"keys":["ed25519:…"],"generated":"2026-11-04T10:00:00Z","sig":"ed25519:…"}
```

`GET /index/{owner}/{name}` returns NDJSON, one line per version, then a last line, the file's signature, of the lines before it and of its time, by one of the index keys. The lines of the versions are only ever appended; the only field that changes is `yanked`. `sleip registry index --deployed <address>` holds it so: it reads the index deployed, checked as Sleip checks it, keeps each of its lines as it was signed, of its yank the registry's, never dropping one, and reads the new versions alone of their archives, those at hand or downloaded of their releases, public, so that an asset replaced in its release changes no line that was signed, and a run of no new version downloads no archive for the index; a version whose archive cannot be had or does not hold it is told as `failed` and left to a later run, the others written; a plugin's file that does not read keeps its lines. The site reads the archive of each plugin's version shown, for its README and license, checked against the index's checksum: one that does not check is told as `failed`, its page of the index alone. The registry's files are read of `plugins/` alone.

```json
{"v":1,"name":"sephiroth74/search-plus","vers":"0.1.2","kind":"luau","api":"^0.1","deps":[{"name":"lokyr/themes","req":"^0.3"}],"caps":["terminal-read","clipboard-write"],"uses":["pane.copy","tabs.*"],"cksum":"sha256:9f2c…","yanked":false}
{"generated":"2026-11-04T10:00:00Z","key":"ed25519:…","sig":"ed25519:…"}
```

Sleip refuses a file signed by a key that `keys.json` lacks, or older than the one that it holds. Every file expires from its signed time, `generated`, of a lifetime that Sleip is built with, so that the signed format stays as it is: online, an index file or the list older than a week, or a `keys.json` older than a year, is refused as expired, at a first contact too, as it could be an old copy that hides a later change, a yank or a key taken off; offline, of the cache, it is taken with a note. A file of a time more than a day to come is refused, as it would pin the Sleips that took it. A `keys.json` made before `KEYS_NOT_BEFORE`, the time of the one that Sleip was built with, is refused. The registry's own tools read the index deployed however old. HTTP caching uses `ETag`/`If-None-Match`. Sleip keeps a local copy of the index for `--offline`, in its cache's `index/<registry>/`, each file written once it checks, beside its ETag.

`GET /list` returns the list of the registry's plugins, which `sleip search` reads, as NDJSON, a line of each plugin of its version shown, the latest not yanked, signed as an index file is, of the subject `*` in place of a plugin's name:

```json
{"v":1,"name":"sephiroth74/hello","vers":"0.1.8","description":"The registry's plugin of its tests.","caps":["clipboard-read"],"yanked":false}
{"generated":"2026-11-10T10:16:46Z","key":"ed25519:…","sig":"ed25519:…"}
```

An index file's line holds its version's `description` too. Sleip tells a person the texts of an index, of a plugin and of its tests as text: each control character but a line's end and a tab, and each change of direction, a replacement character, so that none moves the cursor, colors the terminal, talks to it or hides what follows; `--json` gives them as they are, JSON's escapes. Sleip speaks to a registry over HTTPS, of ureq and rustls of ring, the system's trust store through the platform's verifier, its redirects of HTTPS alone; of `file://`, a mirror, a plain file alone, read within its bound, no folder, device nor pipe; and of `http://` of the machine itself alone, of no redirect, as the tests serve a registry. An address is of no user (`user@host`), space, control character nor backslash (`pkg-model`'s `address`), in `plugins.toml` as in a request. A request's deadline is 30 seconds and a second for each 256 KiB that it may bring, so that an archive of 20 MiB has 110. The archives' address of `config.json`, `dl`, which no key signs, is of the registry's own scheme, of the machine's own `http://` alone, so that it leads to no file nor service of the machine's: an archive is checked against the signed checksum wherever it comes from. The cache keeps each registry in a folder of its scheme and address, `https_pkg.lokyr.dev`. A build of Sleip for debugging takes the root keys of `SLEIP_TEST_ROOT_KEYS` too, as its tests sign their own registry; a release never does.

The keys are ed25519, a key and a signature written `ed25519:` and the base64 of their bytes, a secret key in a file of its own, `ed25519-secret:` and the base64 of its seed. The root key signs `sleip keys.json v1`, the time and each index key, a line each; an index key signs `sleip index v1`, the plugin's name, the time, then the file's lines before its signature as they are, so that a host can neither serve a plugin's file for another's nor change a line. `sleip registry keygen` makes a key, its secret in a new file that no one else reads; `sleip registry sign-keys` writes `keys.json`, of the root key's file, offline; `sleip registry index`, which the CI of `main` runs in its environment of the index key, writes `config.json`, the registry's `keys.json`, checked against Sleip's root keys, and each plugin's file of the versions whose archives its releases hold, each file read back as Sleip reads it before it is written. `sleip registry site` writes the site beside the index: `index.html`, which lists every plugin and which Pagefind searches once the CI runs it over the pages, and `plugins/<owner>/<name>/`, a page of each plugin, of its versions, their capabilities and checksums, and of the README of the version shown, the latest that is not yanked: a plugin's texts as text alone, its Markdown's raw HTML shown as text, its links of the web or of mail alone, a relative one into its repository at the version's commit. The CI of `main` deploys both to GitHub Pages, at `pkg.lokyr.dev`, the index's files at their paths. `sleip registry readme` rewrites the registry's README between its marks `<!-- plugins:start -->` and `<!-- plugins:end -->`: a table of each plugin of the index, its name linked to its page, its version shown, its description and its capabilities, a plugin's texts as Markdown's text alone, their punctuation escaped, no HTML, an address of the web or of mail broken so that GitHub links it not, 200 characters at most. The CI of `main` writes it after the index and, of a list that changed, opens a pull request of its own and merges it, as `main` takes changes of pull requests alone; of a README that changed meanwhile on `main`, it leaves the list to the next run.

## Signatures

- **Phase 1 (F4)**, as [ADR 0031](adr/0031-registry-v0.md) decided:
  - The registry's root key, offline, the maintainer's, whose public part Sleip holds in its binary, signs `keys.json`, the list of the index keys; a new root comes with a new Sleip.
  - An index key, which the registry's CI holds in a GitHub environment of `main` alone, signs each index file whole, after each merge; a stolen index key is taken off `keys.json`, which the Sleips already installed follow.
  - Sleip checks the index's signature, which covers each version's checksum.
  - A publisher is its GitHub account, which its pull request carries, as the user chose on 4 November 2026: the registry's CI builds each archive from its tag, so that no publisher could sign it, and a publisher's own signature comes with trusted publishing, in phase 2.
- **Phase 2 (F7)**: trusted publishing via OIDC (GitHub Actions) and Sigstore signatures, as PyPI and crates.io do.

### The registry's keys, kept

- **The root key's secret** is made by `sleip registry keygen` straight onto a medium offline, a removable drive or an encrypted volume: never on the Mac's own disk, in a folder that a cloud service copies, in a repository, `private/` among them, nor on GitHub; a second copy on a second medium, kept apart.
- It signs `keys.json` alone, of `sleip registry sign-keys`, as an index key is added, replaced or taken off; the medium goes back offline at once. Its public part is in Sleip's `ROOT_KEYS`.
- **The index key's secret** is `SLEIP_INDEX_KEY`, a secret of the registry's environment `index`, which the branch `main` alone reaches; made by `keygen` into a file that goes to the secret and is deleted, its public part in `keys.json`.
- **An index key stolen or lost**: a new one made and put in the secret, `keys.json` signed again of the new key alone, the old one off it; the Sleips installed follow `keys.json`, and the next Sleip's `KEYS_NOT_BEFORE` takes the new `keys.json`'s time, so that it never takes the old one again, at a first contact either.
- **`keys.json` once a year**: Sleip refuses online a `keys.json` older than a year (`KEYS_LIFETIME`), so that the maintainer signs it anew, of the same keys, with the root key offline, before the current one's year ends: the one of 10 November 2026 before 10 November 2027.
- **The index every few hours**: Sleip refuses online an index file or the list older than a week (`INDEX_LIFETIME`); the registry's CI signs them anew every six hours, and its scheduled runs turn the schedule on again, which GitHub turns off after 60 days of no activity of a public repository's.
- **The root key lost or stolen**: a new root made, its public part in a new Sleip's `ROOT_KEYS`, the old one's taken off, and `keys.json` signed again of it; the Sleips before it read no `keys.json` of the new root, and their users take the new Sleip, as the app's update brings it.

## Registry

### v0: static (end of F4)

As [ADR 0031](adr/0031-registry-v0.md) decided, on 4 November 2026:
- **Repo**: `sephiroth74/sleip-registry` holds a file of each plugin, `plugins/<owner>/<name>.toml`, of its repository's URL, over HTTPS, its manifest at its root, and of its versions, in their order, each greater than the one before, each of its tag, the tag's commit, its whole hash, and whether it is `yanked`; `sleip-index`'s `registry` reads it:

  ```toml
  repository = "https://github.com/acme/hello"

  [[versions]]
  version = "0.1.0"
  tag = "v0.1.0"
  commit = "0123456789abcdef0123456789abcdef01234567"
  ```
- **Publishing = opening a PR**, which `sleip publish` can open. CI then:
  - checks that the PR's author is the plugin's owner, the GitHub user or a member of the organization of its name, a push of another account the maintainer's to review (`--sender`), and that the tag names its commit;
  - packages the plugin **from the tagged sources** in a clean container;
  - checks it as `sleip check` does, runs its tests, and validates the manifest, the capabilities, `[uses] commands`, the names and the sizes;
  - refuses a published version changed or taken out; a name that another owner holds, in `main` or in the pull request; the owner `lokyr`, and the names of the first-party plugins and of the app's own commands and contexts; and a plugin's file that is a link or a submodule;
  - tells what it found in the pull request's comment, each text of the pull request or of a plugin, its files' names, repositories, the check's and the tests' messages, a span of code of no backtick, `@` nor `<`, of 500 characters at most, and the comment of 60 000 bytes at most.

  `sleip registry verify` does it all but the author's organizations, which the workflow reads of GitHub, of every page, of the pull request's own changes since the commit that it shares with `main`, as `git diff-tree --no-renames` tells them of each entry's modes, each judged against `main` as it is now, so that a branch made before another plugin's version merged is judged of its own changes alone; the workflow, of `pull_request_target` of the pull requests into `main` alone, is `main`'s whatever the pull request changes, reads the pull request's files as data, and runs the plugins' code, in Sleip's sandbox and of Luau's interpreter alone, in a job that holds no right of the repository's; the jobs that comment and merge take its report as data, into `$RUNNER_TEMP`, its Markdown alone, a plain file of 60 000 bytes at most, of no mention; every job gives its token to `gh` alone, the steps that clone and build of none. `.github/CODEOWNERS` makes the workflows, their actions and `keys.json` the maintainer's, a pull request that changes them waiting for the maintainer's review as `main`'s ruleset asks of the code owners, so that no job's token changes them. The CI runs a build of Sleip for Linux on arm64, a release of the registry's repository, which `just registry-sleip` and `just registry-sleip-release` make, and the workflow pins by its SHA-256.
- **Merging the PR** publishes the version: the CI of `main` packages the archive again from its commit, alike, uploads it to a release of the registry's repository, `<owner>.<name>.<version>`, generates the index, signs it, and deploys it with the site; the workflow that merges a version by itself makes its release itself, as a merge of a workflow's own token starts no other workflow. The job that runs the plugins' code, of no right, decides no merge alone: the job that merges checks the pull request again of `sleip registry verify --files-alone`, of its commit fetched from GitHub as data, no file of it checked out, that each file is a plugin's of an owner that the author is, a version added or a yank alone, pushed by the author, that each new version asks for nothing that the last published did not, their manifests read of git's objects of their commits, in a clone of no checkout, a plain file of 1 MiB at most, no plugin's code run, and merges into `main` alone. Each version is built of the commit that the file pins, fetched by its hash alone, of a depth of one, whatever branch or tag holds it, no file of Git LFS fetched and a transfer slower than a kilobyte a second for a minute stopped; its tag, which `git ls-remote` reads of the repository's tags, peeled, checked before the merge alone, a tag's name as `git check-ref-format` takes it and as no revision reads it (no `~`, `^`, `:`, `@{`, `..`, glob nor control character, 128 bytes at most); and released on its own: one that fails, as of a repository or a commit gone, told and the next released, the job failing at its end, and a schedule every six hours tries again; a draft that a stopped upload left is made anew. A yanked version of no release is skipped, so that a yank ends a version that can no longer be built, and the index lists the released versions alone. A name that two owners hold on `main`, which no check of a pull request lets in, is refused by `versions` and `index` alike, told as failed, the one deployed keeping its lines. Every job has a timeout; the README and GitHub Pages follow an index written, whichever release failed before it; the publishing runs of a change of `plugins/`, `keys.json`, the actions or its own workflow; both workflows check out `main` as it is; the pull request's comment comes after the merge's attempt, which it tells if it failed. `verify` weighs a new version against the last one of its commit too, its manifest read of git's objects. `sleip check` and `sleip test` read none of a plugin's files while its manifest or an entry of `src/`, `tests/` or `locales/` is a link. The repository's rulesets: `main` takes no deletion, no force push and changes of pull requests alone, of no approval but the code owners' of their files, which the workflow's merges are, the repository's admin bypassing them; its branches are removed as their pull requests merge; its tags are neither moved nor deleted, so that a release stays as it was made.
  - A new plugin is merged by the maintainer, after a review.
  - A version of a plugin already published merges by itself if it asks for nothing new, against the version published last: no capability, no host, path or variable of one, and no command in `[uses] commands`; one that asks for more is reviewed.
  - A yank, `yanked = true`, merges by itself; taking one back, or a change of a file other than a plugin's, is reviewed.
- **Hosting**: the index and the site on GitHub Pages, `pkg.lokyr.dev` a CNAME of it; the archives in the releases, where a version published stays as it was, whatever its author does to their repository.
- **The site**: static, generated from the index; each plugin's page, with its README, versions, capabilities and commands, and a search in the browser, as Pagefind's. A plugin's texts show as text: its README's raw HTML as text, its links of the web, of mail and into its repository alone, of `rel="nofollow ugc noopener"`; its README of 512 KiB at most and its HTML of 2 MiB, written as it is read and stopped past them, a README beyond them told as failed and its page shown without it; its description of 300 characters and its license of 100 shown. Each page has a content's policy: a plugin's of no script, images of the site and of HTTPS; the home page's of the site's own scripts alone, Pagefind's and `search.js`. The README's list of the plugins breaks every address of the web, `www.` whatever its case, and of mail, and its marks are each once, on a line of their own.
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
