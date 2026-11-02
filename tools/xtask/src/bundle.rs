//! `cargo xtask bundle`: builds `target/Lokyr.app` from the release binary.
//!
//! The bundle holds the binary, its `Info.plist`, the terminfo entry
//! compiled into `Contents/Resources/terminfo`, the icons: the
//! default variant's as the bundle's own, `AppIcon.icns`, and every
//! variant's in `Contents/Resources/icons`, from `crates/app/resources/icons`;
//! and, for each language of the app's catalogs, `<language>.lproj` with
//! the usage strings of its `infoplist.ftl`, which `Info.plist` takes in
//! English; and Sparkle's updater, in `Contents/Frameworks`, which the app
//! loads, and the keys of its feed in `Info.plist`.
//! It is signed with the identity that `LOKYR_SIGN_IDENTITY` names, the
//! maintainer's Developer ID, in the environment or in the untracked
//! `private/lokyr.env`, so that macOS keeps the permissions that it gave the
//! app from one build to the next; without it, as on CI, ad hoc; with the
//! hardened runtime, and the entitlements of the resources that the
//! programs in the terminal ask for, without which the runtime refuses them
//! without a prompt. `cargo xtask notarize` has it notarized. No signing
//! identity, team or credential is in the repository.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::{Command, ExitCode};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};

#[path = "../../../crates/app/build/version.rs"]
mod version;

/// The bundle's identifier, which follows the terminal's name.
pub const IDENTIFIER: &str = "io.github.sephiroth74.lokyr";

/// The bundle's name, and the app's in the Finder, the Dock and the menus.
pub const NAME: &str = "Lokyr";

/// The binary, as the app's crate names it, and the bundle's, which the
/// panes' programs run as their command line names it, on a volume that
/// tells the case of names apart too.
pub(crate) const EXECUTABLE: &str = "lokyr";

/// The oldest macOS that the app runs on.
const MINIMUM_SYSTEM: &str = "26.0";

/// The icons' variants, each an SVG and its `.icns`, relative to the
/// workspace root.
const ICONS: &str = "crates/app/resources/icons";

/// The variant that the bundle shows as its own.
const DEFAULT_ICON: &str = "lokyr-jormungandr";

