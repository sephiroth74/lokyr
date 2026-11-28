//! `cargo xtask feed`: the updates' feeds. The feed of tests, which the
//! builds on the maintainer's Mac read once `feed serve` points them at it,
//! is in the untracked `private/feed`: the DMGs, their release notes, and
//! the appcast that Sparkle's `generate_appcast` writes over them, each DMG
//! and the appcast signed with the EdDSA key. The public feed is the
//! site's, below.
//!
//! - `feed add <dmg> [--notes <file>] [--channel <name>]` puts the DMG and
//!   its notes, in Markdown, into the feed, writes the appcast again, and
//!   checks its signature. The notes are `docs/releases/<X.Y.Z>.md` of the
//!   DMG's version, or the file that `--notes` names; `--channel` puts the
//!   item in a channel of Sparkle's, as a beta's.
//! - `feed serve` has Lokyr on this Mac read the feed of tests, through
//!   the updater's `SUFeedURL` in its defaults, which wins over the
//!   bundle's, and serves the feed on `127.0.0.1` until Control-C;
//!   `defaults delete io.github.sephiroth74.lokyr SUFeedURL` gives the
//!   builds their own feed back.
//!
//! - `feed publish <dmg>` puts a release's DMG into the public feed, which
//!   `lokyr.dev/appcast.xml` serves of the latest release's asset and the
//!   bundle's `SUFeedURL` names: `generate_appcast` writes it in the
//!   untracked `private/public-feed`, of the published appcast, and of the
//!   last five released DMGs, downloaded from their GitHub releases where
//!   the folder lacks them, of which it makes the release's deltas; each
//!   item's DMG and deltas at its own GitHub release, `v<X.Y.Z>`, which
//!   `generate_appcast`'s one prefix cannot tell, and the appcast signed
//!   again, into `target/releases/<X.Y.Z>/appcast.xml`. `cargo xtask
//!   release draft` uploads the deltas and the appcast to the release's
//!   draft, the feed once the release is published, as its DMG is not
//!   public before.
//!
//! The key is the keychain's, of the account `lokyr` of Sparkle's
//! `generate_keys`, or the file that `LOKYR_SPARKLE_KEY_FILE` names, in
//! the environment or in `private/lokyr.env`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};

use crate::bundle;

/// The feed's directory, relative to the workspace root.
const DIRECTORY: &str = "private/feed";

/// The public feed's directory, of the released DMGs and their deltas,
/// relative to the workspace root.
const PUBLIC_DIRECTORY: &str = "private/public-feed";

/// The public feed's appcast that the site kept before each release held
/// its own, relative to the workspace root: the feed of a release of none.
pub(crate) const PUBLIC_APPCAST: &str = "site/public/appcast.xml";

/// The latest published release's appcast, an asset of the release.
const LATEST_APPCAST: &str =
    "https://github.com/sephiroth74/lokyr/releases/latest/download/appcast.xml";

/// Where the releases' assets are downloaded from, each of its tag,
/// `v<X.Y.Z>/`.
const RELEASES: &str = "https://github.com/sephiroth74/lokyr/releases/download/";

/// The repository of the releases.
const REPOSITORY: &str = "sephiroth74/lokyr";

/// How many released versions a release's deltas are made from, the
/// latest before it, as `generate_appcast`'s `--maximum-deltas` takes.
const DELTAS_FROM: usize = 5;

/// The release notes, `<X.Y.Z>.md`, relative to the workspace root.
const NOTES: &str = "docs/releases";

/// Where the feed of tests is served.
const ADDRESS: &str = "127.0.0.1:8123";

/// The keychain's account of the EdDSA key.
const ACCOUNT: &str = "lokyr";

/// The variable that names a file of the EdDSA key in place of the
/// keychain's.
const KEY_FILE_VARIABLE: &str = "LOKYR_SPARKLE_KEY_FILE";

