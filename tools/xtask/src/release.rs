//! `cargo xtask release [<step>] <version>`: a candidate, `X.Y.Z-rc.N`, or
//! a release, `X.Y.Z`, of the crate's numbers, from the commit checked out,
//! which GitHub has, in steps that the CI runs apart and `just release`
//! runs in a row, `check`, `build` and `draft`, the maintainer publishing:
//!
//! - `check`, of no secret, as a release's pull request runs it: its notes
//!   and their listing, `CHANGELOG.md`'s section of the format that
//!   `check-changelog` checks, a release's pages of no mark of what no
//!   release has yet, the checkout as committed and on GitHub, the version
//!   not released, and the build's number, of the commit's moment, above
//!   the feeds' latest, as Sparkle tells a newer version by it: the feed
//!   of tests, if it is there, and the public one, the latest release's
//!   asset, of which a feed's first release goes `--without-feed`; of no
//!   version, the crate's, as `build`'s;
//! - `build`, of the signing identity and Apple's notary: the bundle built
//!   as that version, through `LOKYR_RELEASE`, its number checked again,
//!   the binary's dSYM of the binary's UUID, the bill of materials, and the
//!   DMG, the bundle and the image notarized and stapled, all in
//!   `target/releases/<version>`;
//! - `draft`, of the EdDSA key: the draft, `v<version>`, of the commit, a
//!   candidate's a pre-release, in place of a draft of that tag, never of a
//!   published release, of the DMG, the dSYM and the bill, its notes
//!   `docs/releases/<X.Y.Z>.md`; the DMG in the feed of tests, if it is
//!   there; a release's DMG in the public feed, its deltas and the appcast
//!   assets of the draft, which the site serves once the release is the
//!   latest; a release's Homebrew cask, of the DMG's SHA-256; and
//!   `target/releases/<version>/draft.md`, the draft's address, the DMG's
//!   size and the deltas', which the CI tells the release's pull request;
//! - `publish [--target <commit>] [--rehearsal]`: the draft, a draft
//!   alone, moved to `<commit>`, as the merge of the release's pull request,
//!   and published, its tag made there, a release the latest; a rehearsal's
//!   a pre-release, never the latest, so that the feed, the site and the
//!   updates keep the latest release.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};

use crate::{bundle, cask, feed, sbom, sparkle};

/// The repository of the releases.
const REPOSITORY: &str = "sephiroth74/lokyr";

/// The feed's appcast, relative to the workspace root.
const APPCAST: &str = "private/feed/appcast.xml";

/// What `release` takes.
const USAGE: &str = "release takes a step, `check`, `build`, `draft` or `publish`, if one alone, the version, X.Y.Z-rc.N or X.Y.Z (the crate's of `check` and `build` without one), and `--without-feed`; `publish` takes `--target <commit>` and `--rehearsal`";

pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let args: Vec<String> = args.collect();
    let Options {
        step,
        version,
        without_feed,
        target,
        rehearsal,
    } = parse(&args, env!("CARGO_PKG_VERSION"))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    // The version is of the crate's numbers, as the build takes it.
    bundle::version::version(env!("CARGO_PKG_VERSION"), Some(&version), 0)
        .map_err(anyhow::Error::msg)?;
    let release = Release {
        root,
        version,
        without_feed,
        target,
        rehearsal,
    };
    match step {
        Some(Step::Check) => release.check()?,
        Some(Step::Build) => release.build()?,
        Some(Step::Draft) => release.draft()?,
        Some(Step::Publish) => release.publish()?,
        None => {
            release.check()?;
            release.build()?;
            release.draft()?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// A step of a release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Check,
    Build,
    Draft,
    Publish,
}

/// What `release` was asked.
#[derive(Debug, PartialEq, Eq)]
struct Options {
    /// The step, if one alone.
    step: Option<Step>,
    /// The version.
    version: String,
    /// Whether it goes without the feed, as a feed's first release.
    without_feed: bool,
    /// The commit that `publish` moves the draft to.
    target: Option<String>,
    /// Whether `publish` rehearses, of a pre-release never the latest.
    rehearsal: bool,
}

/// What `args` ask: of `check` and `build` without a version, the crate's,
/// `crate_version`; `--target` and `--rehearsal` of `publish` alone.
fn parse(args: &[String], crate_version: &str) -> Result<Options> {
    let (step, rest) = match args.first().map(String::as_str) {
        Some("check") => (Some(Step::Check), &args[1..]),
        Some("build") => (Some(Step::Build), &args[1..]),
        Some("draft") => (Some(Step::Draft), &args[1..]),
        Some("publish") => (Some(Step::Publish), &args[1..]),
        _ => (None, args),
    };
    let publishing = step == Some(Step::Publish);
    let mut version = None;
    let mut without_feed = false;
    let mut target = None;
    let mut rehearsal = false;
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--without-feed" => without_feed = true,
            "--target" if publishing => {
                let commit = rest
                    .next()
                    .with_context(|| format!("{USAGE}: `--target` of no commit"))?;
                ensure!(
                    !commit.is_empty() && commit.bytes().all(|byte| byte.is_ascii_hexdigit()),
                    "{USAGE}: `{commit}` is no commit's hash"
                );
                target = Some(commit.to_owned());
            }
            "--rehearsal" if publishing => rehearsal = true,
            other if other.starts_with('-') || version.is_some() => bail!("{USAGE}, not `{other}`"),
            other => version = Some(other.to_owned()),
        }
    }
    let version = match (version, step) {
        (Some(version), _) => version,
        (None, Some(Step::Check | Step::Build)) => crate_version.to_owned(),
        (None, _) => bail!("{USAGE}"),
    };
    Ok(Options {
        step,
        version,
        without_feed,
        target,
        rehearsal,
    })
}

/// A release, or a candidate, of its version, from the workspace at
/// `root`.
struct Release {
    root: PathBuf,
    version: String,
    without_feed: bool,
    /// The commit that `publish` moves the draft to.
    target: Option<String>,
    /// Whether `publish` rehearses, of a pre-release never the latest.
    rehearsal: bool,
}

impl Release {
    /// Its tag, `v<version>`.
    fn tag(&self) -> String {
        format!("v{}", self.version)
    }

    /// Whether it is a candidate, of a suffix.
    fn is_candidate(&self) -> bool {
        self.version.contains('-')
    }

    /// The folder of its assets, `target/releases/<version>`.
    fn out(&self) -> PathBuf {
        self.root.join("target/releases").join(&self.version)
    }

    /// Its DMG, which `build` makes.
    fn dmg(&self) -> PathBuf {
        self.root
            .join("target")
            .join(format!("{}-{}.dmg", bundle::NAME, self.version))
    }

    /// Its dSYM's archive, which `build` makes.
    fn symbols(&self) -> PathBuf {
        self.out()
            .join(format!("{}-{}.dSYM.zip", bundle::NAME, self.version))
    }

    /// Its bill of materials, which `build` makes.
    fn bill(&self) -> PathBuf {
        self.out()
            .join(format!("{}-{}.cdx.json", bundle::NAME, self.version))
    }

    /// Its notes, `docs/releases/<X.Y.Z>.md`.
    fn notes(&self) -> PathBuf {
        self.root
            .join("docs/releases")
            .join(format!("{}.md", feed::release_of(&self.version)))
    }

    /// The commit checked out, as GitHub has it.
    fn commit(&self) -> Result<String> {
        Ok(output(
            Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(&self.root),
            "reading the commit",
        )?
        .trim()
        .to_owned())
    }

