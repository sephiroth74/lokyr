# Contributing to Lokyr

Thank you for your interest in Lokyr. This guide tells how to set up the project, the rules that the code and the documents follow, and what a pull request needs before it can be merged.

Lokyr is young: its first version is under way, for macOS 26 or later on Apple Silicon. Linux comes later. By taking part, you agree to follow the [code of conduct](CODE_OF_CONDUCT.md).

## Before you start

- **A large change starts as an issue.** Open one before you write a new feature, a new dependency or a change of design, so that we can agree on its shape first. A small fix, a typo or a test can go straight to a pull request.
- **Read the documents of the part that you change.** Start with [docs/README.md](docs/README.md), then [ARCHITECTURE.md](docs/ARCHITECTURE.md), and the document of the area, such as [VT-CORE.md](docs/VT-CORE.md) for the terminal core. The work is planned in [docs/tasks](docs/tasks/README.md), one file per phase of the [roadmap](docs/ROADMAP.md).
- **The decisions are in [docs/adr](docs/adr/README.md).** A change that goes against one needs a new ADR first, which an issue discusses.

## Setting up

You need:
- a Mac with Apple Silicon, on macOS 26 or later;
- Xcode 26 or later, with its Metal toolchain, a download of its own since Xcode 26, which the renderer's shaders need to build: `xcodebuild -downloadComponent MetalToolchain`;
- Rust through [rustup](https://rustup.rs): `rust-toolchain.toml` pins the version, which rustup installs at the first build;
- [`just`](https://github.com/casey/just), which runs the project's recipes: `brew install just`;
- [`cargo-deny`](https://github.com/EmbarkStudios/cargo-deny), which checks the dependencies' licenses and advisories: `cargo install cargo-deny --locked`.

Then:

```sh
cargo run -p lokyr-app   # the terminal, with your login shell
just --list             # the project's recipes
just ci                 # the checks of CI, as the next section tells
```

Fuzzing uses a nightly Rust, which the `justfile` pins, and `cargo-fuzz`; you need it only to work on the fuzzers.

The bundle, `target/Lokyr.app`, which `just bundle` builds, is signed with the identity that `LOKYR_SIGN_IDENTITY` names, as `codesign --sign` takes it, and ad hoc without it. `just` and `cargo xtask` read the variables of the untracked `private/lokyr.env`, where the maintainer names the Developer ID and the notary's profile, so that no signing identity, team or credential is in the repository; a variable of the environment wins over the file's. `just notarize` has Apple notarize the bundle and staples the ticket, for the maintainer: it needs the Developer ID and the notary's credentials in the keychain, the profile `lokyr-notary` that `xcrun notarytool store-credentials` makes, or the one that `LOKYR_NOTARY_PROFILE` names; it sends the bundle to Apple's notary service.

The bundle carries the notices of the licenses of what it holds of other projects, in `Contents/Resources/licenses`: the symbols' font's, Sparkle's, and in `crates` every crate's of the binary, with an index, which `cargo xtask bundle` writes from `cargo metadata`, of the license that Lokyr takes of each crate's expression among those that `deny.toml` allows, its own files, or the license's text of SPDX's list, in `tools/xtask/licenses`, under its authors' copyright where its package has none; a crate that allows none of them fails the bundle, and a license that `deny.toml` comes to allow needs its text there.

The bundle holds Sparkle's updater, of the release that `tools/xtask/src/sparkle.rs` pins with its digest, which `cargo xtask bundle` downloads once into `target/sparkle`, and signs with the bundle's identity; the app loads it after its first window, and `cargo run` runs without it. The private EdDSA key that signs the updates is the maintainer's, in the login keychain under the account `lokyr` of Sparkle's `generate_keys`, with a copy in the untracked `private/`; the public one is in `sparkle.rs`.

`just dmg` builds `target/Lokyr-<version>.dmg` of the bundle, the app and a link to Applications over a background, after it has Apple notarize the bundle if it has no ticket yet, and has Apple notarize the image too; `cargo xtask dmg --no-notarize` builds it of the bundle as it is, to look at it. The window's look is the Finder's own `.DS_Store`, kept in `crates/app/resources/dmg`, which `cargo xtask dmg layout` has the Finder write again, with the background rendered from its SVG by `rsvg-convert`, when the window's size or the icons' places change; it needs the terminal's permission to control the Finder.

The updates' feed of tests, until the repository is public, is on the maintainer's Mac, in the untracked `private/feed`: `cargo xtask feed add target/Lokyr-<version>.dmg` puts a DMG there with its notes, `docs/releases/<X.Y.Z>.md` or the file that `--notes` names, and has Sparkle's `generate_appcast` write the appcast, each DMG and the appcast signed with the EdDSA key, the keychain's or the file that `LOKYR_SPARKLE_KEY_FILE` names; `--channel <name>` puts the version in a channel, as a beta's. `just feed-serve` has Lokyr on that Mac read the feed, through the updater's `SUFeedURL` in its defaults, and serves it on `127.0.0.1:8123` until Control-C; `defaults delete io.github.sephiroth74.lokyr SUFeedURL` gives the builds their own feed back.

`just release <version>` releases a candidate, `X.Y.Z-rc.N`, or a release, `X.Y.Z`, of the workspace's version, from the commit checked out, without changes, once GitHub has it: the bundle built as that version, its build's number above the feed's latest, which it refuses to release without, unless `--without-feed`, as a feed's first release, and with `SOURCE_DATE_EPOCH` set, which would give the number, the binary's dSYM, which the release profile makes beside it, and the bill of materials, CycloneDX's JSON of the crates that the binary holds, Sparkle and the symbols' font, kept in `target/releases/<version>`; the DMG, the bundle and the image notarized; a draft of the GitHub release, `v<version>`, a candidate's a pre-release, with the DMG, the dSYM and the bill, its notes `docs/releases/<X.Y.Z>.md`; and the DMG in the feed of tests. The maintainer publishes the draft, which makes its tag. `cargo xtask symbolicate <report.ips>` reads a crash report of macOS, from `~/Library/Logs/DiagnosticReports`: its crashed thread and the main thread, Lokyr's frames told by their functions, files and lines through the dSYM of the binary's UUID, which `target/releases` keeps, or the one that `--dsym` names. The app's own log, which a panic writes its backtrace to, is `~/Library/Logs/Lokyr/lokyr.log`.

## The gate: `just ci`

`just ci` runs the checks of CI that a Mac can make, and a pull request needs it green:
- formatting, `cargo fmt --check`, and clippy with every warning an error;
- the build and the tests of the whole workspace, and a build of the platform-independent crates for Linux;
- `cargo deny`: the dependencies' licenses, advisories and sources;
- `cargo xtask check-deps`: the [dependency rules](tools/xtask/deps-rules.toml), such as no networking in the app and no macOS crate in the platform-independent ones;
- `cargo xtask check-comments`: no comment names a task, a phase or an ADR;
- `cargo xtask check-links`: the internal links and anchors of the Markdown files resolve;
- `cargo xtask bundle`: the app's bundle, `target/Lokyr.app`, signed with the identity that `LOKYR_SIGN_IDENTITY` names, or ad hoc without it.

Two checks of CI are not in it, and run on Linux: the tests of the platform-independent crates, which `just ci` builds for Linux without running; and the count of the instructions that the terminal's core takes for a set of scenarios, which fails a pull request that raises one by more than 3% over its base, with `just instructions <base>` where Valgrind runs.

## The code

The [conventions](docs/ARCHITECTURE.md#conventions) are the full list. The ones that reviews meet most:

- **English** for the code, its comments, the documents and the commit messages.
- **Comments describe the code** as it is: what it does and why. They never name the plan: no task's identifier, no phase, no ADR's number, no section of `docs/`. A link to an external specification, such as xterm's, Unicode's or Apple's, is welcome. `just check-comments` catches the plan's names; a review catches the rest.
- **`unsafe`** only in the crates that the conventions list, such as the platform's, the renderer's, the fonts' and the PTY's, and each block with a `// SAFETY:` comment that tells why it holds. The other crates forbid it.
- **Errors**: `thiserror` in the libraries, `anyhow` in the binaries.
- **Dependencies**: the workspace's `Cargo.toml` names them all; a new one needs a reason, a license that `deny.toml` allows, and a place that the dependency rules accept. The app contains no networking code: downloads are Sleip's.
- **Formatting**: `cargo fmt`, with the workspace's `rustfmt.toml`.

## Tests

- **New code has tests**, and a fix has the test that would have caught the bug.
- **The platform's tests** in `crates/platform-macos/tests` run without the test harness, on the main thread, as AppKit needs: each is a program that starts the application, opens its windows and quits. They hide their app at once, but keys typed while they start can still reach their window, and a click or a key while a context menu of theirs is open closes it: run them again if one fails with keys that it did not send, or with a menu that closed before its time. `alert_keys` pastes and copies through the system's clipboard, whose items it puts back as it ends.
- **The golden images** in `crates/app/tests/golden` are what the GPU draws of text, colors, decorations, the cursor and the selection, at scales of 1 and 2, in SF Mono, and of the shapes that the renderer draws itself, without text: `crates/app/tests/golden.rs` compares a drawing with them, and a pixel can differ by a little, as the GPU and the rasterizer round; a few pixels of the shapes alone. A test that fails writes the drawing, and a map of the pixels that differ, under `target/tmp/golden`. A change that is meant to change the drawing comes with new images: `LOKYR_GOLDEN_UPDATE=1 cargo test -p lokyr-app --test golden` writes them, which you look at before you commit them; a new release of macOS that draws a font otherwise can need them too. They are drawn on a system whose preferred languages hold none of Chinese, Japanese and Korean: CoreText's fallback for CJK text takes its font from the first of them in the list, so that a Mac that has one there, as an input method's setup can add, draws the scenes of CJK text in another font, and fails them, as CI's runner does not.
- **Checks by hand**: what no test can check, such as an input method, a drawing next to another terminal's or a measure, is told in the pull request, with what you saw.

## The documents

- **A changed decision** gets a new ADR, which supersedes the old one, whose status line says so; an accepted ADR is never rewritten. The tables of [docs/README.md](docs/README.md) and [docs/adr/README.md](docs/adr/README.md) follow.
- **A key of the config** that a change adds, changes or removes is in [docs/CONFIGURATION.md](docs/CONFIGURATION.md), the users' guide: the platforms where it applies, in a small line under its name, `<sub>Platforms: macOS</sub>`; its values, its default, what it does, and whether it applies yet. The guide's complete example holds every key, which a test of the kernel checks. The guide names no other terminal emulator; it can name the tools that help, such as vim, Font Book on macOS or GNU Stow.
- **A changed behavior** changes the documents that describe it.
- **Links resolve**, which `just check-links` checks.

## Commits

- **One logical change a commit**, which builds and passes its tests on its own.
- **The message**: a summary line, then a blank line, then a body that tells what changed and why, in English. A planned task's commit starts with its identifier, as `F2-71: review fixes, the cursor's colors`.

## Pull requests

- Open it from a branch of your fork, against `main`, and fill in the [template](.github/pull_request_template.md).
- Keep it to one subject: a smaller pull request is reviewed sooner.
- CI must be green. A reviewer may ask for changes; push them as new commits, which are squashed or kept as the review decides.

## Translations

A translation, a new language or a better one, follows [TRANSLATING.md](TRANSLATING.md): the catalogs, their format, and the steps of a new language.

## Reporting a bug

Open an issue with the bug's template: the macOS version, the Mac, Lokyr's commit, your config, the steps that make the bug happen, and what you expected. A security problem goes privately to the maintainer, through GitHub's private vulnerability reporting, not in a public issue.

## License

Lokyr is under the [MIT license](LICENSE). By contributing, you agree that your contributions are under the same license.