/// What marks the appcast's signature, which `generate_appcast` embeds at
/// its end.
const SIGNATURE_MARK: &str = "sparkle-signatures:";

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    match args.next().as_deref() {
        Some("add") => {
            let mut dmg = None;
            let mut notes = None;
            let mut channel = None;
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--notes" => {
                        notes = Some(PathBuf::from(args.next().context("--notes names a file")?));
                    }
                    "--channel" => {
                        channel = Some(args.next().context("--channel names a channel")?);
                    }
                    _ if dmg.is_none() && !arg.starts_with('-') => dmg = Some(PathBuf::from(arg)),
                    _ => bail!(
                        "feed add takes a DMG, `--notes <file>` and `--channel <name>`, not `{arg}`"
                    ),
                }
            }
            let dmg = dmg.context("feed add takes the DMG to add")?;
            add(&root, &dmg, notes.as_deref(), channel.as_deref())?;
        }
        Some("serve") => {
            if let Some(arg) = args.next() {
                bail!("feed serve takes no arguments, not `{arg}`");
            }
            serve(&root)?;
        }
        Some("publish") => {
            let dmg = PathBuf::from(
                args.next()
                    .context("feed publish takes the DMG to publish")?,
            );
            if let Some(arg) = args.next() {
                bail!("feed publish takes the DMG alone, not `{arg}`");
            }
            let previous = published_appcast(&root)?;
            let published = publish(&root, &dmg, previous.as_deref())?;
            for asset in published.deltas.iter().chain([&published.appcast]) {
                println!("feed: {} to upload to its release", asset.display());
            }
        }
        _ => bail!(
            "feed takes `add <dmg> [--notes <file>] [--channel <name>]`, `serve` or `publish <dmg>`"
        ),
    }
    Ok(ExitCode::SUCCESS)
}

/// Adds `dmg`, with its notes, of `notes` or of its release's file, to the
/// feed of tests, in `channel` if any, and writes the appcast again,
/// signed.
fn add(root: &Path, dmg: &Path, notes: Option<&Path>, channel: Option<&str>) -> Result<()> {
    let name = dmg
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no name", dmg.display()))?;
    let version = version_of(name).with_context(|| {
        format!(
            "{name} is not a DMG of `cargo xtask dmg`, `{}-<version>.dmg`",
            bundle::NAME
        )
    })?;
    let notes = match notes {
        Some(notes) => notes.to_owned(),
        None => root.join(NOTES).join(format!("{}.md", release_of(version))),
    };
    ensure!(
        notes.is_file(),
        "{} is not there: write the release's notes in Markdown, or name them with --notes",
        notes.display()
    );
    let directory = root.join(DIRECTORY);
    fs::create_dir_all(&directory).with_context(|| format!("making {}", directory.display()))?;
    copy_unless_same(dmg, &directory.join(name))?;
    let stem = name.trim_end_matches(".dmg");
    copy_unless_same(&notes, &directory.join(format!("{stem}.md")))?;
    let tools = crate::sparkle::release(&root.join("target"))?.join("bin");
    let key = key_arguments(root)?;
    let mut generate = Command::new(tools.join("generate_appcast"));
    generate
        .args(&key)
        .args(["--embed-release-notes", "--download-url-prefix"])
        .arg(format!("http://{ADDRESS}/"));
    if let Some(channel) = channel {
        generate.args(["--channel", channel]);
    }
    status(generate.arg(&directory), "writing the appcast")?;
    let appcast = directory.join("appcast.xml");
    let text =
        fs::read_to_string(&appcast).with_context(|| format!("reading {}", appcast.display()))?;
    ensure!(
        signed(&text),
        "{} has no signature: the DMG's app does not require a signed feed",
        appcast.display()
    );
    status(
        Command::new(tools.join("sign_update"))
            .args(&key)
            .arg("--verify")
            .arg(&appcast),
        "checking the appcast's signature",
    )?;
    println!(
        "feed: {name} added to {}, its appcast signed; `just feed-serve` serves it",
        directory.display()
    );
    Ok(())
}

