//! Sparkle, the updater that the bundle holds in `Contents/Frameworks`
//! (<https://sparkle-project.org>): the release of a pinned version,
//! downloaded once into `target/sparkle` and checked against its digest;
//! its framework without what an app outside the sandbox does not run,
//! signed with the bundle's identity, part by part from the inside, as
//! Sparkle's documentation of the hardened runtime has it; its license; and
//! the keys of the `Info.plist` that set the feed, its key and the checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};

/// The release of Sparkle that the bundle holds.
pub(crate) const VERSION: &str = "2.10.1";

/// The SHA-256 digest of the release's archive.
const DIGEST: &str = "1a856d2a9edb8e628de88121d7be04cf0e2167f8641650b78fca00b14244cf27";

/// The address of the updates' feed.
const FEED: &str = "https://lokyr.dev/appcast.xml";

/// The public EdDSA key whose private key signs the updates, in base64, as
/// Sparkle's `generate_keys --account lokyr` tells it; the private key
/// stays in the maintainer's keychain.
const PUBLIC_KEY: &str = "7U7ab9wyadh2Bf1/uOgvuwpgyATX9gwc+DDpDr/5Fyw=";

/// How long between two checks now and then, in seconds: a day.
const CHECK_INTERVAL: u32 = 86_400;

/// The framework's parts that an app outside the sandbox does not run: its
/// headers and module, and its XPC services, which only the keys of a
/// sandboxed app's `Info.plist` turn on.
const UNUSED: [&str; 4] = ["Headers", "PrivateHeaders", "Modules", "XPCServices"];

/// The framework's helpers, signed before the framework: the tool that
/// installs an update and the app that shows its progress.
const HELPERS: [&str; 2] = ["Versions/B/Autoupdate", "Versions/B/Updater.app"];

/// The parts of the release that the bundle and the feed take: the
/// framework, its license, and the tools that write and sign the feed.
const PARTS: [&str; 4] = [
    "Sparkle.framework",
    "LICENSE",
    "bin/generate_appcast",
    "bin/sign_update",
];

/// The entries of the bundle's `Info.plist` for Sparkle: the feed, its key,
/// a feed signed with it and each update checked against it before it
/// unpacks, the checks now and then on, as `[updates] check` turns them off
/// at runtime, without Sparkle's prompt to turn them on, a day apart, and
/// no profile of the Mac sent with them.
pub(crate) fn plist_entries() -> String {
    format!(
        concat!(
            "\t<key>SUFeedURL</key>\n\t<string>{FEED}</string>\n",
            "\t<key>SUPublicEDKey</key>\n\t<string>{PUBLIC_KEY}</string>\n",
            "\t<key>SURequireSignedFeed</key>\n\t<true/>\n",
            "\t<key>SUVerifyUpdateBeforeExtraction</key>\n\t<true/>\n",
            "\t<key>SUEnableAutomaticChecks</key>\n\t<true/>\n",
            "\t<key>SUScheduledCheckInterval</key>\n\t<integer>{CHECK_INTERVAL}</integer>\n",
            "\t<key>SUEnableSystemProfiling</key>\n\t<false/>\n",
        ),
        FEED = FEED,
        PUBLIC_KEY = PUBLIC_KEY,
        CHECK_INTERVAL = CHECK_INTERVAL,
    )
}

/// The directory of the release in `target`, with its [`PARTS`]:
/// downloaded and unpacked the first time, its archive checked against
/// [`DIGEST`].
pub(crate) fn release(target: &Path) -> Result<PathBuf> {
    let directory = target.join("sparkle").join(VERSION);
    if PARTS.iter().all(|part| directory.join(part).exists()) {
        return Ok(directory);
    }
    fs::create_dir_all(&directory).with_context(|| format!("making {}", directory.display()))?;
    let archive = directory.join(format!("Sparkle-{VERSION}.tar.xz"));
    if !archive.is_file() || digest(&archive)? != DIGEST {
        let partial = archive.with_extension("partial");
        let url = format!(
            "https://github.com/sparkle-project/Sparkle/releases/download/{VERSION}/Sparkle-{VERSION}.tar.xz"
        );
        println!("bundle: downloading Sparkle {VERSION}");
        run(
            Command::new("curl")
                .args(["--fail", "--silent", "--show-error", "--location"])
                .args(["--retry", "3", "--output"])
                .arg(&partial)
                .arg(&url),
            "downloading Sparkle",
        )?;
        let found = digest(&partial)?;
        ensure!(
            found == DIGEST,
            "Sparkle's archive of {url} has the digest {found}, not {DIGEST}"
        );
        fs::rename(&partial, &archive).with_context(|| format!("keeping {}", archive.display()))?;
    }
    let unpacked = directory.join("unpacked");
    if unpacked.exists() {
        fs::remove_dir_all(&unpacked)
            .with_context(|| format!("removing {}", unpacked.display()))?;
    }
    fs::create_dir_all(&unpacked).with_context(|| format!("making {}", unpacked.display()))?;
    run(
        Command::new("tar")
            .arg("-xJf")
            .arg(&archive)
            .arg("-C")
            .arg(&unpacked)
            .args(PARTS.map(|part| format!("./{part}"))),
        "unpacking Sparkle",
    )?;
    for part in PARTS {
        let kept = directory.join(part);
        if let Some(parent) = kept.parent() {
            fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
        }
        if kept.is_dir() {
            fs::remove_dir_all(&kept).with_context(|| format!("removing {}", kept.display()))?;
        } else if kept.exists() {
            fs::remove_file(&kept).with_context(|| format!("removing {}", kept.display()))?;
        }
        fs::rename(unpacked.join(part), &kept)
            .with_context(|| format!("keeping Sparkle's {part}"))?;
    }
    fs::remove_dir_all(&unpacked).with_context(|| format!("removing {}", unpacked.display()))?;
    Ok(directory)
}

