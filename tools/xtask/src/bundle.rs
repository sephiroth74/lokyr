//! `cargo xtask bundle`: builds `target/Lokyr.app` from the release binary.
//!
//! The bundle holds the binary, its `Info.plist`, the terminfo entry
//! compiled into `Contents/Resources/terminfo`, the icons: the
//! default variant's as the bundle's own, `AppIcon.icns`, and every
//! variant's in `Contents/Resources/icons`, from `crates/app/resources/icons`;
//! and, for each language of the app's catalogs, `<language>.lproj` with
//! the usage strings of its `infoplist.ftl`, which `Info.plist` takes in
//! English.
//! It is signed with the project's Developer ID when the
//! keychain has it, so that macOS keeps the permissions that it gave the
//! app from one build to the next, or with the identity that
//! `LOKYR_SIGN_IDENTITY` names; otherwise, as on a machine without it,
//! ad hoc; with the hardened runtime, and the entitlements of the resources
//! that the programs in the terminal ask for, without which the runtime
//! refuses them without a prompt. `cargo xtask notarize` has it notarized.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

/// The bundle's identifier, which follows the terminal's name.
pub const IDENTIFIER: &str = "io.github.sephiroth74.lokyr";

/// The bundle's name, and the app's in the Finder, the Dock and the menus.
pub const NAME: &str = "Lokyr";

/// The binary, as the app's crate names it, and the bundle's, which the
/// panes' programs run as their command line names it, on a volume that
/// tells the case of names apart too.
const EXECUTABLE: &str = "lokyr";

/// The oldest macOS that the app runs on.
const MINIMUM_SYSTEM: &str = "26.0";

/// The icons' variants, each an SVG and its `.icns`, relative to the
/// workspace root.
const ICONS: &str = "crates/app/resources/icons";

/// The variant that the bundle shows as its own.
const DEFAULT_ICON: &str = "lokyr-jormungandr";

/// The team of the project's Developer ID.
const TEAM: &str = "XXXXXXXXXX";

/// The variable that names another signing identity, or `-` for ad hoc.
const IDENTITY_VARIABLE: &str = "LOKYR_SIGN_IDENTITY";

/// The usage strings that macOS shows when a program started in the
/// terminal asks for what its privacy settings guard, which it asks on the
/// terminal's behalf: the keys of `Info.plist`, which the catalogs'
/// `infoplist.ftl` give their texts.
const USAGES: [&str; 16] = [
    "NSMicrophoneUsageDescription",
    "NSCameraUsageDescription",
    "NSAppleEventsUsageDescription",
    "NSDesktopFolderUsageDescription",
    "NSDocumentsFolderUsageDescription",
    "NSDownloadsFolderUsageDescription",
    "NSRemovableVolumesUsageDescription",
    "NSNetworkVolumesUsageDescription",
    "NSLocalNetworkUsageDescription",
    "NSBluetoothAlwaysUsageDescription",
    "NSContactsUsageDescription",
    "NSCalendarsFullAccessUsageDescription",
    "NSRemindersFullAccessUsageDescription",
    "NSPhotoLibraryUsageDescription",
    "NSLocationUsageDescription",
    "NSSpeechRecognitionUsageDescription",
];

/// The entitlements of the hardened runtime that let a program started in
/// the terminal ask for what macOS guards, which it asks on the terminal's
/// behalf: Apple Events, the microphone, the camera, the contacts, the
/// calendars, the location and the photos. Without them, the runtime
/// refuses each without the prompt that its usage string tells.
const ENTITLEMENTS: [&str; 7] = [
    "com.apple.security.automation.apple-events",
    "com.apple.security.device.audio-input",
    "com.apple.security.device.camera",
    "com.apple.security.personal-information.addressbook",
    "com.apple.security.personal-information.calendars",
    "com.apple.security.personal-information.location",
    "com.apple.security.personal-information.photos-library",
];

