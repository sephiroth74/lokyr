//! `cargo xtask bundle`: builds `target/Bolt.app` from the release binary.
//!
//! The bundle holds the binary, its `Info.plist`, the terminfo entry
//! compiled into `Contents/Resources/terminfo`, and the icon, which
//! `actool` compiles from the Icon Composer document
//! `crates/app/resources/AppIcon.icon` into `Assets.car` and
//! `AppIcon.icns`. It is signed with the project's Developer ID when the
//! keychain has it, so that macOS keeps the permissions that it gave the
//! app from one build to the next, or with the identity that
//! `BOLT_SIGN_IDENTITY` names; otherwise, as on a machine without it,
//! ad hoc. It is not notarized.

use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

/// The bundle's identifier, which follows the terminal's name.
pub const IDENTIFIER: &str = "io.github.sephiroth74.bolt";

/// The bundle's name, and the app's in the Finder and the Dock.
const NAME: &str = "Bolt";

/// The binary, as the app's crate names it.
const EXECUTABLE: &str = "bolt";

/// The oldest macOS that the app runs on.
const MINIMUM_SYSTEM: &str = "26.0";

/// The Icon Composer document of the icon, relative to the workspace root.
const ICON: &str = "crates/app/resources/AppIcon.icon";

/// The team of the project's Developer ID.
const TEAM: &str = "XXXXXXXXXX";

/// The variable that names another signing identity, or `-` for ad hoc.
const IDENTITY_VARIABLE: &str = "BOLT_SIGN_IDENTITY";

/// The usage strings that macOS shows when a program started in the
/// terminal asks for what its privacy settings guard, which it asks on the
/// terminal's behalf: each key, and what the program wants.
const USAGES: [(&str, &str); 16] = [
    ("NSMicrophoneUsageDescription", "use the microphone"),
    ("NSCameraUsageDescription", "use the camera"),
    ("NSAppleEventsUsageDescription", "control other apps"),
    ("NSDesktopFolderUsageDescription", "read the Desktop folder"),
    (
        "NSDocumentsFolderUsageDescription",
        "read the Documents folder",
    ),
    (
        "NSDownloadsFolderUsageDescription",
        "read the Downloads folder",
    ),
    (
        "NSRemovableVolumesUsageDescription",
        "read removable volumes",
    ),
    ("NSNetworkVolumesUsageDescription", "read network volumes"),
    (
        "NSLocalNetworkUsageDescription",
        "reach devices on the local network",
    ),
    ("NSBluetoothAlwaysUsageDescription", "use Bluetooth"),
    ("NSContactsUsageDescription", "read your contacts"),
    (
        "NSCalendarsFullAccessUsageDescription",
        "use your calendars",
    ),
    (
        "NSRemindersFullAccessUsageDescription",
        "use your reminders",
    ),
    ("NSPhotoLibraryUsageDescription", "read your photos"),
    ("NSLocationUsageDescription", "know your location"),
    (
        "NSSpeechRecognitionUsageDescription",
        "use speech recognition",
    ),
];

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    if let Some(arg) = args.next() {
        bail!("bundle takes no argument, not `{arg}`");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    run_command(
        Command::new(env!("CARGO"))
            .args(["build", "--release", "--locked", "-p", "bolt-app"])
            .current_dir(&root),
        "building the app",
    )?;
    let app = root.join("target").join(format!("{NAME}.app"));
    if app.exists() {
        fs::remove_dir_all(&app).with_context(|| format!("removing {}", app.display()))?;
    }
    let (macos, resources) = (app.join("Contents/MacOS"), app.join("Contents/Resources"));
    for directory in [&macos, &resources] {
        fs::create_dir_all(directory).with_context(|| format!("making {}", directory.display()))?;
    }
    let binary = root.join("target/release").join(EXECUTABLE);
    fs::copy(&binary, macos.join(EXECUTABLE))
        .with_context(|| format!("copying {}", binary.display()))?;
    crate::terminfo::compile(
        &root.join(crate::terminfo::SOURCE),
        &resources.join("terminfo"),
    )?;
    let icon = root.join(ICON);
    let has_icon = icon.exists();
    if has_icon {
        compile_icon(&icon, &resources)?;
    } else {
        eprintln!("bundle: no icon at {ICON}: the app takes the system's");
    }
    let version = env!("CARGO_PKG_VERSION");
    fs::write(
        app.join("Contents/Info.plist"),
        info_plist(version, has_icon),
    )
    .context("writing the Info.plist")?;
    let identity = identity()?;
    sign(&app, &identity)?;
    println!(
        "bundle: {} signed {}",
        app.display(),
        if identity == "-" {
            "ad hoc".to_owned()
        } else {
            format!("by {identity}")
        }
    );
    Ok(ExitCode::SUCCESS)
}