    /// The feeds' appcasts, the tests' if it is there and the public one,
    /// as one text; none of neither.
    fn appcasts(&self) -> Option<String> {
        let appcasts: Vec<String> = fs::read_to_string(self.root.join(APPCAST))
            .ok()
            .into_iter()
            .chain(feed::published_appcast(&self.root))
            .collect();
        (!appcasts.is_empty()).then(|| appcasts.join("\n"))
    }

    /// What a release takes before anything is built, of no secret.
    fn check(&self) -> Result<()> {
        epoch_unset(std::env::var_os("SOURCE_DATE_EPOCH").as_deref())?;
        let notes = self.notes();
        ensure!(
            notes.is_file(),
            "{} is not there: write the release's notes first",
            notes.display()
        );
        let release = feed::release_of(&self.version);
        let listed = fs::read_to_string(self.root.join("docs/releases/README.md"))
            .context("reading docs/releases/README.md")?;
        ensure!(
            lists_notes(&listed, release),
            "docs/releases/README.md does not list {release}'s notes: add `- [{release}]({release}.md), <date>: <what it brought>` at the top of its list"
        );
        let changelog =
            fs::read_to_string(self.root.join("CHANGELOG.md")).context("reading CHANGELOG.md")?;
        ensure!(
            crate::changelog::has_release(&changelog, release),
            "CHANGELOG.md has no `## [{release}] - YYYY-MM-DD`: move its Unreleased entries there first"
        );
        crate::changelog::check(&changelog).map_err(anyhow::Error::msg)?;
        // A release's pages tell what it has as released: a candidate's keep
        // their marks, as the release is still to come.
        if !self.is_candidate() {
            let left = crate::since::left(&self.root)?;
            ensure!(
                left.is_empty(),
                "these still say `not released yet`: {}; run `cargo xtask since {}` first, in the commit that prepares the release",
                left.join(", "),
                self.version
            );
        }
        let changed = output(
            Command::new("git")
                .args(["status", "--porcelain"])
                .current_dir(&self.root),
            "reading the checkout's state",
        )?;
        ensure!(
            changed.trim().is_empty(),
            "the checkout has changes: a release is of a commit, as it is"
        );
        let commit = self.commit()?;
        ensure!(
            Command::new("gh")
                .args(["api", "--silent"])
                .arg(format!("repos/{REPOSITORY}/commits/{commit}"))
                .status()
                .is_ok_and(|status| status.success()),
            "GitHub does not have the commit {commit}: push it first"
        );
        let tag = self.tag();
        let released = Command::new("gh")
            .args(["release", "view", &tag, "--repo", REPOSITORY])
            .output()
            .context("asking GitHub for the release")?;
        ensure!(
            !released.status.success(),
            "{tag} has a release on GitHub already"
        );
        // The number that the build will take, of the commit's moment.
        let number: u64 = bundle::version::stamp(bundle::build_moment(&self.root)?)
            .parse()
            .context("the build's number")?;
        above_the_feed(number, self.appcasts().as_deref(), self.without_feed)?;
        println!("release: {tag} of {commit}, build {number}, passes its checks");
        Ok(())
    }