/// Has Lokyr on this Mac read the feed of tests, and serves it until
/// Control-C.
fn serve(root: &Path) -> Result<()> {
    let directory = root.join(DIRECTORY);
    ensure!(
        directory.join("appcast.xml").is_file(),
        "{} has no appcast: `cargo xtask feed add <dmg>` writes it",
        directory.display()
    );
    status(
        Command::new("defaults")
            .args(["write", bundle::IDENTIFIER, "SUFeedURL"])
            .arg(format!("http://{ADDRESS}/appcast.xml")),
        "pointing Lokyr's updater at the feed of tests",
    )?;
    let (host, port) = ADDRESS.split_once(':').context("the address's port")?;
    println!(
        "feed: Lokyr on this Mac reads http://{ADDRESS}/appcast.xml; serving {} until Control-C",
        directory.display()
    );
    status(
        Command::new("python3")
            .args(["-m", "http.server", port, "--bind", host, "--directory"])
            .arg(&directory),
        "serving the feed",
    )
}

/// The public feed's appcast as it is now: the latest published release's
/// asset, or, of a release before the releases held it, the site's; none
/// of neither. Fails as [`latest_release_appcast`] does.
pub(crate) fn published_appcast(root: &Path) -> Result<Option<String>> {
    Ok(match latest_release_appcast()? {
        Some(appcast) => Some(appcast),
        None => fs::read_to_string(root.join(PUBLIC_APPCAST)).ok(),
    })
}

/// The first release that holds its appcast as an asset: a latest release
/// from it of none is a fault, not a release before the releases held it.
const FIRST_OF_ITS_APPCAST: [u64; 3] = [1, 6, 0];