/// The app's catalogs, a directory each language, relative to the workspace
/// root.
const LOCALES: &str = "crates/app/resources/locales";

/// The symbols' font that the binary holds, with its license and its
/// icons' licenses, which the bundle carries.
const SYMBOLS_FONT: &str = "crates/text-core/fonts";

/// The language of the source catalogs, which `Info.plist` takes.
const SOURCE: &str = "en";

pub fn run(mut args: impl Iterator<Item = String>) -> Result<ExitCode> {
    // The app's features to build with, as `--features three-drawables`.
    let features = match (args.next(), args.next()) {
        (None, _) => None,
        (Some(flag), Some(features)) if flag == "--features" => Some(features),
        (Some(arg), _) => bail!("bundle takes `--features <features>` alone, not `{arg}`"),
    };
    if let Some(arg) = args.next() {
        bail!("bundle takes `--features <features>` alone, not `{arg}`");
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .context("the workspace's root")?;
    run_command(
        Command::new(env!("CARGO"))
            .args(["build", "--release", "--locked", "-p", "lokyr-app"])
            .args(
                features
                    .iter()
                    .flat_map(|features| ["--features", features]),
            )
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
    copy_icons(&root.join(ICONS), &resources)?;
    copy_font_licenses(&root.join(SYMBOLS_FONT), &resources)?;
    let catalogs = usage_catalogs(&root.join(LOCALES))?;
    for (language, usages) in &catalogs {
        let directory = resources.join(format!("{language}.lproj"));
        fs::create_dir_all(&directory)
            .with_context(|| format!("making {}", directory.display()))?;
        fs::write(directory.join("InfoPlist.strings"), localized_plist(usages))
            .with_context(|| format!("writing the {language} InfoPlist.strings"))?;
    }
    let version = told_version(&binary)?;
    fs::write(
        app.join("Contents/Info.plist"),
        info_plist(&version, &catalogs)?,
    )
    .context("writing the Info.plist")?;
    let identity = identity()?;
    let entitlements = root.join("target").join(format!("{NAME}.entitlements"));
    fs::write(&entitlements, entitlements_plist()).context("writing the entitlements")?;
    sign(&app, &identity, &entitlements)?;
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

/// The usage strings of each language of the catalogs in `locales`, by
/// language: each language's `infoplist.ftl`, of a message for each key of
/// [`USAGES`], each a line `key = text`, without variables.
fn usage_catalogs(locales: &Path) -> Result<BTreeMap<String, Vec<(String, String)>>> {
    let mut catalogs = BTreeMap::new();
    for entry in fs::read_dir(locales).with_context(|| format!("reading {}", locales.display()))? {
        let directory = entry?.path();
        let Some(language) = directory.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let path = directory.join("infoplist.ftl");
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        catalogs.insert(
            language.to_owned(),
            usages(&text).with_context(|| path.display().to_string())?,
        );
    }
    if !catalogs.contains_key(SOURCE) {
        bail!("{} has no catalog of {SOURCE}", locales.display());
    }
    Ok(catalogs)
}

/// The usage strings of `text`, a catalog's `infoplist.ftl`, in the order of
/// [`USAGES`], each of which it has.
fn usages(text: &str) -> Result<Vec<(String, String)>> {
    let mut messages = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(" = ") else {
            bail!("`{line}` is not a message, `key = text`");
        };
        if value.contains(['{', '}']) {
            bail!("the text of `{key}` takes no variable");
        }
        messages.insert(key.to_owned(), value.to_owned());
    }
    USAGES
        .iter()
        .map(|&key| {
            let value = messages
                .remove(key)
                .with_context(|| format!("no message `{key}`"))?;
            Ok((key.to_owned(), value))
        })
        .collect()
}

/// A plist of `pairs`, keys and their strings, with `NSHighResolutionCapable`
/// when `high_resolution`.
fn plist(pairs: &[(String, String)], high_resolution: bool) -> String {
    let mut plist = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    for (key, value) in pairs {
        plist.push_str(&format!(
            "\t<key>{key}</key>\n\t<string>{}</string>\n",
            escaped(value)
        ));
    }
    if high_resolution {
        plist.push_str("\t<key>NSHighResolutionCapable</key>\n\t<true/>\n");
    }
    plist.push_str("</dict>\n</plist>\n");
    plist
}

/// The version that the app's `binary` tells, as `lokyr --version` prints
/// it, the crate's with the date of its commit: `0.1.0-20261101`.
fn told_version(binary: &Path) -> Result<String> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .with_context(|| format!("running {} --version", binary.display()))?;
    let told = String::from_utf8_lossy(&output.stdout);
    told.trim()
        .strip_prefix("lokyr ")
        .map(str::to_owned)
        .with_context(|| format!("{} --version told `{}`", binary.display(), told.trim()))
}

/// The bundle's versions of the app's `version`, the crate's with a date:
/// the crate's, of three numbers as macOS has it, and the date, the build's
/// number, so that About Lokyr shows `0.1.0 (20261101)`.
fn bundle_versions(version: &str) -> Result<(&str, &str)> {
    version
        .split_once('-')
        .filter(|(_, date)| date.len() == 8 && date.bytes().all(|byte| byte.is_ascii_digit()))
        .with_context(|| format!("the version `{version}` has no date"))
}

/// The `Info.plist` of the app at `version`, the crate's with a date, with
/// the usage strings of the source's catalog and the languages of
/// `catalogs`.
fn info_plist(version: &str, catalogs: &BTreeMap<String, Vec<(String, String)>>) -> Result<String> {
    let (short, date) = bundle_versions(version)?;
    let mut keys = vec![
        ("CFBundleDevelopmentRegion", "en".to_owned()),
        ("CFBundleDisplayName", NAME.to_owned()),
        ("CFBundleExecutable", EXECUTABLE.to_owned()),
        ("CFBundleIdentifier", IDENTIFIER.to_owned()),
        ("CFBundleInfoDictionaryVersion", "6.0".to_owned()),
        ("CFBundleName", NAME.to_owned()),
        ("CFBundlePackageType", "APPL".to_owned()),
        ("CFBundleShortVersionString", short.to_owned()),
        ("CFBundleVersion", date.to_owned()),
        ("LSMinimumSystemVersion", MINIMUM_SYSTEM.to_owned()),
        ("NSPrincipalClass", "NSApplication".to_owned()),
    ];
    keys.push(("CFBundleIconFile", "AppIcon".to_owned()));
    let mut pairs: Vec<(String, String)> = keys
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect();
    let source = catalogs
        .get(SOURCE)
        .with_context(|| format!("no catalog of {SOURCE}"))?;
    pairs.extend(source.iter().cloned());
    let mut plist = plist(&pairs, true);
    // The languages that the bundle has, which the system's own texts of
    // the app, as its menus' items, take.
    let languages: String = catalogs
        .keys()
        .map(|language| format!("\t\t<string>{language}</string>\n"))
        .collect();
    let at = plist.rfind("</dict>").expect("the plist's dictionary ends");
    plist.insert_str(
        at,
        &format!("\t<key>CFBundleLocalizations</key>\n\t<array>\n{languages}\t</array>\n"),
    );
    Ok(plist)
}

/// `text` with XML's special characters escaped.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Copies the symbols' font's license and its icons' licenses, of `fonts`,
/// into `resources`' `licenses/symbols-nerd-font`, as the binary holds the
/// font.
fn copy_font_licenses(fonts: &Path, resources: &Path) -> Result<()> {
    let licenses = resources.join("licenses/symbols-nerd-font");
    fs::create_dir_all(&licenses).with_context(|| format!("making {}", licenses.display()))?;
    for name in ["LICENSE", "README.md"] {
        let path = fonts.join(name);
        fs::copy(&path, licenses.join(name))
            .with_context(|| format!("copying {}", path.display()))?;
    }
    Ok(())
}

/// Copies the icons' variants from `icons` into `resources`: each one's
/// `.icns` into `icons`, and the default's as the bundle's own,
/// `AppIcon.icns`, which `CFBundleIconFile` names.
fn copy_icons(icons: &Path, resources: &Path) -> Result<()> {
    let variants = resources.join("icons");
    fs::create_dir_all(&variants).with_context(|| format!("making {}", variants.display()))?;
    let mut found = Vec::new();
    for entry in fs::read_dir(icons).with_context(|| format!("reading {}", icons.display()))? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "icns")
        {
            let name = path.file_name().context("an icon's name")?;
            fs::copy(&path, variants.join(name))
                .with_context(|| format!("copying {}", path.display()))?;
            found.push(path);
        }
    }
    let default = icons.join(format!("{DEFAULT_ICON}.icns"));
    if !found.contains(&default) {
        bail!("no default icon at {}", default.display());
    }
    fs::copy(&default, resources.join("AppIcon.icns"))
        .with_context(|| format!("copying {}", default.display()))?;
    Ok(())
}