    /// The bundle, the DMG, the dSYM and the bill, of the signing identity
    /// and Apple's notary.
    fn build(&self) -> Result<()> {
        epoch_unset(std::env::var_os("SOURCE_DATE_EPOCH").as_deref())?;
        let root = &self.root;
        status(
            Command::new(env!("CARGO"))
                .args(["xtask", "bundle"])
                .env("LOKYR_RELEASE", &self.version)
                .current_dir(root),
            "building the bundle",
        )?;
        let app = root.join("target").join(format!("{}.app", bundle::NAME));
        let info = fs::read_to_string(app.join("Contents/Info.plist"))
            .context("reading the Info.plist")?;
        let number: u64 = plist_string(&info, "CFBundleVersion")
            .and_then(|number| number.parse().ok())
            .context("the bundle's CFBundleVersion")?;
        above_the_feed(number, self.appcasts().as_deref(), self.without_feed)?;
        let out = self.out();
        if out.exists() {
            fs::remove_dir_all(&out).with_context(|| format!("removing {}", out.display()))?;
        }
        fs::create_dir_all(&out).with_context(|| format!("making {}", out.display()))?;
        let dsym = root
            .join("target/release")
            .join(format!("{}.dSYM", bundle::EXECUTABLE));
        let binary = app.join("Contents/MacOS").join(bundle::EXECUTABLE);
        let (dsym_uuid, binary_uuid) = (uuid(&dsym)?, uuid(&binary)?);
        ensure!(
            dsym_uuid == binary_uuid,
            "{} is of {dsym_uuid}, not of the bundle's binary, {binary_uuid}",
            dsym.display()
        );
        status(
            Command::new("ditto")
                .args(["-c", "-k", "--keepParent"])
                .arg(&dsym)
                .arg(self.symbols()),
            "zipping the dSYM",
        )?;
        let crates = sbom::crates(root)?;
        fs::write(
            self.bill(),
            serde_json::to_string_pretty(&sbom::bill(&self.version, &crates, sparkle::VERSION))?,
        )
        .context("writing the bill of materials")?;
        status(
            Command::new(env!("CARGO"))
                .args(["xtask", "dmg"])
                .current_dir(root),
            "building the DMG",
        )?;
        let dmg = self.dmg();
        ensure!(dmg.is_file(), "{} is not there", dmg.display());
        println!(
            "release: {}'s DMG, dSYM and bill, of build {number}, in {}",
            self.tag(),
            out.display()
        );
        Ok(())
    }

    /// The draft of the release, of what `build` made, of the EdDSA key.
    fn draft(&self) -> Result<()> {
        let root = &self.root;
        let (dmg, symbols, bill) = (self.dmg(), self.symbols(), self.bill());
        for asset in [&dmg, &symbols, &bill] {
            ensure!(
                asset.is_file(),
                "{} is not there: run `cargo xtask release build {}` first",
                asset.display(),
                self.version
            );
        }
        let commit = self.commit()?;
        let tag = self.tag();
        self.replace_draft()?;
        status(
            Command::new("gh").args(draft_arguments(
                &self.version,
                &commit,
                &self.notes(),
                &[dmg.clone(), symbols, bill],
            )),
            "making the draft of the release",
        )?;
        // The feed of tests, of the maintainer's Mac alone.
        if root.join(APPCAST).is_file() {
            status(
                Command::new(env!("CARGO"))
                    .args(["xtask", "feed", "add"])
                    .arg(&dmg)
                    .current_dir(root),
                "adding the DMG to the feed of tests",
            )?;
        }
        // A candidate stays out of the public feed and has no cask: Homebrew
        // takes the releases alone.
        if self.is_candidate() {
            self.summarize(&dmg, &[])?;
            println!(
                "release: {tag}'s draft is on GitHub, of {commit}; publish it, then `git fetch --tags`"
            );
            return Ok(());
        }
        let previous = feed::published_appcast(root);
        let published = feed::publish(root, &dmg, previous.as_deref())?;
        status(
            Command::new("gh")
                .args(["release", "upload", &tag, "--repo", REPOSITORY])
                .args(&published.deltas)
                .arg(&published.appcast),
            "uploading the deltas and the appcast to the draft",
        )?;
        for written in cask::write(root, &self.version, None)? {
            println!("release: the cask, {}", written.display());
        }
        self.summarize(&dmg, &published.deltas)?;
        println!(
            "release: {tag}'s draft is on GitHub, of {commit}, of its appcast; publish it, then deploy the site, `git fetch --tags`, and push the tap's cask"
        );
        Ok(())
    }