/// The latest published release's appcast, an asset of the release, as
/// GitHub serves it, tried again of a failure that passes; none of a
/// release before the releases held it, whose feed is the site's. Fails of
/// the network's failure past its tries, of a latest release from
/// [`FIRST_OF_ITS_APPCAST`] of no appcast, and of an answer that is no
/// appcast, so that no feed of an old release is served, or built on, of a
/// failure that tells nothing.
pub(crate) fn latest_release_appcast() -> Result<Option<String>> {
    let body =
        std::env::temp_dir().join(format!("lokyr-latest-appcast-{}.xml", std::process::id()));
    let output = Command::new("curl")
        .args(["--silent", "--show-error", "--location", "--max-time", "60"])
        .args(["--retry", "4", "--retry-delay", "5", "--retry-connrefused"])
        .arg("--output")
        .arg(&body)
        .args(["--write-out", "%{http_code} %{url_effective}"])
        .arg(LATEST_APPCAST)
        .output()
        .context("running curl for the latest release's appcast")?;
    let text = fs::read_to_string(&body).unwrap_or_default();
    let _ = fs::remove_file(&body);
    ensure!(
        output.status.success(),
        "the latest release's appcast could not be fetched: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    latest_of(&String::from_utf8_lossy(&output.stdout), &text)
}

/// The latest release's appcast of curl's `told`, its answer's status and
/// its last address, and of `text`, its answer's body, as
/// [`latest_release_appcast`] tells it.
fn latest_of(told: &str, text: &str) -> Result<Option<String>> {
    let told = told.trim();
    let (status, address) = told.split_once(' ').unwrap_or((told, ""));
    match status {
        "200" => {
            ensure!(
                text.contains("<rss"),
                "the latest release's appcast, of {address}, is no appcast"
            );
            Ok(Some(text.to_owned()))
        }
        // No asset of that name: of the release that the redirect names.
        "404" => {
            let version = address
                .split("/releases/download/v")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .with_context(|| {
                    format!("GitHub found no latest release's appcast, of {address}")
                })?;
            let numbers: Vec<u64> = version
                .split('-')
                .next()
                .unwrap_or_default()
                .split('.')
                .map(str::parse)
                .collect::<Result<_, _>>()
                .with_context(|| format!("`{version}` is no release's version"))?;
            ensure!(
                numbers.as_slice() < FIRST_OF_ITS_APPCAST.as_slice(),
                "the latest release, v{version}, holds no appcast.xml: upload its appcast, `target/releases/{version}/appcast.xml`, to it"
            );
            Ok(None)
        }
        other => bail!("GitHub answered {other} of the latest release's appcast, of {address}"),
    }
}

/// A release put into the public feed: the deltas made for it, and the
/// appcast, signed, both assets of its release.
pub(crate) struct Published {
    pub(crate) deltas: Vec<PathBuf>,
    pub(crate) appcast: PathBuf,
}

/// Puts the release's `dmg` into the public feed, of `previous`, the feed's
/// appcast before it, and the released DMGs that `private/public-feed`
/// keeps; writes the appcast again, each item at its own release and
/// signed, into `target/releases/<version>/appcast.xml`, an asset of the
/// release, which the site serves once the release is the latest; and
/// gives the deltas made for the release, to upload to it.
pub fn publish(root: &Path, dmg: &Path, previous: Option<&str>) -> Result<Published> {
    let name = dmg
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("{} has no name", dmg.display()))?;
    let version = version_of(name).with_context(|| {
        format!(
            "{name} is not a DMG of `cargo xtask dmg`, `{}-<version>.dmg`",
            bundle::NAME
        )
    })?;
    ensure!(
        !version.contains('-'),
        "{version} is no release's version: a candidate or a build stays out of the public feed"
    );
    let notes = root.join(NOTES).join(format!("{version}.md"));
    ensure!(notes.is_file(), "{} is not there", notes.display());
    let directory = root.join(PUBLIC_DIRECTORY);
    fs::create_dir_all(&directory).with_context(|| format!("making {}", directory.display()))?;
    copy_unless_same(dmg, &directory.join(name))?;
    copy_unless_same(
        &notes,
        &directory.join(format!("{}.md", name.trim_end_matches(".dmg"))),
    )?;
    // The published feed is the one to add to, whatever the directory
    // kept.
    let appcast = directory.join("appcast.xml");
    match previous {
        Some(previous) => fs::write(&appcast, previous)
            .with_context(|| format!("writing {}", appcast.display()))?,
        None if appcast.exists() => {
            fs::remove_file(&appcast).with_context(|| format!("removing {}", appcast.display()))?;
        }
        None => {}
    }
    fetch_released(&directory, version)?;
    let tools = crate::sparkle::release(&root.join("target"))?.join("bin");
    let key = key_arguments(root)?;
    status(
        Command::new(tools.join("generate_appcast"))
            .args(&key)
            .args(["--maximum-deltas", &DELTAS_FROM.to_string()])
            .args(["--embed-release-notes", "--download-url-prefix"])
            .arg(format!("{RELEASES}v{version}/"))
            .arg(&directory),
        "writing the public appcast",
    )?;
    let text =
        fs::read_to_string(&appcast).with_context(|| format!("reading {}", appcast.display()))?;
    fs::write(&appcast, at_their_releases(&text)?)
        .with_context(|| format!("writing {}", appcast.display()))?;
    status(
        Command::new(tools.join("sign_update"))
            .args(&key)
            .arg(&appcast),
        "signing the public appcast",
    )?;
    let text =
        fs::read_to_string(&appcast).with_context(|| format!("reading {}", appcast.display()))?;
    ensure!(signed(&text), "{} has no signature", appcast.display());
    status(
        Command::new(tools.join("sign_update"))
            .args(&key)
            .arg("--verify")
            .arg(&appcast),
        "checking the public appcast's signature",
    )?;
    let out = root.join("target/releases").join(version);
    fs::create_dir_all(&out).with_context(|| format!("making {}", out.display()))?;
    let asset = out.join("appcast.xml");
    fs::copy(&appcast, &asset).with_context(|| format!("writing {}", asset.display()))?;
    let deltas = deltas_of(&text, version)
        .into_iter()
        .map(|delta| directory.join(delta))
        .collect::<Vec<_>>();
    for delta in &deltas {
        ensure!(delta.is_file(), "{} is not there", delta.display());
    }
    println!(
        "feed: {name} in {}, signed, an asset of its release",
        asset.display()
    );
    Ok(Published {
        deltas,
        appcast: asset,
    })
}

