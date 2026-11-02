//! `cargo xtask feed`: the feed of tests of the updates, which the builds
//! on the maintainer's Mac read until the repository is public, in the
//! untracked `private/feed`: the DMGs, their release notes, and the
//! appcast that Sparkle's `generate_appcast` writes over them, each DMG
//! and the appcast signed with the EdDSA key.
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
        _ => bail!("feed takes `add <dmg> [--notes <file>] [--channel <name>]` or `serve`"),
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
    fs::copy(dmg, directory.join(name)).with_context(|| format!("copying {}", dmg.display()))?;
    let stem = name.trim_end_matches(".dmg");
    fs::copy(&notes, directory.join(format!("{stem}.md")))
        .with_context(|| format!("copying {}", notes.display()))?;
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
fn release_of(version: &str) -> &str {
    version
        .split_once('-')
        .map_or(version, |(release, _)| release)
}

/// Whether the appcast's `text` has the signature that `generate_appcast`
/// embeds.
fn signed(text: &str) -> bool {
    text.contains(SIGNATURE_MARK)
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
    fn an_appcast_is_signed_by_its_embedded_signatures() {
        let signed_feed =
            "<rss></rss>\n<!-- sparkle-signatures:\nedSignature: AAAA\nlength: 12\n-->\n";
        assert!(signed(signed_feed));
        assert!(!signed("<rss></rss>\n"));
    }
}