    /// Deletes the draft of the release's tag, if any, which a new draft
    /// replaces, as a pull request's new commit drafts the release again;
    /// fails of a published release of that tag.
    fn replace_draft(&self) -> Result<()> {
        let tag = self.tag();
        let told = Command::new("gh")
            .args(["release", "view", &tag, "--repo", REPOSITORY])
            .args(["--json", "isDraft", "--jq", ".isDraft"])
            .output()
            .context("looking for the release on GitHub")?;
        // None of that tag: `gh` fails, as it finds no release.
        if !told.status.success() {
            return Ok(());
        }
        let draft = String::from_utf8_lossy(&told.stdout).trim() == "true";
        ensure!(draft, "{tag} is released already: release another version");
        println!("release: replacing {tag}'s draft");
        status(
            Command::new("gh").args(["release", "delete", &tag, "--repo", REPOSITORY, "--yes"]),
            "deleting the release's former draft",
        )
    }

    /// Writes `draft.md` beside the release's assets: the draft's address,
    /// the size of its `dmg` and of its `deltas`, which the CI tells the
    /// release's pull request.
    fn summarize(&self, dmg: &Path, deltas: &[PathBuf]) -> Result<()> {
        let tag = self.tag();
        let address = output(
            Command::new("gh")
                .args(["release", "view", &tag, "--repo", REPOSITORY])
                .args(["--json", "url", "--jq", ".url"]),
            "reading the draft's address",
        )?;
        let deltas: Vec<(String, u64)> = deltas
            .iter()
            .map(|delta| {
                let name = delta
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                (name, bundle::size_of(delta))
            })
            .collect();
        let text = summary(&tag, address.trim(), bundle::size_of(dmg), &deltas);
        let file = self.out().join("draft.md");
        fs::write(&file, text).with_context(|| format!("writing {}", file.display()))
    }

    /// The draft, a draft alone, moved to the target if any, and
    /// published, its tag made there: a candidate a pre-release, a release
    /// the latest; a rehearsal's a pre-release, never the latest.
    fn publish(&self) -> Result<()> {
        let tag = self.tag();
        let draft = output(
            Command::new("gh")
                .args(["release", "view", &tag, "--repo", REPOSITORY])
                .args(["--json", "isDraft", "--jq", ".isDraft"]),
            "looking for the release's draft",
        )?;
        ensure!(
            draft.trim() == "true",
            "{tag} is no draft: published already, or of no draft"
        );
        if let Some(target) = &self.target {
            status(
                Command::new("gh")
                    .args(["release", "edit", &tag, "--repo", REPOSITORY])
                    .args(["--target", target]),
                "moving the draft to its commit",
            )?;
        }
        let mut command = Command::new("gh");
        command.args([
            "release",
            "edit",
            &tag,
            "--repo",
            REPOSITORY,
            "--draft=false",
        ]);
        command.args(publish_flags(self.is_candidate(), self.rehearsal));
        status(&mut command, "publishing the release")?;
        println!(
            "release: {tag} published{}",
            if self.rehearsal {
                ", a rehearsal's pre-release"
            } else {
                ""
            }
        );
        Ok(())
    }
}

/// The flags of `gh release edit` that publish a release: a rehearsal's a
/// pre-release that is never the latest, a candidate a pre-release, a
/// release the latest.
fn publish_flags(candidate: bool, rehearsal: bool) -> &'static [&'static str] {
    match (rehearsal, candidate) {
        (true, _) => &["--prerelease", "--latest=false"],
        (false, true) => &["--prerelease"],
        (false, false) => &["--latest"],
    }
}

/// The draft's summary, in Markdown, of its `tag`, its `address`, its DMG's
/// size `dmg` and its `deltas`, each of its name and size.
fn summary(tag: &str, address: &str, dmg: u64, deltas: &[(String, u64)]) -> String {
    let mut text = format!(
        "**{tag}** is drafted: [its draft]({address}).\n\n- The DMG: {}.\n",
        bundle::megabytes(dmg)
    );
    if deltas.is_empty() {
        text.push_str("- No deltas: a candidate stays out of the public feed.\n");
    } else {
        text.push_str(&format!("- {} deltas:\n", deltas.len()));
        for (name, size) in deltas {
            text.push_str(&format!("  - `{name}`, {}\n", bundle::megabytes(*size)));
        }
    }
    text
}

