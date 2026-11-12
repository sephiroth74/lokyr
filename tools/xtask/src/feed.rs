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
//! - `feed publish <dmg>` puts a release's DMG into the public feed, the
//!   site's `site/public/appcast.xml`, which `lokyr.dev/appcast.xml` serves
//!   and the bundle's `SUFeedURL` names: `generate_appcast` writes it in
//!   the untracked `private/public-feed`, which keeps the released DMGs,
//!   of which it makes the deltas, from the site's appcast; each item's
//!   DMG and deltas at its own GitHub release, `v<X.Y.Z>`, which
//!   `generate_appcast`'s one prefix cannot tell, and the appcast signed
//!   again. `just release` publishes each release's, and uploads its
//!   deltas to its release; the appcast is committed and pushed once the
//!   release is published, as its DMG is not public before.
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

/// The public feed's appcast, in the site, relative to the workspace root.
pub(crate) const PUBLIC_APPCAST: &str = "site/public/appcast.xml";

/// Where the releases' assets are downloaded from, each of its tag,
/// `v<X.Y.Z>/`.
const RELEASES: &str = "https://github.com/sephiroth74/lokyr/releases/download/";

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
            for delta in publish(&root, &dmg)? {
                println!("feed: {} to upload to its release", delta.display());
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

/// Puts the release's `dmg` into the public feed, of the site's appcast and
/// the released DMGs that `private/public-feed` keeps, writes the site's
/// appcast again, each item at its own release and signed, and gives the
/// deltas made for the release, to upload to it.
pub fn publish(root: &Path, dmg: &Path) -> Result<Vec<PathBuf>> {
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
    // The site's appcast is the feed's, whatever the directory kept.
    let appcast = directory.join("appcast.xml");
    let site = root.join(PUBLIC_APPCAST);
    if site.is_file() {
        fs::copy(&site, &appcast).with_context(|| format!("copying {}", site.display()))?;
    } else if appcast.exists() {
        fs::remove_file(&appcast).with_context(|| format!("removing {}", appcast.display()))?;
    }
    let tools = crate::sparkle::release(&root.join("target"))?.join("bin");
    let key = key_arguments(root)?;
    status(
        Command::new(tools.join("generate_appcast"))
            .args(&key)
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
    fs::copy(&appcast, &site).with_context(|| format!("writing {}", site.display()))?;
    let deltas = deltas_of(&text, version)
        .into_iter()
        .map(|delta| directory.join(delta))
        .collect::<Vec<_>>();
    for delta in &deltas {
        ensure!(delta.is_file(), "{} is not there", delta.display());
    }
    println!(
        "feed: {name} in {}, signed; commit it once the release is published",
        site.display()
    );
    Ok(deltas)
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
        assert!(publish(&root, &dmg("0.8.0")).unwrap().is_empty());
        let deltas = publish(&root, &dmg("0.9.0")).unwrap();
        assert_eq!(deltas.len(), 1, "{deltas:?}");
        let site = fs::read_to_string(root.join(PUBLIC_APPCAST)).unwrap();
        assert!(site.contains(&format!("{RELEASES}v0.9.0/Lokyr-0.9.0.dmg")));
        assert!(site.contains(&format!("{RELEASES}v0.8.0/Lokyr-0.8.0.dmg")));
        let delta = deltas[0].file_name().unwrap().to_str().unwrap();
        assert!(site.contains(&format!("{RELEASES}v0.9.0/{delta}")));
        assert!(signed(&site));
        // A candidate stays out.
        assert!(publish(&root, Path::new("Lokyr-1.0.0-rc.1.dmg")).is_err());
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
