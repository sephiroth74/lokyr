//! `cargo xtask cask <version> [--tap <folder>]`: the Homebrew cask of a
//! release, `Casks/lokyr.rb` of the project's tap,
//! `sephiroth74/homebrew-tap`, which `brew install --cask
//! sephiroth74/tap/lokyr` installs: the DMG of the release's GitHub assets,
//! its SHA-256 of `target/Lokyr-<version>.dmg`, Sparkle's updates told to
//! Homebrew, the app's command line tools linked, and the app's folders that
//! `brew uninstall --zap` takes away. It writes the cask into the tap's
//! folder, `../homebrew-tap` beside the workspace by default, if it is
//! there, and into `target/releases/<version>/lokyr.rb`; the tap's commit
//! and push come once the release is published.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};

use crate::bundle;

/// The repository of the releases, whose assets the cask downloads.
const REPOSITORY: &str = "sephiroth74/lokyr";

/// The app's folders and files that `brew uninstall --zap` takes away: the
/// config, the plugins' lists and the themes; the state, the plugins' store
/// and Sleip's cache; Sparkle's cache and storage, the log, the defaults
/// and the windows' saved state; in the alphabet's order, as Homebrew's
/// style asks.
const ZAPPED: [&str; 8] = [
    "~/.config/lokyr",
    "~/Library/Application Support/lokyr",
    "~/Library/Caches/io.github.sephiroth74.lokyr",
    "~/Library/Caches/lokyr",
    "~/Library/HTTPStorages/io.github.sephiroth74.lokyr",
    "~/Library/Logs/Lokyr",
    "~/Library/Preferences/io.github.sephiroth74.lokyr.plist",
    "~/Library/Saved Application State/io.github.sephiroth74.lokyr.savedState",
];

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let version = args
        .next()
        .context("cask takes the release's version, X.Y.Z, and `--tap <folder>`")?;
    let mut tap = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tap" => tap = Some(PathBuf::from(args.next().context("--tap takes a folder")?)),
            _ => bail!("cask takes the version and `--tap <folder>`, not `{arg}`"),
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    for written in write(&root, &version, tap.as_deref())? {
        println!("cask: {}", written.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// Writes the cask of the release `version`, of its DMG in `target`, into
/// `target/releases/<version>/lokyr.rb` and into the tap's folder, `tap`,
/// or `../homebrew-tap` if it is there: the files written.
pub fn write(root: &Path, version: &str, tap: Option<&Path>) -> Result<Vec<PathBuf>> {
    ensure!(
        is_release(version),
        "`{version}` is no release's version, X.Y.Z: a candidate has no cask"
    );
    let dmg = root
        .join("target")
        .join(format!("{}-{version}.dmg", bundle::NAME));
    ensure!(
        dmg.is_file(),
        "{} is not there: `just dmg` makes it",
        dmg.display()
    );
    let text = text(version, &digest(&dmg)?);
    let mut written = Vec::new();
    let kept = root.join("target/releases").join(version).join("lokyr.rb");
    let tap = tap
        .map(Path::to_path_buf)
        .or_else(|| Some(root.join("../homebrew-tap")).filter(|tap| tap.is_dir()));
    for file in std::iter::once(kept).chain(tap.map(|tap| tap.join("Casks/lokyr.rb"))) {
        if let Some(folder) = file.parent() {
            fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
        }
        fs::write(&file, &text).with_context(|| format!("writing {}", file.display()))?;
        written.push(file);
    }
    Ok(written)
}

/// Whether `version` is a release's, `X.Y.Z` of numbers alone.
fn is_release(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

/// The cask of the release `version`, of its DMG's SHA-256 `sha256`.
pub fn text(version: &str, sha256: &str) -> String {
    let zapped: Vec<String> = ZAPPED
        .iter()
        .map(|path| format!("    \"{path}\","))
        .collect();
    format!(
        r##"cask "lokyr" do
  version "{version}"
  sha256 "{sha256}"

  url "https://github.com/{REPOSITORY}/releases/download/v#{{version}}/{name}-#{{version}}.dmg"
  name "{name}"
  desc "Terminal emulator whose every feature is a plugin"
  homepage "https://lokyr.dev/"

  livecheck do
    url :url
    strategy :github_latest
  end

  auto_updates true
  depends_on arch: :arm64
  depends_on macos: :tahoe

  app "{name}.app"
  binary "#{{appdir}}/{name}.app/Contents/MacOS/{executable}"
  binary "#{{appdir}}/{name}.app/Contents/MacOS/{sleip}"

  zap trash: [
{zapped}
  ]
end
"##,
        name = bundle::NAME,
        executable = bundle::EXECUTABLE,
        sleip = bundle::SLEIP,
        zapped = zapped.join("\n"),
    )
}

/// The SHA-256 digest of the file at `path`, in hex.
fn digest(path: &Path) -> Result<String> {
    let output = Command::new("shasum")
        .args(["-a", "256"])
        .arg(path)
        .output()
        .with_context(|| format!("hashing {}", path.display()))?;
    ensure!(
        output.status.success(),
        "hashing {} failed: {}",
        path.display(),
        output.status
    );
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .next()
        .map(str::to_owned)
        .with_context(|| format!("shasum told no digest of {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cask_downloads_the_release_s_dmg_and_links_the_tools() {
        let text = text("1.0.0", &"a".repeat(64));
        assert!(text.contains("version \"1.0.0\""));
        assert!(text.contains(&format!("sha256 \"{}\"", "a".repeat(64))));
        assert!(text.contains(
            "url \"https://github.com/sephiroth74/lokyr/releases/download/v#{version}/Lokyr-#{version}.dmg\""
        ));
        assert!(text.contains("auto_updates true"));
        assert!(text.contains("depends_on macos: :tahoe"));
        assert!(text.contains("app \"Lokyr.app\""));
        assert!(text.contains("binary \"#{appdir}/Lokyr.app/Contents/MacOS/lokyr\""));
        assert!(text.contains("binary \"#{appdir}/Lokyr.app/Contents/MacOS/sleip\""));
        for path in ZAPPED {
            assert!(text.contains(&format!("\"{path}\",")), "{path}");
        }
    }

    #[test]
    fn a_candidate_has_no_cask() {
        assert!(is_release("1.0.0"));
        assert!(is_release("10.2.33"));
        for version in ["1.0.0-rc.1", "1.0", "1.0.0-20261112", "v1.0.0", "1..0"] {
            assert!(!is_release(version), "{version}");
        }
        let root = std::env::temp_dir();
        assert!(write(&root, "1.0.0-rc.1", None).is_err());
    }
}