/// Whether `list`, the release notes' list, `docs/releases/README.md`'s,
/// has an item that links the notes of `release`.
fn lists_notes(list: &str, release: &str) -> bool {
    let link = format!("- [{release}]({release}.md)");
    list.lines()
        .any(|line| line.trim_start().starts_with(&link))
}

/// Fails where `SOURCE_DATE_EPOCH` is set, to `epoch`, which would give the
/// build its number in place of the commit's.
fn epoch_unset(epoch: Option<&std::ffi::OsStr>) -> Result<()> {
    ensure!(
        epoch.is_none(),
        "SOURCE_DATE_EPOCH is set, which would give the build its number in place of the commit's: unset it"
    );
    Ok(())
}

/// Fails unless the build's `number` is above the latest of the feed's
/// `appcast`, as Sparkle tells a newer version by it; or where the feed is
/// not there, or tells no number, unless `without_feed`, as a feed's
/// first release.
fn above_the_feed(number: u64, appcast: Option<&str>, without_feed: bool) -> Result<()> {
    match appcast.and_then(latest_build) {
        Some(latest) => ensure!(
            number > latest,
            "the build's number, {number}, is not above the feed's latest, {latest}: build the release of a commit of its own, after its last candidate's"
        ),
        None => ensure!(
            without_feed,
            "the feed, {APPCAST}, is not there or tells no build's number, against which the release's must be checked: `--without-feed` releases without it, as a feed's first release"
        ),
    }
    Ok(())
}

/// The arguments of `gh` that make the draft of `version`'s release, of
/// `commit`, its notes of the file `notes`, with `assets`: a candidate's is
/// a pre-release.
fn draft_arguments(version: &str, commit: &str, notes: &Path, assets: &[PathBuf]) -> Vec<String> {
    let mut arguments = vec![
        "release".to_owned(),
        "create".to_owned(),
        format!("v{version}"),
        "--repo".to_owned(),
        REPOSITORY.to_owned(),
        "--draft".to_owned(),
        "--target".to_owned(),
        commit.to_owned(),
        "--title".to_owned(),
        format!("{} {version}", bundle::NAME),
        "--notes-file".to_owned(),
        notes.display().to_string(),
    ];
    if version.contains("-rc.") {
        arguments.push("--prerelease".to_owned());
    }
    arguments.extend(assets.iter().map(|asset| asset.display().to_string()));
    arguments
}

/// The string of `key` in the plist's `text`, as the bundle writes it.
fn plist_string<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let after = text.split_once(&format!("<key>{key}</key>"))?.1;
    let value = after.trim_start().strip_prefix("<string>")?;
    value.split_once("</string>").map(|(value, _)| value)
}

/// The highest build's number of the appcast's `text`, its items'
/// `sparkle:version`.
fn latest_build(text: &str) -> Option<u64> {
    text.split("<sparkle:version>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</sparkle:version>")?.0.trim().parse().ok())
        .max()
}

/// The UUID of the arm64 code of the binary or the dSYM at `path`, as
/// `dwarfdump --uuid` tells it.
fn uuid(path: &Path) -> Result<String> {
    let told = output(
        Command::new("dwarfdump").arg("--uuid").arg(path),
        &format!("reading the UUID of {}", path.display()),
    )?;
    arm64_uuid(&told).with_context(|| format!("dwarfdump told no arm64 UUID of {}", path.display()))
}

/// The UUID of the arm64 code of `dwarfdump --uuid`'s `told`.
fn arm64_uuid(told: &str) -> Option<String> {
    told.lines().find_map(|line| {
        let rest = line.strip_prefix("UUID: ")?;
        let (uuid, rest) = rest.split_once(' ')?;
        rest.starts_with("(arm64)").then(|| uuid.to_owned())
    })
}