/// Runs `command`, which `what` describes, and fails if it fails.
fn run_command(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| what.to_owned())?;
    if !status.success() {
        bail!("{what} failed: {status}");
    }
    Ok(())
}

/// The `Info.plist` of the app at `version`, which names its icon when it
/// has one.
fn info_plist(version: &str, icon: bool) -> String {
    let mut keys = vec![
        ("CFBundleDevelopmentRegion", "en".to_owned()),
        ("CFBundleDisplayName", NAME.to_owned()),
        ("CFBundleExecutable", EXECUTABLE.to_owned()),
        ("CFBundleIdentifier", IDENTIFIER.to_owned()),
        ("CFBundleInfoDictionaryVersion", "6.0".to_owned()),
        ("CFBundleName", NAME.to_owned()),
        ("CFBundlePackageType", "APPL".to_owned()),
        ("CFBundleShortVersionString", version.to_owned()),
        ("CFBundleVersion", version.to_owned()),
        ("LSMinimumSystemVersion", MINIMUM_SYSTEM.to_owned()),
        ("NSPrincipalClass", "NSApplication".to_owned()),
    ];
    if icon {
        keys.push(("CFBundleIconName", "AppIcon".to_owned()));
        keys.push(("CFBundleIconFile", "AppIcon".to_owned()));
    }
    for (key, what) in USAGES {
        keys.push((
            key,
            format!("A program started in the terminal wants to {what}."),
        ));
    }
    let mut plist = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    for (key, value) in keys {
        plist.push_str(&format!(
            "\t<key>{key}</key>\n\t<string>{}</string>\n",
            escaped(&value)
        ));
    }
    plist.push_str("\t<key>NSHighResolutionCapable</key>\n\t<true/>\n</dict>\n</plist>\n");
    plist
}

/// `text` with XML's special characters escaped.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Compiles the Icon Composer document `icon` into `resources`: the asset
/// catalog, `Assets.car`, which `CFBundleIconName` names, and
/// `AppIcon.icns`, which `CFBundleIconFile` names, for what reads no
/// catalog.
fn compile_icon(icon: &Path, resources: &Path) -> Result<()> {
    let partial = std::env::temp_dir().join(format!("bolt-icon-{}.plist", std::process::id()));
    let compiled = run_command(
        Command::new("xcrun")
            .arg("actool")
            .arg(icon)
            .arg("--compile")
            .arg(resources)
            .args(["--platform", "macosx"])
            .args(["--minimum-deployment-target", MINIMUM_SYSTEM])
            .args(["--app-icon", "AppIcon"])
            .args(["--include-all-app-icons", "--errors", "--warnings"])
            .args(["--output-format", "human-readable-text"])
            .arg("--output-partial-info-plist")
            .arg(&partial),
        "compiling the icon with actool",
    );
    let _ = fs::remove_file(&partial);
    compiled?;
    for made in ["Assets.car", "AppIcon.icns"] {
        if !resources.join(made).exists() {
            bail!("actool made no {made} from {}", icon.display());
        }
    }
    Ok(())
}