/// A language's `InfoPlist.strings`: its usage strings, `usages`, and the
/// app's name, the same in every language, which the Finder, the Dock and
/// the menu bar take from the language's strings once the bundle has them.
fn localized_plist(usages: &[(String, String)]) -> String {
    let mut pairs = vec![
        ("CFBundleDisplayName".to_owned(), NAME.to_owned()),
        ("CFBundleName".to_owned(), NAME.to_owned()),
    ];
    pairs.extend(usages.iter().cloned());
    plist(&pairs, false)
}

/// The identity that signs the bundle: the one that `LOKYR_SIGN_IDENTITY`
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

/// The plist of [`ENTITLEMENTS`], each granted.
fn entitlements_plist() -> String {
    let mut plist = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    for key in ENTITLEMENTS {
        plist.push_str(&format!("\t<key>{key}</key>\n\t<true/>\n"));
    }
    plist.push_str("</dict>\n</plist>\n");
    plist
}

/// Signs `app` with `identity`, with the hardened runtime and the
/// entitlements of the file `entitlements`, and checks the signature as
/// Gatekeeper does.
fn sign(app: &Path, identity: &str, entitlements: &Path) -> Result<()> {
    run_command(
        Command::new("codesign")
            .args(["--force", "--options", "runtime", "--sign", identity])
            .arg("--entitlements")
            .arg(entitlements)
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

    /// The catalogs of the workspace's app.
    fn catalogs() -> BTreeMap<String, Vec<(String, String)>> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        usage_catalogs(&root.join(LOCALES)).unwrap()
    }

    #[test]
    fn the_catalogs_give_every_usage_string_in_every_language() {
        let catalogs = catalogs();
        let languages: Vec<&str> = catalogs.keys().map(String::as_str).collect();
        assert_eq!(languages, ["en", "it"]);
        for usages in catalogs.values() {
            assert_eq!(usages.len(), USAGES.len());
        }
        let italian = &catalogs["it"];
        assert_eq!(italian[0].0, "NSMicrophoneUsageDescription");
        assert_eq!(
            italian[0].1,
            "Un programma avviato nel terminale vuole usare il microfono."
        );
        let strings = plist(italian, false);
        assert!(strings.contains("<key>NSCameraUsageDescription</key>"));
        assert!(!strings.contains("NSHighResolutionCapable"));
        // A catalog that lacks one, or gives one a variable, is refused.
        assert!(usages("NSMicrophoneUsageDescription = x\n").is_err());
        let variable = "NSMicrophoneUsageDescription = { $x }\n";
        assert!(
            usages(variable)
                .unwrap_err()
                .to_string()
                .contains("no variable")
        );
    }

    #[test]
    fn the_symbols_font_s_licenses_go_into_the_bundle() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let resources = std::env::temp_dir().join(format!("lokyr-licenses-{}", std::process::id()));
        copy_font_licenses(&root.join(SYMBOLS_FONT), &resources).unwrap();
        let licenses = resources.join("licenses/symbols-nerd-font");
        let license = fs::read_to_string(licenses.join("LICENSE")).unwrap();
        assert!(license.starts_with("The MIT License"), "{license}");
        let notices = fs::read_to_string(licenses.join("README.md")).unwrap();
        assert!(notices.contains("| Font Awesome |"), "{notices}");
        fs::remove_dir_all(resources).unwrap();
    }

    #[test]
    fn the_bundle_s_versions_are_the_crate_s_and_the_date() {
        assert_eq!(
            bundle_versions("0.1.0-20261101").unwrap(),
            ("0.1.0", "20261101")
        );
        for version in ["0.1.0", "0.1.0-dev", "0.1.0-2026110", "0.1.0-2026110x"] {
            assert!(bundle_versions(version).is_err(), "{version}");
        }
    }

    #[test]
    fn the_info_plist_has_the_keys_of_the_bundle() {
        let plist = info_plist("1.2.3-20261101", &catalogs()).unwrap();
        for (key, value) in [
            ("CFBundleIdentifier", IDENTIFIER),
            ("CFBundleExecutable", "lokyr"),
            ("CFBundleShortVersionString", "1.2.3"),
            ("CFBundleVersion", "20261101"),
            ("LSMinimumSystemVersion", "26.0"),
            ("CFBundleIconFile", "AppIcon"),
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
        assert!(plist.contains(
            "<key>CFBundleLocalizations</key>\n\t<array>\n\t\t<string>en</string>\n\t\t<string>it</string>\n\t</array>"
        ));
    }

    /// A binary signed as the bundle is has the entitlements, as `codesign`
    /// reads them back.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_signature_grants_the_entitlements() {
        let directory = std::env::temp_dir().join(format!("lokyr-sign-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let binary = directory.join("true");
        fs::copy("/usr/bin/true", &binary).unwrap();
        let entitlements = directory.join("test.entitlements");
        fs::write(&entitlements, entitlements_plist()).unwrap();
        sign(&binary, "-", &entitlements).unwrap();
        let read = Command::new("codesign")
            .args(["-d", "--entitlements", "-", "--xml"])
            .arg(&binary)
            .output()
            .unwrap();
        assert!(read.status.success());
        let granted = String::from_utf8_lossy(&read.stdout);
        for key in ENTITLEMENTS {
            assert!(
                granted.contains(&format!("<key>{key}</key><true/>")),
                "{key}: {granted}"
            );
        }
        let flags = Command::new("codesign")
            .args(["-d", "--verbose=2"])
            .arg(&binary)
            .output()
            .unwrap();
        let flags = String::from_utf8_lossy(&flags.stderr);
        // The hardened runtime, among the code directory's flags.
        let runtime = flags
            .lines()
            .any(|line| line.contains("flags=") && line.contains("runtime"));
        assert!(runtime, "{flags}");
        fs::remove_dir_all(directory).unwrap();
    }

    /// Each language's strings name the app as the bundle does.
    #[test]
    fn a_language_s_strings_name_the_app() {
        let usages = vec![(
            "NSCameraUsageDescription".to_owned(),
            "Un programma vuole la fotocamera.".to_owned(),
        )];
        let strings = localized_plist(&usages);
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            assert!(
                strings.contains(&format!("<key>{key}</key>\n\t<string>Lokyr</string>")),
                "{strings}"
            );
        }
        assert!(strings.contains("Un programma vuole la fotocamera."));
    }

    /// The plist reads back as macOS reads it, whole, with each key's value.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_info_plist_is_one_that_macos_reads() {
        let path = std::env::temp_dir().join(format!("lokyr-info-{}.plist", std::process::id()));
        fs::write(&path, info_plist("1.2.3-20261101", &catalogs()).unwrap()).unwrap();
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