/// Downloads into `directory` the DMGs of the [`DELTAS_FROM`] latest
/// published releases before `version` that it lacks, of their GitHub
/// releases, which `generate_appcast` makes the release's deltas of: the
/// maintainer's Mac keeps them, a runner of the CI has none.
fn fetch_released(directory: &Path, version: &str) -> Result<()> {
    let listed = crate::notarize::output(
        Command::new("gh").args([
            "release",
            "list",
            "--repo",
            REPOSITORY,
            "--exclude-drafts",
            "--exclude-pre-releases",
            "--limit",
            "100",
            "--json",
            "tagName",
            "--jq",
            ".[].tagName",
        ]),
        "listing the published releases",
    )?;
    let tags: Vec<&str> = listed.lines().map(str::trim).collect();
    for released in released_before(&tags, version, DELTAS_FROM) {
        let name = format!("{}-{released}.dmg", bundle::NAME);
        if directory.join(&name).is_file() {
            continue;
        }
        println!("feed: downloading {name} of v{released}, for the deltas");
        status(
            Command::new("gh")
                .args(["release", "download", &format!("v{released}")])
                .args(["--repo", REPOSITORY, "--pattern", &name, "--dir"])
                .arg(directory),
            &format!("downloading {name}"),
        )?;
    }
    Ok(())
}

/// The versions of the releases of `tags`, `vX.Y.Z`, before `version`, the
/// latest `count` of them, the latest first.
fn released_before(tags: &[&str], version: &str, count: usize) -> Vec<String> {
    let numbers = |version: &str| -> Option<(u64, u64, u64)> {
        let mut parts = version.split('.').map(|part| part.parse::<u64>().ok());
        let numbers = (parts.next()??, parts.next()??, parts.next()??);
        parts.next().is_none().then_some(numbers)
    };
    let Some(before) = numbers(version) else {
        return Vec::new();
    };
    let mut released: Vec<((u64, u64, u64), &str)> = tags
        .iter()
        .filter_map(|tag| tag.strip_prefix('v'))
        .filter_map(|released| Some((numbers(released)?, released)))
        .filter(|(numbers, _)| *numbers < before)
        .collect();
    released.sort_unstable_by_key(|&(numbers, _)| std::cmp::Reverse(numbers));
    released.dedup();
    released
        .into_iter()
        .take(count)
        .map(|(_, released)| released.to_owned())
        .collect()
}