/// Puts the framework of `release` into the bundle `app`'s
/// `Contents/Frameworks`, without its [`UNUSED`] parts, signs its helpers
/// then itself with `identity`, and its license into the bundle's
/// `Contents/Resources/licenses/sparkle`.
pub(crate) fn embed(release: &Path, app: &Path, identity: &str) -> Result<()> {
    let frameworks = app.join("Contents/Frameworks");
    fs::create_dir_all(&frameworks).with_context(|| format!("making {}", frameworks.display()))?;
    let framework = frameworks.join("Sparkle.framework");
    run(
        Command::new("ditto")
            .arg(release.join("Sparkle.framework"))
            .arg(&framework),
        "copying Sparkle's framework",
    )?;
    strip(&framework)?;
    for part in HELPERS {
        sign(&framework.join(part), identity)?;
    }
    sign(&framework, identity)?;
    let licenses = app.join("Contents/Resources/licenses/sparkle");
    fs::create_dir_all(&licenses).with_context(|| format!("making {}", licenses.display()))?;
    fs::copy(release.join("LICENSE"), licenses.join("LICENSE"))
        .context("copying Sparkle's license")?;
    Ok(())
}

/// Takes the [`UNUSED`] parts out of `framework`: their links at its top
/// and their directories in its version.
fn strip(framework: &Path) -> Result<()> {
    for name in UNUSED {
        let link = framework.join(name);
        if link.symlink_metadata().is_ok() {
            fs::remove_file(&link).with_context(|| format!("removing {}", link.display()))?;
        }
        let directory = framework.join("Versions/B").join(name);
        if directory.exists() {
            fs::remove_dir_all(&directory)
                .with_context(|| format!("removing {}", directory.display()))?;
        }
    }
    Ok(())
}

/// Signs `path` with `identity` and the hardened runtime, with a secure
/// timestamp unless ad hoc.
fn sign(path: &Path, identity: &str) -> Result<()> {
    let mut command = Command::new("codesign");
    command.args(["--force", "--options", "runtime", "--sign", identity]);
    if identity != "-" {
        command.arg("--timestamp");
    }
    run(
        command.arg(path),
        &format!("signing {}", path.file_name().unwrap_or_default().display()),
    )
}

/// The SHA-256 digest of the file at `path`, in hex.
fn digest(path: &Path) -> Result<String> {
    let output = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .with_context(|| format!("hashing {}", path.display()))?;
    if !output.status.success() {
        bail!("hashing {} failed: {}", path.display(), output.status);
    }
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .with_context(|| format!("shasum told no digest of {}", path.display()))
}

/// Runs `command`, which `what` describes, and fails if it fails.
fn run(command: &mut Command, what: &str) -> Result<()> {
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
    fn the_plist_s_entries_set_the_feed_its_key_and_a_check_a_day() {
        let entries = plist_entries();
        assert!(
            entries
                .contains("<key>SUFeedURL</key>\n\t<string>https://lokyr.dev/appcast.xml</string>")
        );
        assert!(entries.contains(&format!(
            "<key>SUPublicEDKey</key>\n\t<string>{PUBLIC_KEY}</string>"
        )));
        assert!(entries.contains("<key>SUEnableAutomaticChecks</key>\n\t<true/>"));
        assert!(
            entries.contains("<key>SUScheduledCheckInterval</key>\n\t<integer>86400</integer>")
        );
        assert!(entries.contains("<key>SUEnableSystemProfiling</key>\n\t<false/>"));
        // A feed signed, and each update checked before it unpacks, which
        // Sparkle asks of a signed feed.
        assert!(entries.contains("<key>SURequireSignedFeed</key>\n\t<true/>"));
        assert!(entries.contains("<key>SUVerifyUpdateBeforeExtraction</key>\n\t<true/>"));
        // An Ed25519 key of 32 bytes, in base64.
        assert_eq!(PUBLIC_KEY.len(), 44);
        assert!(PUBLIC_KEY.ends_with('='));
    }

    #[test]
    #[cfg(unix)]
    fn stripping_takes_the_unused_parts_and_their_links_and_keeps_the_rest() {
        let framework = std::env::temp_dir()
            .join(format!("lokyr-sparkle-{}", std::process::id()))
            .join("Sparkle.framework");
        let _ = fs::remove_dir_all(&framework);
        let version = framework.join("Versions/B");
        for name in UNUSED.iter().chain(&["Resources"]) {
            fs::create_dir_all(version.join(name)).unwrap();
            std::os::unix::fs::symlink(
                Path::new("Versions/Current").join(name),
                framework.join(name),
            )
            .unwrap();
        }
        fs::write(version.join("Sparkle"), b"code").unwrap();
        std::os::unix::fs::symlink("B", framework.join("Versions/Current")).unwrap();
        strip(&framework).unwrap();
        for name in UNUSED {
            assert!(framework.join(name).symlink_metadata().is_err(), "{name}");
            assert!(!version.join(name).exists(), "{name}");
        }
        assert!(framework.join("Resources").is_dir());
        assert!(version.join("Sparkle").is_file());
        fs::remove_dir_all(framework.parent().unwrap()).unwrap();
    }
}