/// The identity that signs the bundle: the one that `BOLT_SIGN_IDENTITY`
/// names, the project's Developer ID when the keychain has it, or `-`, ad
/// hoc.
fn identity() -> Result<String> {
    if let Some(named) = std::env::var_os(IDENTITY_VARIABLE) {
        return named
            .into_string()
            .map_err(|_| anyhow::anyhow!("{IDENTITY_VARIABLE} is not UTF-8"));
    }
    let listed = Command::new("security")
        .args(["find-identity", "-v", "-p", "codesigning"])
        .output()
        .context("listing the signing identities")?;
    let listing = String::from_utf8_lossy(&listed.stdout);
    Ok(developer_id(&listing).unwrap_or_else(|| "-".to_owned()))
}

/// The project's Developer ID in `listing`, as `security find-identity`
/// lists the identities: the name between quotes of the line of its team.
fn developer_id(listing: &str) -> Option<String> {
    let wanted = format!("({TEAM})");
    listing.lines().find_map(|line| {
        let name = line.split('"').nth(1)?;
        (name.starts_with("Developer ID Application:") && name.ends_with(&wanted))
            .then(|| name.to_owned())
    })
}

/// Signs `app` with `identity`, with the hardened runtime, and checks the
/// signature as Gatekeeper does.
fn sign(app: &Path, identity: &str) -> Result<()> {
    run_command(
        Command::new("codesign")
            .args(["--force", "--options", "runtime", "--sign", identity])
            .arg(app),
        "signing the bundle",
    )?;
    run_command(
        Command::new("codesign")
            .args(["--verify", "--strict", "--verbose=2"])
            .arg(app),
        "checking the bundle's signature",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_project_s_developer_id_is_found_among_others() {
        let listing = concat!(
            "  1) 0123 \"Apple Development: Someone (AAAAAAAAAA)\"\n",
            "  2) 4567 \"Developer ID Application: Someone Else (BBBBBBBBBB)\"\n",
            "  3) 89AB \"Developer ID Application: The Maintainer (XXXXXXXXXX)\"\n",
            "     3 valid identities found\n",
        );
        assert_eq!(
            developer_id(listing).as_deref(),
            Some("Developer ID Application: The Maintainer (XXXXXXXXXX)")
        );
        assert_eq!(developer_id("     0 valid identities found\n"), None);
    }

    #[test]
    fn the_info_plist_has_the_keys_of_the_bundle() {
        let plist = info_plist("1.2.3", true);
        for (key, value) in [
            ("CFBundleIdentifier", IDENTIFIER),
            ("CFBundleExecutable", "bolt"),
            ("CFBundleShortVersionString", "1.2.3"),
            ("LSMinimumSystemVersion", "26.0"),
            ("CFBundleIconName", "AppIcon"),
            (
                "NSMicrophoneUsageDescription",
                "A program started in the terminal wants to use the microphone.",
            ),
        ] {
            assert!(
                plist.contains(&format!("<key>{key}</key>\n\t<string>{value}</string>")),
                "{key}"
            );
        }
        assert!(plist.contains("<key>NSHighResolutionCapable</key>\n\t<true/>"));
        assert!(!info_plist("1.2.3", false).contains("CFBundleIconName"));
    }

    /// The plist reads back as macOS reads it, whole, with each key's value.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_info_plist_is_one_that_macos_reads() {
        let path = std::env::temp_dir().join(format!("bolt-info-{}.plist", std::process::id()));
        fs::write(&path, info_plist("1.2.3", true)).unwrap();
        let lint = Command::new("plutil")
            .arg("-lint")
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            lint.status.success(),
            "{}",
            String::from_utf8_lossy(&lint.stdout)
        );
        let extract = |key: &str| {
            let output = Command::new("plutil")
                .args(["-extract", key, "raw", "-o", "-"])
                .arg(&path)
                .output()
                .unwrap();
            String::from_utf8(output.stdout).unwrap().trim().to_owned()
        };
        assert_eq!(extract("CFBundleIdentifier"), IDENTIFIER);
        assert_eq!(extract("NSHighResolutionCapable"), "true");
        assert_eq!(
            extract("NSLocalNetworkUsageDescription"),
            "A program started in the terminal wants to reach devices on the local network."
        );
        fs::remove_file(path).unwrap();
    }
}