/// The appcast's `text` of each item's DMG and deltas at the item's own
/// release, `v<X.Y.Z>/` of its `shortVersionString`, which
/// `generate_appcast` gives the prefix of the release that it writes.
fn at_their_releases(text: &str) -> Result<String> {
    let item = regex::Regex::new(r"(?s)<item>.*?</item>")?;
    let version = regex::Regex::new(
        r"<sparkle:shortVersionString>([0-9A-Za-z.\-]+)</sparkle:shortVersionString>",
    )?;
    let release = regex::Regex::new(&format!(r#"{}v[^/"]+/"#, regex::escape(RELEASES)))?;
    let mut failed = None;
    let written = item.replace_all(text, |found: &regex::Captures| {
        let item = &found[0];
        match version.captures(item) {
            Some(of) => release
                .replace_all(item, format!("{RELEASES}v{}/", &of[1]).as_str())
                .into_owned(),
            None => {
                failed.get_or_insert_with(|| item.chars().take(120).collect::<String>());
                item.to_owned()
            }
        }
    });
    if let Some(item) = failed {
        bail!("an item of the appcast has no shortVersionString: {item}…");
    }
    Ok(written.into_owned())
}

/// The file names of the deltas of the item of `version` in the appcast's
/// `text`.
fn deltas_of(text: &str, version: &str) -> Vec<String> {
    let item = regex::Regex::new(r"(?s)<item>.*?</item>").expect("the item's pattern");
    let delta = regex::Regex::new(r#"url="[^"]*/([^"/]+\.delta)""#).expect("the delta's pattern");
    let mark = format!("<sparkle:shortVersionString>{version}</sparkle:shortVersionString>");
    item.find_iter(text)
        .map(|found| found.as_str())
        .filter(|item| item.contains(&mark))
        .flat_map(|item| {
            delta
                .captures_iter(item)
                .map(|of| of[1].to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The arguments of Sparkle's tools that name the key: the file of
/// `LOKYR_SPARKLE_KEY_FILE`, from the workspace root if relative, or the
/// keychain's account.
fn key_arguments(root: &Path) -> Result<Vec<OsString>> {
    Ok(match crate::private::variable(KEY_FILE_VARIABLE)? {
        Some(file) if !file.is_empty() => {
            let file = root.join(file);
            ensure!(file.is_file(), "{} is not there", file.display());
            vec!["--ed-key-file".into(), file.into_os_string()]
        }
        _ => vec!["--account".into(), ACCOUNT.into()],
    })
}

/// The version of a DMG of `cargo xtask dmg`, of its file's `name`.
fn version_of(name: &str) -> Option<&str> {
    name.strip_prefix(bundle::NAME)?
        .strip_prefix('-')?
        .strip_suffix(".dmg")
        .filter(|version| !version.is_empty())
}

/// The release of `version`, without its date or its candidate:
/// `0.3.0` of `0.3.0-20261102` and of `0.3.0-rc.1`.
pub(crate) fn release_of(version: &str) -> &str {
    version
        .split_once('-')
        .map_or(version, |(release, _)| release)
}

/// Whether the appcast's `text` has the signature that `generate_appcast`
/// embeds.
fn signed(text: &str) -> bool {
    text.contains(SIGNATURE_MARK)
}

/// Copies `from` to `to`, unless they are the same file, which a copy
/// would empty, as of a DMG that the feed holds already.
fn copy_unless_same(from: &Path, to: &Path) -> Result<()> {
    if let (Ok(from), Ok(to)) = (from.canonicalize(), to.canonicalize())
        && from == to
    {
        return Ok(());
    }
    fs::copy(from, to).with_context(|| format!("copying {}", from.display()))?;
    Ok(())
}

/// Runs `command`, which `what` describes, and fails if it fails.
fn status(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| what.to_owned())?;
    if !status.success() {
        bail!("{what} failed: {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_release_s_appcast_is_told_apart_from_a_failure() {
        let feed = "<?xml version=\"1.0\"?><rss><channel></channel></rss>";
        let at = "https://objects.githubusercontent.com/appcast.xml";
        // The release's asset.
        assert_eq!(
            latest_of(&format!("200 {at}"), feed).unwrap().as_deref(),
            Some(feed)
        );
        // A release before the releases held it: the site's feed.
        let before =
            "404 https://github.com/sephiroth74/lokyr/releases/download/v1.5.0/appcast.xml";
        assert_eq!(latest_of(before, "Not Found").unwrap(), None);
        // A release from 1.6.0 of none: a fault, never the old feed.
        let missing = latest_of(
            "404 https://github.com/sephiroth74/lokyr/releases/download/v1.6.0/appcast.xml",
            "Not Found",
        )
        .unwrap_err()
        .to_string();
        assert!(
            missing.contains("v1.6.0, holds no appcast.xml"),
            "{missing}"
        );
        assert!(
            latest_of(
                "404 https://github.com/sephiroth74/lokyr/releases/download/v1.10.2/appcast.xml",
                ""
            )
            .is_err()
        );
        // GitHub's failure, a page that is no appcast, and a 404 of no
        // release's address.
        assert!(latest_of(&format!("503 {at}"), "").is_err());
        assert!(latest_of(&format!("200 {at}"), "<html>").is_err());
        assert!(
            latest_of(
                "404 https://github.com/sephiroth74/lokyr/releases/latest",
                ""
            )
            .is_err()
        );
    }

    #[test]
    fn the_deltas_are_made_of_the_latest_releases_before() {
        let tags = [
            "v1.5.0",
            "v1.4.0",
            "v1.10.0",
            "v1.3.0",
            "v1.2.0",
            "v1.1.0",
            "v1.0.0",
            "v0.9.0",
            "v1.6.0-rc.1",
            "nightly",
        ];
        assert_eq!(
            released_before(&tags, "1.6.0", 5),
            ["1.5.0", "1.4.0", "1.3.0", "1.2.0", "1.1.0"]
        );
        assert_eq!(released_before(&tags, "1.11.0", 2), ["1.10.0", "1.5.0"]);
        assert_eq!(released_before(&tags, "1.0.0", 5), ["0.9.0"]);
        assert!(released_before(&tags, "1.6.0-rc.1", 5).is_empty());
    }

    #[test]
    fn a_dmg_s_name_gives_its_version_and_its_release() {
        assert_eq!(
            version_of("Lokyr-0.3.0-20261102.dmg"),
            Some("0.3.0-20261102")
        );
        assert_eq!(version_of("Lokyr-1.0.0-rc.2.dmg"), Some("1.0.0-rc.2"));
        assert_eq!(version_of("Lokyr-1.0.0.dmg"), Some("1.0.0"));
        assert_eq!(version_of("Lokyr-.dmg"), None);
        assert_eq!(version_of("Other-1.0.0.dmg"), None);
        assert_eq!(version_of("Lokyr-1.0.0.zip"), None);
        assert_eq!(release_of("0.3.0-20261102"), "0.3.0");
        assert_eq!(release_of("1.0.0-rc.2"), "1.0.0");
        assert_eq!(release_of("1.0.0"), "1.0.0");
    }

    #[test]
    fn a_file_copied_onto_itself_keeps_its_bytes() {
        let directory =
            std::env::temp_dir().join(format!("lokyr-feed-copy-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let dmg = directory.join("Lokyr-1.0.0.dmg");
        fs::write(&dmg, b"an image").unwrap();
        // The same file, by another path too.
        copy_unless_same(&dmg, &dmg).unwrap();
        copy_unless_same(
            &directory
                .join("../")
                .join(directory.file_name().unwrap())
                .join("Lokyr-1.0.0.dmg"),
            &dmg,
        )
        .unwrap();
        assert_eq!(fs::read(&dmg).unwrap(), b"an image");
        // Another file is copied.
        let copy = directory.join("copy.dmg");
        copy_unless_same(&dmg, &copy).unwrap();
        assert_eq!(fs::read(&copy).unwrap(), b"an image");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn each_item_downloads_from_its_own_release() {
        // As generate_appcast writes 1.1.0 of the folder that holds 1.0.0's
        // DMG too: both of 1.1.0's prefix.
        let feed = r#"<rss><channel>
<item><title>1.1.0</title><sparkle:version>202612011200</sparkle:version><sparkle:shortVersionString>1.1.0</sparkle:shortVersionString>
<enclosure url="https://github.com/sephiroth74/lokyr/releases/download/v1.1.0/Lokyr-1.1.0.dmg" length="1"/>
<sparkle:deltas><enclosure url="https://github.com/sephiroth74/lokyr/releases/download/v1.1.0/Lokyr202612011200-202611200900.delta" sparkle:deltaFrom="202611200900"/></sparkle:deltas></item>
<item><title>1.0.0</title><sparkle:version>202611200900</sparkle:version><sparkle:shortVersionString>1.0.0</sparkle:shortVersionString>
<enclosure url="https://github.com/sephiroth74/lokyr/releases/download/v1.1.0/Lokyr-1.0.0.dmg" length="1"/></item>
</channel></rss>"#;
        let written = at_their_releases(feed).unwrap();
        assert!(written.contains("download/v1.1.0/Lokyr-1.1.0.dmg"));
        assert!(written.contains("download/v1.1.0/Lokyr202612011200-202611200900.delta"));
        assert!(written.contains("download/v1.0.0/Lokyr-1.0.0.dmg"));
        assert!(!written.contains("download/v1.1.0/Lokyr-1.0.0.dmg"));
        // Written again, as it is.
        assert_eq!(at_their_releases(&written).unwrap(), written);
        assert_eq!(
            deltas_of(&written, "1.1.0"),
            ["Lokyr202612011200-202611200900.delta"]
        );
        assert!(deltas_of(&written, "1.0.0").is_empty());
        // An item of no version is refused.
        assert!(at_their_releases("<item><title>?</title></item>").is_err());
    }

    /// Two releases published, of Sparkle's tools and the EdDSA key, in a
    /// root of its own: each item downloads from its own release, the
    /// second's delta from its, and the site's appcast is signed. Of the
    /// maintainer's Mac, of 0.8.0's and 0.9.0's DMGs in
    /// `private/releases`: `cargo test -p xtask -- --ignored publishing`.
    #[test]
    #[ignore = "Sparkle's tools, the signing key and two released DMGs"]
    fn publishing_two_releases_downloads_each_from_its_own() {
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .unwrap();
        let root = std::env::temp_dir().join(format!("lokyr-feed-publish-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("docs/releases")).unwrap();
        fs::create_dir_all(root.join("site/public")).unwrap();
        std::os::unix::fs::symlink(workspace.join("target"), root.join("target")).unwrap();
        // The key's file, where the environment names one, from the root.
        if let Some(file) = crate::private::variable(KEY_FILE_VARIABLE)
            .unwrap()
            .filter(|file| !file.is_empty() && Path::new(file).is_relative())
        {
            let linked = root.join(&file);
            fs::create_dir_all(linked.parent().unwrap()).unwrap();
            std::os::unix::fs::symlink(workspace.join(&file), linked).unwrap();
        }
        for version in ["0.8.0", "0.9.0"] {
            fs::copy(
                workspace.join(NOTES).join(format!("{version}.md")),
                root.join(NOTES).join(format!("{version}.md")),
            )
            .unwrap();
        }
        let dmg = |version: &str| {
            workspace
                .join("private/releases")
                .join(format!("v{version}"))
                .join(format!("{}-{version}.dmg", bundle::NAME))
        };
        let first = publish(&root, &dmg("0.8.0"), None).unwrap();
        assert!(first.deltas.is_empty());
        let previous = fs::read_to_string(&first.appcast).unwrap();
        let second = publish(&root, &dmg("0.9.0"), Some(&previous)).unwrap();
        let deltas = second.deltas;
        assert_eq!(deltas.len(), 1, "{deltas:?}");
        let site = fs::read_to_string(&second.appcast).unwrap();
        assert!(site.contains(&format!("{RELEASES}v0.9.0/Lokyr-0.9.0.dmg")));
        assert!(site.contains(&format!("{RELEASES}v0.8.0/Lokyr-0.8.0.dmg")));
        let delta = deltas[0].file_name().unwrap().to_str().unwrap();
        assert!(site.contains(&format!("{RELEASES}v0.9.0/{delta}")));
        assert!(signed(&site));
        // A candidate stays out.
        assert!(publish(&root, Path::new("Lokyr-1.0.0-rc.1.dmg"), None).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn an_appcast_is_signed_by_its_embedded_signatures() {
        let signed_feed =
            "<rss></rss>\n<!-- sparkle-signatures:\nedSignature: AAAA\nlength: 12\n-->\n";
        assert!(signed(signed_feed));
        assert!(!signed("<rss></rss>\n"));
    }
}