/// The variable that names the signing identity, as `codesign --sign`
/// takes it, the name of a certificate of the keychain or its hash, or `-`
/// for ad hoc.
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
const LOCALES: &str = "crates/app-core/resources/locales";

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
    let moment = build_moment(&root)?;
    run_command(
        Command::new(env!("CARGO"))
            .args(["build", "--release", "--locked", "-p", "lokyr-app"])
            // The version's date, of the moment of the bundle's number.
            .env("SOURCE_DATE_EPOCH", moment.to_string())
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
    // The binary of this build, not one of another day or release.
    let release = std::env::var("LOKYR_RELEASE")
        .ok()
        .filter(|release| !release.is_empty());
    let built = version::version(env!("CARGO_PKG_VERSION"), release.as_deref(), moment)
        .map_err(anyhow::Error::msg)?;
    ensure!(
        version == built,
        "{} tells `{version}`, where this build gives `{built}`: a binary of another build",
        binary.display()
    );
    fs::write(
        app.join("Contents/Info.plist"),
        info_plist(&version, moment, &catalogs)?,
    )
    .context("writing the Info.plist")?;
    let identity = identity()?;
    let sparkle = crate::sparkle::release(&root.join("target"))?;
    crate::sparkle::embed(&sparkle, &app, &identity)?;
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
/// it, the crate's with the date of its commit: `0.2.0-20261101`.
pub(crate) fn told_version(binary: &Path) -> Result<String> {
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

/// The moment of the build, in seconds since the epoch, as the app's build
/// script takes it: `SOURCE_DATE_EPOCH`'s, when it is set, or else the time
/// of HEAD's commit, or else, for a source without git, now.
fn build_moment(root: &Path) -> Result<u64> {
    if let Ok(given) = std::env::var("SOURCE_DATE_EPOCH") {
        return given
            .trim()
            .parse()
            .with_context(|| format!("SOURCE_DATE_EPOCH is `{given}`, not seconds"));
    }
    let committed = Command::new("git")
        .args(["log", "-1", "--format=%ct"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8_lossy(&output.stdout).trim().parse().ok());
    match committed {
        Some(seconds) => Ok(seconds),
        None => Ok(SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("the clock is before 1970")?
            .as_secs()),
    }
}

/// Whether `text` is of digits alone, one at least.
fn digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// The bundle's versions of the app's `version`, built at `moment`, in
/// seconds since the epoch: the short one, its three numbers without the
/// suffix of a build's date or of a candidate, as macOS shows it, `0.2.0`
/// of `0.2.0-20261101` and `1.0.0` of `1.0.0-rc.2`; and the build's number,
/// its moment in UTC to the minute, `202611011530`, which grows from one
/// commit to the next, as the updates compare it. About Lokyr shows
/// `0.2.0 (202611011530)`.
fn bundle_versions(version: &str, moment: u64) -> Result<(&str, String)> {
    let (short, suffix) = match version.split_once('-') {
        Some((short, suffix)) => (short, Some(suffix)),
        None => (version, None),
    };
    let numbers: Vec<&str> = short.split('.').collect();
    ensure!(
        numbers.len() == 3 && numbers.iter().all(|number| digits(number)),
        "the version `{version}` is not of three numbers"
    );
    let number = version::stamp(moment);
    match suffix {
        None => {}
        Some(date) if date.len() == 8 && digits(date) => ensure!(
            number.starts_with(date),
            "the version `{version}` is of another day than the build, {number}"
        ),
        Some(candidate) if candidate.strip_prefix("rc.").is_some_and(digits) => {}
        Some(other) => {
            bail!(
                "the version `{version}` has a suffix of neither a date nor a candidate, `{other}`"
            )
        }
    }
    Ok((short, number))
}

/// The `Info.plist` of the app at `version`, built at `moment`, with the
/// usage strings of the source's catalog and the languages of `catalogs`.
fn info_plist(
    version: &str,
    moment: u64,
    catalogs: &BTreeMap<String, Vec<(String, String)>>,
) -> Result<String> {
    let (short, number) = bundle_versions(version, moment)?;
    let mut keys = vec![
        ("CFBundleDevelopmentRegion", "en".to_owned()),
        ("CFBundleDisplayName", NAME.to_owned()),
        ("CFBundleExecutable", EXECUTABLE.to_owned()),
        ("CFBundleIdentifier", IDENTIFIER.to_owned()),
        ("CFBundleInfoDictionaryVersion", "6.0".to_owned()),
        ("CFBundleName", NAME.to_owned()),
        ("CFBundlePackageType", "APPL".to_owned()),
        ("CFBundleShortVersionString", short.to_owned()),
        ("CFBundleVersion", number),
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
        &format!(
            "\t<key>CFBundleLocalizations</key>\n\t<array>\n{languages}\t</array>\n{}",
            crate::sparkle::plist_entries()
        ),
    );
    Ok(plist)
}

/// `text` with XML's special characters escaped.
fn escaped(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Copies the symbols' font's license, the notices of its icons' sets, and
/// the texts of the sets' licenses in its `licenses`, of `fonts`, into
/// `resources`' `licenses/symbols-nerd-font`, in the same places, as the
/// binary holds the font.
fn copy_font_licenses(fonts: &Path, resources: &Path) -> Result<()> {
    let licenses = resources.join("licenses/symbols-nerd-font");
    let sets = licenses.join("licenses");
    fs::create_dir_all(&sets).with_context(|| format!("making {}", sets.display()))?;
    for name in ["LICENSE", "README.md"] {
        let path = fonts.join(name);
        fs::copy(&path, licenses.join(name))
            .with_context(|| format!("copying {}", path.display()))?;
    }
    let texts = fonts.join("licenses");
    let entries = fs::read_dir(&texts).with_context(|| format!("reading {}", texts.display()))?;
    for entry in entries {
        let path = entry
            .with_context(|| format!("reading {}", texts.display()))?
            .path();
        let Some(name) = path.file_name() else {
            continue;
        };
        fs::copy(&path, sets.join(name)).with_context(|| format!("copying {}", path.display()))?;
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
/// names, in the environment or in `private/lokyr.env`, or `-`, ad hoc.
pub(crate) fn identity() -> Result<String> {
    Ok(crate::private::variable(IDENTITY_VARIABLE)?
        .filter(|named| !named.is_empty())
        .unwrap_or_else(|| "-".to_owned()))
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

/// Signs `app` with `identity`, with the hardened runtime, the
/// entitlements of the file `entitlements` and a secure timestamp unless ad
/// hoc, over its frameworks signed already, and checks the signature, of
/// the code that it holds too, as Gatekeeper does.
fn sign(app: &Path, identity: &str, entitlements: &Path) -> Result<()> {
    let mut command = Command::new("codesign");
    command.args(["--force", "--options", "runtime", "--sign", identity]);
    if identity != "-" {
        command.arg("--timestamp");
    }
    run_command(
        command.arg("--entitlements").arg(entitlements).arg(app),
        "signing the bundle",
    )?;
    run_command(
        Command::new("codesign")
            .args(["--verify", "--deep", "--strict", "--verbose=2"])
            .arg(app),
        "checking the bundle's signature",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // The sets' licenses, which a copy of their icons carries.
        let text = |name: &str| fs::read_to_string(licenses.join("licenses").join(name)).unwrap();
        assert!(text("apache-2.0.txt").contains("Apache License"));
        assert!(text("pomicons.txt").contains("SIL Open Font License"));
        assert!(text("codicons.txt").starts_with("Attribution 4.0 International"));
        fs::remove_dir_all(resources).unwrap();
    }

    /// 1 November 2026, at 15:30 UTC.
    const MOMENT: u64 = 1_793_547_000;

    #[test]
    fn the_build_s_moment_is_told_to_the_minute_in_utc() {
        assert_eq!(version::stamp(0), "197001010000");
        assert_eq!(version::stamp(MOMENT + 59), "202611011530");
        // A leap day's last minute, and the first day after a century's
        // leap February.
        assert_eq!(version::stamp(1_709_251_140), "202402292359");
        assert_eq!(version::stamp(951_868_800), "200003010000");
    }

    #[test]
    fn the_bundle_s_versions_are_the_short_one_and_the_build_s_moment() {
        for (version, short) in [
            ("0.2.0-20261101", "0.2.0"),
            ("1.0.0-rc.2", "1.0.0"),
            ("1.0.0-rc.10", "1.0.0"),
            ("1.0.0", "1.0.0"),
        ] {
            let (told, number) = bundle_versions(version, MOMENT).unwrap();
            assert_eq!(told, short, "{version}");
            assert_eq!(number, "202611011530", "{version}");
        }
        for version in [
            "0.2.0-20261102",
            "0.2",
            "0.2.0.1",
            "0.x.0",
            "0.2.0-dev",
            "0.2.0-2026110",
            "0.2.0-rc.",
            "0.2.0-rc.x",
        ] {
            assert!(bundle_versions(version, MOMENT).is_err(), "{version}");
        }
    }

    #[test]
    fn a_nightly_s_a_candidate_s_and_a_release_s_versions_reach_the_bundle() {
        for (release, told, short) in [
            (None, "1.2.3-20261101", "1.2.3"),
            (Some("1.2.3-rc.2"), "1.2.3-rc.2", "1.2.3"),
            (Some("1.2.3-rc.10"), "1.2.3-rc.10", "1.2.3"),
            (Some("1.2.3"), "1.2.3", "1.2.3"),
        ] {
            let version = version::version("1.2.3", release, MOMENT).unwrap();
            assert_eq!(version, told, "{release:?}");
            let (bundled, number) = bundle_versions(&version, MOMENT).unwrap();
            assert_eq!(
                (bundled, number.as_str()),
                (short, "202611011530"),
                "{release:?}"
            );
        }
        // Of other numbers than the crate's, or of another form.
        for release in [
            "1.2.4",
            "1.2.3-rc.",
            "1.2.3-rc.x",
            "1.2.3-beta.1",
            "1.2.3-rc.1-x",
            "1.2",
        ] {
            assert!(
                version::version("1.2.3", Some(release), MOMENT).is_err(),
                "{release}"
            );
        }
    }

    #[test]
    fn the_info_plist_has_the_keys_of_the_bundle() {
        let plist = info_plist("1.2.3-20261101", MOMENT, &catalogs()).unwrap();
        for (key, value) in [
            ("CFBundleIdentifier", IDENTIFIER),
            ("CFBundleExecutable", "lokyr"),
            ("CFBundleShortVersionString", "1.2.3"),
            ("CFBundleVersion", "202611011530"),
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
        assert!(plist.contains("<key>SUPublicEDKey</key>"));
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
        fs::write(
            &path,
            info_plist("1.2.3-20261101", MOMENT, &catalogs()).unwrap(),
        )
        .unwrap();
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
        assert_eq!(extract("CFBundleShortVersionString"), "1.2.3");
        // The build's number, of twelve digits.
        assert_eq!(extract("CFBundleVersion"), "202611011530");
        assert_eq!(extract("NSHighResolutionCapable"), "true");
        // Sparkle's feed, a check a day, and nothing of the Mac sent.
        assert_eq!(extract("SUFeedURL"), "https://lokyr.dev/appcast.xml");
        assert_eq!(extract("SUScheduledCheckInterval"), "86400");
        assert_eq!(extract("SUEnableAutomaticChecks"), "true");
        assert_eq!(extract("SUEnableSystemProfiling"), "false");
        assert_eq!(
            extract("NSLocalNetworkUsageDescription"),
            "A program started in the terminal wants to reach devices on the local network."
        );
        fs::remove_file(path).unwrap();
    }
}
