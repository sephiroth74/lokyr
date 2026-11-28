# Releasing

Lokyr's releases are made by the CI. The release's pull request into `main` builds, signs, notarizes and drafts the release, and its merge publishes it. This page covers each step, what you still do by hand, and how to release by hand when the CI fails.

## Branches

- **`main`** holds the releases alone. Each one is the merge of a release's branch, tagged `vX.Y.Z` on the merge commit.
- **`release/X.Y`** is the version under way, made from `main` when its work starts. Each feature goes on `feature/<name>` and each fix on `fix/<name>`, both made from `release/X.Y` and merged back into it.
- **A patch release**, `X.Y.Z`, goes the same way, from a `release/X.Y.Z` branch made from `main`.
- **A change to the documentation or the site alone** goes on `hotfix/<name>` from `main`. It is merged into `main` and into the open `release/X.Y`, without a release.

## Prepare the release

Prepare the release in one commit on its branch:

1. Raise the version in `Cargo.toml`, under `[workspace.package]`.
2. Turn the marks of what no release has yet into the release's: `cargo xtask since X.Y.Z`. It changes the marks of the features that no release has, in the pages, in the config's schema and in code blocks, into the release's version.
3. Move `CHANGELOG.md`'s Unreleased entries under `## [X.Y.Z] - YYYY-MM-DD`, and add the release's link at the end.
4. Write the release's notes, `docs/releases/X.Y.Z.md`, and list them at the top of `docs/releases/README.md`. Sparkle's window shows them with the update.
5. Update the home page, `site/public/index.html`: the structured data's `softwareVersion` and `releaseNotes`, the release's pill, the roadmap's line and the measurements' footnote. Update the README's note of the release too.

`cargo xtask release check` runs the release's checks on the Mac, without any secret. A test of the site fails when the home page names another version than the workspace's.

## The pull request

Open a pull request of `release/X.Y` into `main`. The release workflow, `.github/workflows/release.yml`, runs these jobs:

| Job | When it runs | What it does |
|---|---|---|
| `check` | At each push | The release's checks, the same as `cargo xtask release check` |
| `build` | At each push, once you approve it | The DMG, signed and notarized, its dSYM and its bill of materials |
| `draft` | After `build`, once you approve it | The release's draft, its deltas and its feed, told on the pull request |
| `publish` | At the merge, once you approve it | The release published, the site deployed and the cask pushed |

- **`check`** looks at the notes and their listing, the changelog's section, the marks left, that no release has the version already, and the build's number, which must be above the feed's latest. It reads no secret.
- **`build`** builds the bundle as the version and signs it with the Developer ID, imported into a keychain made for the run and deleted at its end. It has Apple notarize the bundle and the DMG, and staples their tickets. The DMG, the dSYM and the bill of materials are an artifact of the run.
- **`draft`** drafts `vX.Y.Z` of the pull request's head, in place of the draft that a former push made. It makes the deltas from the last five releases' DMGs, signs the feed and writes the cask. A comment on the pull request gives the draft's address, the DMG's size and the deltas.

### Approve the jobs

The jobs that read secrets run in the GitHub environment `release`, which waits for your approval: open the run and choose Review deployments. A pull request from a fork never reaches them.

### Make a candidate

While the pull request is a draft, `build` and `draft` make the next candidate, `X.Y.Z-rc.N`, where N is one above the last candidate published. Each push replaces that candidate's draft. A candidate is a pre-release, without a feed, deltas or a cask.

1. Publish the candidate from its draft to try it: `cargo xtask release publish X.Y.Z-rc.N`.
2. Mark the pull request ready for review: `build` and `draft` run again, for the release.

Publishing the release deletes the drafts of its candidates that are left.

### Do these by hand

- **Measure the draft's DMG** before you merge: its startup at rest, and the A/B against the last release, as [the measurements](bench/RELEASES.md) describe.
- **Check the update through Sparkle**, from the last release, once the release is published.
- **Fetch the tag**: `git fetch --tags`.

## The merge

Merge the pull request with a merge commit. Once you approve it, `publish`:

1. Checks that the merge holds the tree that `build` built, which holds when `main` has nothing that the release's branch lacks.
2. Checks that the draft was made of the pull request's last commit, and that it holds its DMG and its feed. A draft of an earlier push, or of a `draft` that failed or still runs, is refused: run `build` and `draft` again, then publish. A `publish` waits for a `draft` that runs.
3. Moves the draft to the merge commit and publishes it as the latest release. Its tag is made there.
4. Asks for a run of the site of `main`, which serves the new release's feed. The merge's own push deploys nothing, so that no copy of the app sees an update before its DMG is public. The site's build, and the release's steps that read the feed, fail rather than go on with an older feed when GitHub doesn't answer, or when the latest release lacks its feed.
5. Pushes the cask to the Homebrew tap.

A step that fails stops the rest, and the run's summary lists what is left to do by hand. You can also run the failed jobs again: a release already published at the merge, and a cask already pushed, pass unchanged.

## Rehearse a release

A release's pull request into `test/main`, a branch made from `main`, rehearses the whole workflow without reaching anyone:

- The release is published as a pre-release that is never the latest, so the feed, the site and Homebrew keep the latest release.
- The site is built and not deployed.
- The cask goes to a branch `test/lokyr-<version>-<run>` of the tap.

Afterwards, delete the pre-release with its tag, `test/main` and the tap's branch. The rules on `v*` tags let an administrator alone delete them:

```sh
gh release delete vX.Y.Z --yes --cleanup-tag
git push origin --delete test/main
```

## The secrets

The environment `release` holds these secrets:

| Secret | What it holds |
|---|---|
| `DEVELOPER_ID_P12` | The Developer ID Application certificate and its private key, a `.p12` in base64 |
| `DEVELOPER_ID_P12_PASSWORD` | The `.p12`'s password |
| `NOTARY_KEY` | An App Store Connect API key, the `.p8` file |
| `NOTARY_KEY_ID` | The API key's id |
| `NOTARY_ISSUER` | The issuer's id of a team's key, left out for an individual key |
| `SPARKLE_EDDSA_KEY` | Sparkle's private EdDSA key, as `generate_keys -x` writes it |
| `TAP_TOKEN` | A token that writes the tap's repository alone |

The environment's rules allow `main`, `release/*` and `refs/pull/*/merge`, because GitHub matches a pull request's job against its merge ref. The jobs check the secrets' forms, and name a wrong one without its value. The logs mask the signing identity, its team and the notarization's ids.

## Release by hand

When the CI fails, run the same steps on the Mac. They use the keychain's Developer ID, the notary's profile and Sparkle's key:

```sh
just release X.Y.Z
cargo xtask release publish X.Y.Z --target <the merge commit> --built <the release's last commit>
```

`just release` runs `check`, `build` and `draft`, and each one runs alone as `cargo xtask release <step> X.Y.Z`. A draft that the CI made is replaced. Then deploy the site, `gh workflow run site.yml --ref main`, and update the cask: `cargo xtask cask X.Y.Z`, then commit and push the tap.
