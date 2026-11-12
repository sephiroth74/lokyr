//! `cargo xtask release <version>`: a candidate, `X.Y.Z-rc.N`, or a
//! release, `X.Y.Z`, of the crate's numbers, from the commit checked out,
//! which GitHub has, to a draft of its GitHub release, which the
//! maintainer publishes; on the maintainer's Mac, which signs and has
//! Apple notarize:
//!
//! 1. the bundle, built as that version, through `LOKYR_RELEASE`;
//! 2. its build's number above the feed's latest, as Sparkle tells a newer
//!    version by it, and a release built of its last candidate's commit
//!    would have the candidate's: the feed of tests must be there, unless
//!    `--without-feed`, as a feed's first release; a `SOURCE_DATE_EPOCH`
//!    set, which would give the number, is refused;
//! 3. the binary's dSYM, of the binary's UUID, and the bill of materials,
//!    kept in `target/releases/<version>`;
//! 4. the DMG, the bundle and the image notarized and stapled;
//! 5. the draft, `v<version>`, of the commit, a candidate's a pre-release,
//!    with the DMG, the dSYM and the bill, its notes
//!    `docs/releases/<X.Y.Z>.md`, and `CHANGELOG.md` its section, of the
//!    format that `check-changelog` checks;
//! 6. the DMG in the feed of tests;
//! 7. a release's DMG in the public feed, `site/public/appcast.xml`, its
//!    deltas uploaded to the draft, the appcast to commit and push once the
//!    release is published, as its DMG is not public before; a candidate
//!    stays out of it;
//! 8. a release's Homebrew cask, of the DMG's SHA-256, in
//!    `target/releases/<version>` and the tap's folder beside the workspace,
//!    to commit and push once the release is published.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};

use crate::{bundle, cask, feed, sbom, sparkle};

/// The repository of the releases.
const REPOSITORY: &str = "sephiroth74/lokyr";

/// The feed's appcast, relative to the workspace root.
const APPCAST: &str = "private/feed/appcast.xml";

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let version = args
        .next()
        .context("release takes the version: a candidate's, X.Y.Z-rc.N, or a release's, X.Y.Z")?;
    let without_feed = match args.next().as_deref() {
        None => false,
        Some("--without-feed") => true,
        Some(arg) => bail!("release takes the version and `--without-feed`, not `{arg}`"),
    };
    if let Some(arg) = args.next() {
        bail!("release takes the version and `--without-feed`, not `{arg}`");
    }
    epoch_unset(std::env::var_os("SOURCE_DATE_EPOCH").as_deref())?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    // The version is of the crate's numbers, as the build takes it.
    bundle::version::version(env!("CARGO_PKG_VERSION"), Some(&version), 0)
        .map_err(anyhow::Error::msg)?;
    let notes = root
        .join("docs/releases")
        .join(format!("{}.md", feed::release_of(&version)));
    ensure!(
        notes.is_file(),
        "{} is not there: write the release's notes first",
        notes.display()
    );
    let release = feed::release_of(&version);
    let changelog =
        fs::read_to_string(root.join("CHANGELOG.md")).context("reading CHANGELOG.md")?;
    ensure!(
        crate::changelog::has_release(&changelog, release),
        "CHANGELOG.md has no `## [{release}] - YYYY-MM-DD`: move its Unreleased entries there first"
    );
    crate::changelog::check(&changelog).map_err(anyhow::Error::msg)?;
    let changed = output(
        Command::new("git").args(["status", "--porcelain"]),
        "reading the checkout's state",
    )?;
    ensure!(
        changed.trim().is_empty(),
        "the checkout has changes: a release is of a commit, as it is"
    );
    let commit = output(
        Command::new("git").args(["rev-parse", "HEAD"]),
        "reading the commit",
    )?
    .trim()
    .to_owned();
    ensure!(
        Command::new("gh")
            .args(["api", "--silent"])
            .arg(format!("repos/{REPOSITORY}/commits/{commit}"))
            .status()
            .is_ok_and(|status| status.success()),
        "GitHub does not have the commit {commit}: push it first"
    );
    let tag = format!("v{version}");
    let released = Command::new("gh")
        .args(["release", "view", &tag, "--repo", REPOSITORY])
        .output()
        .context("asking GitHub for the release")?;
    ensure!(
        !released.status.success(),
        "{tag} has a release on GitHub already"
    );

    status(
        Command::new(env!("CARGO"))
            .args(["xtask", "bundle"])
            .env("LOKYR_RELEASE", &version)
            .current_dir(&root),
        "building the bundle",
    )?;
    let app = root.join("target").join(format!("{}.app", bundle::NAME));
    let info =
        fs::read_to_string(app.join("Contents/Info.plist")).context("reading the Info.plist")?;
    let number: u64 = plist_string(&info, "CFBundleVersion")
        .and_then(|number| number.parse().ok())
        .context("the bundle's CFBundleVersion")?;
    // Above both feeds' latest: the tests' and the public one.
    let appcasts: Vec<String> = [APPCAST, feed::PUBLIC_APPCAST]
        .iter()
        .filter_map(|appcast| fs::read_to_string(root.join(appcast)).ok())
        .collect();
    let appcast = (!appcasts.is_empty()).then(|| appcasts.join("\n"));
    above_the_feed(number, appcast.as_deref(), without_feed)?;

    let out = root.join("target/releases").join(&version);
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
    let symbols = out.join(format!("{}-{version}.dSYM.zip", bundle::NAME));
    status(
        Command::new("ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(&dsym)
            .arg(&symbols),
        "zipping the dSYM",
    )?;
    let bill = out.join(format!("{}-{version}.cdx.json", bundle::NAME));
    let crates = sbom::crates(&root)?;
    fs::write(
        &bill,
        serde_json::to_string_pretty(&sbom::bill(&version, &crates, sparkle::VERSION))?,
    )
    .context("writing the bill of materials")?;

    status(
        Command::new(env!("CARGO"))
            .args(["xtask", "dmg"])
            .current_dir(&root),
        "building the DMG",
    )?;
    let dmg = root
        .join("target")
        .join(format!("{}-{version}.dmg", bundle::NAME));
    ensure!(dmg.is_file(), "{} is not there", dmg.display());

    status(
        Command::new("gh").args(draft_arguments(
            &version,
            &commit,
            &notes,
            &[dmg.clone(), symbols, bill],
        )),
        "making the draft of the release",
    )?;
    status(
        Command::new(env!("CARGO"))
            .args(["xtask", "feed", "add"])
            .arg(&dmg)
            .current_dir(&root),
        "adding the DMG to the feed of tests",
    )?;
    // A candidate stays out of the public feed and has no cask: Homebrew
    // takes the releases alone.
    if version.contains('-') {
        println!(
            "release: {tag}'s draft is on GitHub, of {commit}; publish it, then `git fetch --tags`"
        );
    } else {
        let deltas = feed::publish(&root, &dmg)?;
        if !deltas.is_empty() {
            status(
                Command::new("gh")
                    .args(["release", "upload", &tag, "--repo", REPOSITORY])
                    .args(&deltas),
                "uploading the deltas to the draft",
            )?;
        }
        for written in cask::write(&root, &version, None)? {
            println!("release: the cask, {}", written.display());
        }
        println!(
            "release: {tag}'s draft is on GitHub, of {commit}; publish it, then commit {} and push it, `git fetch --tags`, and push the tap's cask",
            feed::PUBLIC_APPCAST
        );
    }
    Ok(ExitCode::SUCCESS)
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