/// Runs `command`, which `what` describes, and fails if it fails.
fn status(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| what.to_owned())?;
    if !status.success() {
        bail!("{what} failed: {status}");
    }
    Ok(())
}

/// Runs `command`, which `what` describes, and gives its output; fails if
/// it fails.
fn output(command: &mut Command, what: &str) -> Result<String> {
    let output = command.output().with_context(|| what.to_owned())?;
    if !output.status.success() {
        bail!(
            "{what} failed: {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rehearsal_is_never_the_latest() {
        assert_eq!(publish_flags(false, false), ["--latest"]);
        assert_eq!(publish_flags(true, false), ["--prerelease"]);
        assert_eq!(
            publish_flags(false, true),
            ["--prerelease", "--latest=false"]
        );
        assert_eq!(
            publish_flags(true, true),
            ["--prerelease", "--latest=false"]
        );
    }

    #[test]
    fn the_draft_s_summary_tells_its_address_and_sizes() {
        let text = summary(
            "v1.6.0",
            "https://github.com/o/r/releases/tag/untagged-1",
            19_200_000,
            &[("Lokyr2-1.delta".to_owned(), 2_500_000)],
        );
        assert!(text.contains(
            "**v1.6.0** is drafted: [its draft](https://github.com/o/r/releases/tag/untagged-1)"
        ));
        assert!(text.contains("- The DMG: 19.2 MB."));
        assert!(text.contains("- 1 deltas:\n  - `Lokyr2-1.delta`, 2.5 MB"));
        assert!(summary("v1.6.0-rc.1", "a", 1, &[]).contains("No deltas"));
    }

    #[test]
    fn a_release_takes_a_step_alone_or_all_of_them_and_check_the_crate_s_version() {
        let args =
            |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_owned).collect() };
        let options = |step, version: &str, without_feed| Options {
            step,
            version: version.to_owned(),
            without_feed,
            target: None,
            rehearsal: false,
        };
        assert_eq!(
            parse(&args("1.6.0"), "1.6.0").unwrap(),
            options(None, "1.6.0", false)
        );
        assert_eq!(
            parse(&args("check"), "1.6.0").unwrap(),
            options(Some(Step::Check), "1.6.0", false)
        );
        assert_eq!(
            parse(&args("build 1.6.0-rc.1 --without-feed"), "1.6.0").unwrap(),
            options(Some(Step::Build), "1.6.0-rc.1", true)
        );
        // A step of no version but `check` and `build`, and anything else,
        // refused.
        assert_eq!(
            parse(&args("build"), "1.6.0").unwrap(),
            options(Some(Step::Build), "1.6.0", false)
        );
        assert_eq!(
            parse(
                &args("publish 1.6.0 --target 0123abcd --rehearsal"),
                "1.6.0"
            )
            .unwrap(),
            Options {
                target: Some("0123abcd".to_owned()),
                rehearsal: true,
                ..options(Some(Step::Publish), "1.6.0", false)
            }
        );
        assert!(parse(&args("publish 1.6.0 --target"), "1.6.0").is_err());
        assert!(parse(&args("publish 1.6.0 --target main;rm"), "1.6.0").is_err());
        assert!(parse(&args("draft 1.6.0 --rehearsal"), "1.6.0").is_err());
        assert!(parse(&args("draft"), "1.6.0").is_err());
        assert!(parse(&args(""), "1.6.0").is_err());
        assert!(parse(&args("1.6.0 1.6.1"), "1.6.0").is_err());
        assert!(parse(&args("check --fast"), "1.6.0").is_err());
    }

    #[test]
    fn the_notes_list_names_a_release_of_its_item() {
        let list = "# Release notes\n\n- [1.2.0](1.2.0.md), 16 November 2026: themes.\n- [1.1.0](1.1.0.md), 14 November 2026: settings.\n";
        assert!(lists_notes(list, "1.2.0"));
        assert!(lists_notes(list, "1.1.0"));
        assert!(!lists_notes(list, "1.3.0"));
        assert!(!lists_notes("See [1.2.0](1.2.0.md).", "1.2.0"));
    }

    #[test]
    fn the_feed_s_latest_build_is_its_highest_number() {
        let appcast = "<item><sparkle:version>202611021148</sparkle:version></item>\
            <item><sparkle:version>202611030900</sparkle:version></item>\
            <item><sparkle:version>202611011000</sparkle:version></item>";
        assert_eq!(latest_build(appcast), Some(202_611_030_900));
        assert_eq!(latest_build("<rss></rss>"), None);
    }

    #[test]
    fn a_release_is_above_the_feed_s_latest_or_told_to_go_without_it() {
        let appcast = "<item><sparkle:version>202611021148</sparkle:version></item>";
        assert!(above_the_feed(202_611_021_200, Some(appcast), false).is_ok());
        // The last candidate's own number.
        assert!(above_the_feed(202_611_021_148, Some(appcast), false).is_err());
        // No feed, or none of a number: refused, unless told.
        assert!(above_the_feed(202_611_021_200, None, false).is_err());
        assert!(above_the_feed(202_611_021_200, Some("<rss></rss>"), false).is_err());
        assert!(above_the_feed(202_611_021_200, None, true).is_ok());
        // Told, a number below the feed's latest is still refused.
        assert!(above_the_feed(202_611_021_100, Some(appcast), true).is_err());
    }

    #[test]
    fn a_release_refuses_a_source_date_epoch_left_set() {
        assert!(epoch_unset(None).is_ok());
        assert!(epoch_unset(Some(std::ffi::OsStr::new("1793625907"))).is_err());
    }

    #[test]
    fn the_plist_gives_its_strings() {
        let plist = "<dict>\n\t<key>CFBundleShortVersionString</key>\n\t<string>0.3.0</string>\n\
            \t<key>CFBundleVersion</key>\n\t<string>202611021148</string>\n</dict>";
        assert_eq!(plist_string(plist, "CFBundleVersion"), Some("202611021148"));
        assert_eq!(
            plist_string(plist, "CFBundleShortVersionString"),
            Some("0.3.0")
        );
        assert_eq!(plist_string(plist, "Missing"), None);
    }

    #[test]
    fn dwarfdump_s_uuid_of_arm64_is_read() {
        let told = "UUID: 0D7C1F2A-0000-1111-2222-333344445555 (arm64) target/release/lokyr.dSYM/Contents/Resources/DWARF/lokyr\n";
        assert_eq!(
            arm64_uuid(told).as_deref(),
            Some("0D7C1F2A-0000-1111-2222-333344445555")
        );
        assert_eq!(arm64_uuid("UUID: AAAA (x86_64) lokyr\n"), None);
    }

    #[test]
    fn a_candidate_s_draft_is_a_pre_release_and_a_release_s_is_not() {
        let assets = [PathBuf::from("target/Lokyr-0.3.0-rc.1.dmg")];
        let notes = Path::new("docs/releases/0.3.0.md");
        let candidate = draft_arguments("0.3.0-rc.1", "abc123", notes, &assets);
        assert_eq!(candidate[..3], ["release", "create", "v0.3.0-rc.1"]);
        assert!(candidate.contains(&"--draft".to_owned()));
        assert!(candidate.contains(&"--prerelease".to_owned()));
        assert!(candidate.contains(&"abc123".to_owned()));
        assert_eq!(candidate.last().unwrap(), "target/Lokyr-0.3.0-rc.1.dmg");
        let release = draft_arguments("0.3.0", "abc123", notes, &assets);
        assert!(!release.contains(&"--prerelease".to_owned()));
        assert!(release.contains(&"Lokyr 0.3.0".to_owned()));
    }
}
