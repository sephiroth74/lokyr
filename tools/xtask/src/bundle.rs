//! `cargo xtask bundle`: builds `target/Lokyr.app` from the release binary.
//!
//! The bundle holds the binary, its `Info.plist`, the terminfo entry
//! compiled into `Contents/Resources/terminfo`, the icons: the
//! default variant's as the bundle's own, `AppIcon.icns`, and every
//! variant's in `Contents/Resources/icons`, from `crates/app/resources/icons`;
//! and, for each language of the app's catalogs, `<language>.lproj` with
//! the usage strings of its `infoplist.ftl`, which `Info.plist` takes in
//! English; and Sparkle's updater, in `Contents/Frameworks`, which the app
//! loads, and the keys of its feed in `Info.plist`; and in
//! `Contents/Resources/licenses` the notices of the licenses of what the
//! binary and the bundle hold of other projects: the crates', Sparkle's and
//! the symbols' font's; and the shells' integration, its scripts for zsh,
//! bash and fish, in `Contents/Resources/shell-integration`.
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
pub(crate) mod version;

/// The bundle's identifier, which follows the terminal's name.
pub const IDENTIFIER: &str = "io.github.sephiroth74.lokyr";

/// The bundle's name, and the app's in the Finder, the Dock and the menus.
pub const NAME: &str = "Lokyr";

/// The binary, as the app's crate names it, and the bundle's, which the
/// panes' programs run as their command line names it, on a volume that
/// tells the case of names apart too.
pub(crate) const EXECUTABLE: &str = "lokyr";

/// The name of Sleip's executable, beside the app's in the bundle's
/// `Contents/MacOS`.
pub(crate) const SLEIP: &str = "sleip";

/// The vendored sources of Luau's type checker, whose license and Lua's,
/// of its VM's headers, the bundle carries, as Sleip holds them.
const LUAU_ANALYSIS: &str = "sleip/analyzer/luau";

/// The oldest macOS that the app runs on.
const MINIMUM_SYSTEM: &str = "26.0";

/// The icons' variants, each an SVG and its `.icns`, relative to the
/// workspace root.
const ICONS: &str = "crates/app/resources/icons";

/// The license, relative to the workspace root, whose line of the copyright
/// the bundle's `NSHumanReadableCopyright` takes, which the About panel
/// shows.
const LICENSE: &str = "LICENSE";

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

/// The shells' integration's scripts, which the bundle copies whole, of
/// their names that start with a dot too, as zsh's `.zshenv`.
const SHELL_INTEGRATION: &str = "crates/pty/shell-integration";

/// The file of the bundle's resources that tells the plugin API's version,
/// which Sleip reads.
pub(crate) const API_VERSION_FILE: &str = "api-version";

/// Copies the tree of `from` into `to`, its files and its folders.
fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("making {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("reading {}", from.display()))? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)
                .with_context(|| format!("copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// The files that the app opens: each its name's message in the catalogs'
/// `infoplist.ftl`, its type's identifier, and the app's rank among the
/// apps that open it. The themes' files, which it asks to install: iTerm2's
/// color presets, `.itermcolors` files, of `Alternate`, which a double
/// click opens with the app where no app of a higher rank claims them, and
/// which the app's Open With menu has; TOML's of `None`, which the app
/// opens only as they are dropped on its icon. The folders, of `None`, a
/// window each as they are dropped on its icon, the Finder's own opening
/// of a folder left as it is. The shell scripts, Terminal's `.command`
/// and `.tool` and `.sh`, and the executables, of the Shell role and
/// `Alternate`, which the app opens once the user makes it the default
/// terminal, each run in a window of its own. Each its types, its rank and
/// its role.
const DOCUMENT_TYPES: [(&str, &[&str], &str, &str); 5] = [
    (
        "document-type-itermcolors",
        &[ITERMCOLORS],
        "Alternate",
        "Viewer",
    ),
    ("document-type-toml", &["public.toml"], "None", "Viewer"),
    ("document-type-folder", &["public.folder"], "None", "Viewer"),
    (
        "document-type-shell-script",
        &[TERMINAL_SCRIPT, "public.shell-script"],
        "Alternate",
        "Shell",
    ),
    (
        "document-type-executable",
        &["public.unix-executable"],
        "Alternate",
        "Shell",
    ),
];

/// Terminal's type of the shell scripts that it runs, `.command` and
/// `.tool`, which the bundle declares as it imports it.
const TERMINAL_SCRIPT: &str = "com.apple.terminal.shell-script";

/// The schemes of the links that the app opens once the user makes it the
/// default terminal: each its name's message in the catalogs'
/// `infoplist.ftl`, and its scheme.
const URL_TYPES: [(&str, &str); 2] = [("url-type-ssh", "ssh"), ("url-type-man-page", "x-man-page")];

/// The services that the app gives the Finder's folders, in its menu of a
/// folder and in the Services menu: each its title's message in the
/// catalogs' `infoplist.ftl`, and the message of the app's provider that
/// it sends, a window or a tab at each folder.
const SERVICES: [(&str, &str); 2] = [
    ("service-new-window", "openWindow"),
    ("service-new-tab", "openTab"),
];

/// The type of iTerm2's color presets, which the bundle declares, as the
/// system has none of them: property lists of the extension `itermcolors`.
const ITERMCOLORS: &str = "com.googlecode.iterm2.itermcolors";

/// The entitlements of the hardened runtime that let a program started in
/// the terminal ask for what macOS guards, which it asks on the terminal's
/// behalf: Apple Events, the microphone, the camera, the contacts, the
/// calendars, the location and the photos. Without them, the runtime
/// refuses each without the prompt that its usage string tells. And the
/// one exception of the hardened runtime, `allow-jit`, which lets Luau's
/// native code generator map its code with `MAP_JIT`, the plugins of Luau
/// running in its interpreter without it.
const ENTITLEMENTS: [&str; 8] = [
    "com.apple.security.cs.allow-jit",
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
    // The app's features to build with, as `--features three-drawables`,
    // and without its defaults, as the minimal distribution's build.
    let (mut features, mut bare) = (None, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--features" if features.is_none() => match args.next() {
                Some(list) => features = Some(list),
                None => bail!("`--features` takes the features"),
            },
            "--no-default-features" if !bare => bare = true,
            _ => bail!(
                "bundle takes `--features <features>` and `--no-default-features` alone, not `{arg}`"
            ),
        }
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
            .args(bare.then_some("--no-default-features"))
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
    // Sleip, which the app's plugin manager runs, of the app's plugin API
    // that the bundle's resources tell.
    run_command(
        Command::new(env!("CARGO"))
            .args(["build", "--release", "--locked", "-p", "sleip-cli"])
            .env("SOURCE_DATE_EPOCH", moment.to_string())
            .current_dir(&root),
        "building Sleip",
    )?;
    let sleip = root.join("target/release").join(SLEIP);
    fs::copy(&sleip, macos.join(SLEIP)).with_context(|| format!("copying {}", sleip.display()))?;
    let analysis = resources.join("licenses/luau-analysis");
    fs::create_dir_all(&analysis).with_context(|| format!("making {}", analysis.display()))?;
    for name in ["LICENSE.txt", "lua_LICENSE.txt", "README.md"] {
        let path = root.join(LUAU_ANALYSIS).join(name);
        fs::copy(&path, analysis.join(name))
            .with_context(|| format!("copying {}", path.display()))?;
    }
    crate::terminfo::compile(
        &root.join(crate::terminfo::SOURCE),
        &resources.join("terminfo"),
    )?;
    copy_icons(&root.join(ICONS), &resources)?;
    copy_tree(
        &root.join(SHELL_INTEGRATION),
        &resources.join("shell-integration"),
    )?;
    copy_font_licenses(&root.join(SYMBOLS_FONT), &resources)?;
    crate::themes::write(&root, &resources)?;
    // The plugin API's version, which Sleip resolves the plugins against,
    // read without starting the app.
    fs::write(
        resources.join(API_VERSION_FILE),
        format!("{}\n", lokyr_kernel::plugins::API_VERSION),
    )
    .context("writing the plugin API's version")?;
    crate::licenses::write(&root, &resources.join("licenses/crates"))?;
    let catalogs = usage_catalogs(&root.join(LOCALES))?;
    let source = &catalogs[SOURCE];
    for (language, catalog) in &catalogs {
        let directory = resources.join(format!("{language}.lproj"));
        fs::create_dir_all(&directory)
            .with_context(|| format!("making {}", directory.display()))?;
        fs::write(
            directory.join("InfoPlist.strings"),
            localized_plist(catalog, source),
        )
        .with_context(|| format!("writing the {language} InfoPlist.strings"))?;
        fs::write(
            directory.join("ServicesMenu.strings"),
            services_strings(catalog, source),
        )
        .with_context(|| format!("writing the {language} ServicesMenu.strings"))?;
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
    let license = root.join(LICENSE);
    let license =
        fs::read_to_string(&license).with_context(|| format!("reading {}", license.display()))?;
    fs::write(
        app.join("Contents/Info.plist"),
        info_plist(&version, moment, &copyright(&license)?, &catalogs)?,
    )
    .context("writing the Info.plist")?;
    let identity = identity()?;
    let sparkle = crate::sparkle::release(&root.join("target"))?;
    crate::sparkle::embed(&sparkle, &app, &identity)?;
    let entitlements = root.join("target").join(format!("{NAME}.entitlements"));
    fs::write(&entitlements, entitlements_plist(&ENTITLEMENTS))
        .context("writing the entitlements")?;
    // Sleip first, nested code that the bundle's signature seals, of
    // `allow-jit` alone, as `sleip test` runs Luau's native code.
    let sleip_entitlements = root.join("target/sleip.entitlements");
    fs::write(
        &sleip_entitlements,
        entitlements_plist(&["com.apple.security.cs.allow-jit"]),
    )
    .context("writing Sleip's entitlements")?;
    sign_code(
        &macos.join(SLEIP),
        &identity,
        &sleip_entitlements,
        "signing Sleip",
    )?;
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
    println!(
        "bundle: {} of {} (the app's binary {}, Sleip's {})",
        app.display(),
        megabytes(size_of(&app)),
        megabytes(size_of(&macos.join(EXECUTABLE))),
        megabytes(size_of(&macos.join(SLEIP)))
    );
    Ok(ExitCode::SUCCESS)
}

/// The bytes of the file or the folder at `path`, its files' whole.
pub(crate) fn size_of(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !metadata.is_dir() {
        return metadata.len();
    }
    fs::read_dir(path).map_or(0, |entries| {
        entries.flatten().map(|entry| size_of(&entry.path())).sum()
    })
}

/// `bytes` in megabytes, of a decimal, as a release's notes tell a size.
pub(crate) fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Runs `command`, which `what` describes, and fails if it fails.
fn run_command(command: &mut Command, what: &str) -> Result<()> {
    let status = command.status().with_context(|| what.to_owned())?;
    if !status.success() {
        bail!("{what} failed: {status}");
    }
    Ok(())
}

/// A language's texts of the bundle's `Info.plist`: its usage strings, of
/// the keys of [`USAGES`], and its document types' names, of the messages
/// of [`DOCUMENT_TYPES`], each in its order.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Catalog {
    usages: Vec<(String, String)>,
    types: Vec<String>,
    services: Vec<String>,
    urls: Vec<String>,
}

/// The texts of each language of the catalogs in `locales`, by language:
/// each language's `infoplist.ftl`, of a message for each key of
/// [`USAGES`] and each document type's name, each a line `key = text`,
/// without variables.
fn usage_catalogs(locales: &Path) -> Result<BTreeMap<String, Catalog>> {
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
            catalog(&text).with_context(|| path.display().to_string())?,
        );
    }
    if !catalogs.contains_key(SOURCE) {
        bail!("{} has no catalog of {SOURCE}", locales.display());
    }
    Ok(catalogs)
}

/// The texts of `text`, a catalog's `infoplist.ftl`: the usage strings, in
/// the order of [`USAGES`], and the document types' names, in the order of
/// [`DOCUMENT_TYPES`], each of which it has.
fn catalog(text: &str) -> Result<Catalog> {
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
    let mut take = |key: &str| {
        messages
            .remove(key)
            .with_context(|| format!("no message `{key}`"))
    };
    let usages = USAGES
        .iter()
        .map(|&key| Ok((key.to_owned(), take(key)?)))
        .collect::<Result<_>>()?;
    let types = DOCUMENT_TYPES
        .iter()
        .map(|&(message, ..)| take(message))
        .collect::<Result<_>>()?;
    let urls = URL_TYPES
        .iter()
        .map(|&(message, _)| take(message))
        .collect::<Result<_>>()?;
    let services = SERVICES
        .iter()
        .map(|&(message, _)| take(message))
        .collect::<Result<_>>()?;
    Ok(Catalog {
        usages,
        types,
        services,
        urls,
    })
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

/// The copyright of the license's text, `license`: its line
/// `Copyright (c) …`, of the sign in place of `(c)`.
fn copyright(license: &str) -> Result<String> {
    let line = license
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Copyright (c) "))
        .context("the license has no line `Copyright (c) …`")?;
    Ok(line.replacen("(c)", "©", 1))
}

/// The `Info.plist` of the app at `version`, built at `moment`, of
/// `copyright`, with the usage strings of the source's catalog and the
/// languages of `catalogs`.
fn info_plist(
    version: &str,
    moment: u64,
    copyright: &str,
    catalogs: &BTreeMap<String, Catalog>,
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
        ("NSHumanReadableCopyright", copyright.to_owned()),
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
    pairs.extend(source.usages.iter().cloned());
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
            "\t<key>CFBundleLocalizations</key>\n\t<array>\n{languages}\t</array>\n{}{}{}",
            document_types(&source.types, &source.urls),
            services(&source.services),
            crate::sparkle::plist_entries()
        ),
    );
    Ok(plist)
}

/// The entries of `Info.plist` of the files and the links that the app
/// opens, of the source's names of [`DOCUMENT_TYPES`], `names`, and of
/// [`URL_TYPES`], `links`: the document types, the declarations of the
/// types of other apps that it takes, iTerm2's color presets and
/// Terminal's shell scripts, and the links' schemes.
fn document_types(names: &[String], links: &[String]) -> String {
    let mut entries = String::from("\t<key>CFBundleDocumentTypes</key>\n\t<array>\n");
    for ((_, identifiers, rank, role), name) in DOCUMENT_TYPES.iter().zip(names) {
        let identifiers: String = identifiers
            .iter()
            .map(|identifier| format!("\t\t\t\t<string>{identifier}</string>\n"))
            .collect();
        entries.push_str(&format!(
            concat!(
                "\t\t<dict>\n",
                "\t\t\t<key>CFBundleTypeName</key>\n\t\t\t<string>{name}</string>\n",
                "\t\t\t<key>CFBundleTypeRole</key>\n\t\t\t<string>{role}</string>\n",
                "\t\t\t<key>LSHandlerRank</key>\n\t\t\t<string>{rank}</string>\n",
                "\t\t\t<key>LSItemContentTypes</key>\n",
                "\t\t\t<array>\n{identifiers}\t\t\t</array>\n",
                "\t\t</dict>\n",
            ),
            name = escaped(name),
            role = role,
            rank = rank,
            identifiers = identifiers,
        ));
    }
    entries.push_str("\t</array>\n");
    let name_of = |type_identifier: &str| {
        DOCUMENT_TYPES
            .iter()
            .zip(names)
            .find(|((_, identifiers, ..), _)| identifiers.contains(&type_identifier))
            .map_or("", |(_, name)| name.as_str())
    };
    let imported = |identifier: &str, conforms: &str, extensions: &[&str]| {
        let extensions: String = extensions
            .iter()
            .map(|extension| format!("\t\t\t\t\t<string>{extension}</string>\n"))
            .collect();
        format!(
            concat!(
                "\t\t<dict>\n",
                "\t\t\t<key>UTTypeIdentifier</key>\n\t\t\t<string>{identifier}</string>\n",
                "\t\t\t<key>UTTypeDescription</key>\n\t\t\t<string>{name}</string>\n",
                "\t\t\t<key>UTTypeConformsTo</key>\n",
                "\t\t\t<array>\n\t\t\t\t<string>{conforms}</string>\n\t\t\t</array>\n",
                "\t\t\t<key>UTTypeTagSpecification</key>\n\t\t\t<dict>\n",
                "\t\t\t\t<key>public.filename-extension</key>\n",
                "\t\t\t\t<array>\n{extensions}\t\t\t\t</array>\n",
                "\t\t\t</dict>\n",
                "\t\t</dict>\n",
            ),
            identifier = identifier,
            name = escaped(name_of(identifier)),
            conforms = conforms,
            extensions = extensions,
        )
    };
    entries.push_str("\t<key>UTImportedTypeDeclarations</key>\n\t<array>\n");
    entries.push_str(&imported(
        ITERMCOLORS,
        "com.apple.property-list",
        &["itermcolors"],
    ));
    entries.push_str(&imported(
        TERMINAL_SCRIPT,
        "public.shell-script",
        &["command", "tool"],
    ));
    entries.push_str("\t</array>\n");
    entries.push_str("\t<key>CFBundleURLTypes</key>\n\t<array>\n");
    for ((_, scheme), name) in URL_TYPES.iter().zip(links) {
        entries.push_str(&format!(
            concat!(
                "\t\t<dict>\n",
                "\t\t\t<key>CFBundleURLName</key>\n\t\t\t<string>{name}</string>\n",
                "\t\t\t<key>CFBundleTypeRole</key>\n\t\t\t<string>Viewer</string>\n",
                "\t\t\t<key>CFBundleURLSchemes</key>\n",
                "\t\t\t<array>\n\t\t\t\t<string>{scheme}</string>\n\t\t\t</array>\n",
                "\t\t</dict>\n",
            ),
            name = escaped(name),
            scheme = scheme,
        ));
    }
    entries.push_str("\t</array>\n");
    entries
}

/// The entries of `Info.plist` of the services that the app gives, of the
/// source's titles of [`SERVICES`], `titles`: each on the folders that the
/// Finder selected, on by default, as its required context, empty, makes
/// it, its title localized by each language's `ServicesMenu.strings`.
fn services(titles: &[String]) -> String {
    let mut entries = String::from("\t<key>NSServices</key>\n\t<array>\n");
    for ((_, message), title) in SERVICES.iter().zip(titles) {
        entries.push_str(&format!(
            concat!(
                "\t\t<dict>\n",
                "\t\t\t<key>NSMenuItem</key>\n",
                "\t\t\t<dict>\n\t\t\t\t<key>default</key>\n\t\t\t\t<string>{title}</string>\n\t\t\t</dict>\n",
                "\t\t\t<key>NSMessage</key>\n\t\t\t<string>{message}</string>\n",
                "\t\t\t<key>NSPortName</key>\n\t\t\t<string>{NAME}</string>\n",
                "\t\t\t<key>NSRequiredContext</key>\n\t\t\t<dict/>\n",
                "\t\t\t<key>NSSendFileTypes</key>\n",
                "\t\t\t<array>\n\t\t\t\t<string>public.folder</string>\n\t\t\t</array>\n",
                "\t\t</dict>\n",
            ),
            title = escaped(title),
            message = message,
            NAME = NAME,
        ));
    }
    entries.push_str("\t</array>\n");
    entries
}

/// A language's `ServicesMenu.strings`, of its `catalog`: its services'
/// titles, each keyed by the title of `source`, the catalog of
/// `Info.plist`'s own, as the system looks them up.
fn services_strings(catalog: &Catalog, source: &Catalog) -> String {
    let pairs: Vec<(String, String)> = source
        .services
        .iter()
        .map(|title| escaped(title))
        .zip(catalog.services.iter().cloned())
        .collect();
    plist(&pairs, false)
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

/// A language's `InfoPlist.strings`, of its `catalog`: its usage strings;
/// the app's name, the same in every language, which the Finder, the Dock
/// and the menu bar take from the language's strings once the bundle has
/// them; and its document types' and links' names, each keyed by the
/// name of `source`, the catalog of `Info.plist`'s own, as the system
/// looks them up.
fn localized_plist(catalog: &Catalog, source: &Catalog) -> String {
    let mut pairs = vec![
        ("CFBundleDisplayName".to_owned(), NAME.to_owned()),
        ("CFBundleName".to_owned(), NAME.to_owned()),
    ];
    pairs.extend(catalog.usages.iter().cloned());
    pairs.extend(
        source
            .types
            .iter()
            .map(|name| escaped(name))
            .zip(catalog.types.iter().cloned()),
    );
    pairs.extend(
        source
            .urls
            .iter()
            .map(|name| escaped(name))
            .zip(catalog.urls.iter().cloned()),
    );
    plist(&pairs, false)
}

/// The identity that signs the bundle: the one that `LOKYR_SIGN_IDENTITY`
/// names, in the environment or in `private/lokyr.env`, or `-`, ad hoc.
pub(crate) fn identity() -> Result<String> {
    Ok(crate::private::variable(IDENTITY_VARIABLE)?
        .filter(|named| !named.is_empty())
        .unwrap_or_else(|| "-".to_owned()))
}

/// The plist of `entitlements`, each granted, as [`ENTITLEMENTS`], the
/// app's.
fn entitlements_plist(entitlements: &[&str]) -> String {
    let mut plist = String::from(concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" ",
        "\"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n",
        "<plist version=\"1.0\">\n<dict>\n",
    ));
    for key in entitlements {
        plist.push_str(&format!("\t<key>{key}</key>\n\t<true/>\n"));
    }
    plist.push_str("</dict>\n</plist>\n");
    plist
}

/// Signs the code at `path`, a bundle or an executable, with `identity`, of
/// the hardened runtime, the entitlements of the file `entitlements` and a
/// secure timestamp unless ad hoc, as `what` tells the step.
fn sign_code(path: &Path, identity: &str, entitlements: &Path, what: &str) -> Result<()> {
    let mut command = Command::new("codesign");
    command.args(["--force", "--options", "runtime", "--sign", identity]);
    if identity != "-" {
        command.arg("--timestamp");
    }
    run_command(
        command.arg("--entitlements").arg(entitlements).arg(path),
        what,
    )
}

/// Signs `app` with `identity`, with the hardened runtime, the
/// entitlements of the file `entitlements` and a secure timestamp unless ad
/// hoc, over its frameworks signed already, and checks the signature, of
/// the code that it holds too, as Gatekeeper does.
fn sign(app: &Path, identity: &str, entitlements: &Path) -> Result<()> {
    sign_code(app, identity, entitlements, "signing the bundle")?;
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
    fn catalogs() -> BTreeMap<String, Catalog> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        usage_catalogs(&root.join(LOCALES)).unwrap()
    }

    #[test]
    fn the_catalogs_give_every_usage_string_in_every_language() {
        let catalogs = catalogs();
        let languages: Vec<&str> = catalogs.keys().map(String::as_str).collect();
        assert_eq!(languages, ["en", "it"]);
        for catalog in catalogs.values() {
            assert_eq!(catalog.usages.len(), USAGES.len());
            assert_eq!(catalog.types.len(), DOCUMENT_TYPES.len());
            assert_eq!(catalog.services.len(), SERVICES.len());
            assert_eq!(catalog.urls.len(), URL_TYPES.len());
        }
        assert_eq!(catalogs["it"].services[0], "Nuova finestra Lokyr qui");
        let italian = &catalogs["it"].usages;
        assert_eq!(italian[0].0, "NSMicrophoneUsageDescription");
        assert_eq!(
            italian[0].1,
            "Un programma avviato nel terminale vuole usare il microfono."
        );
        let strings = plist(italian, false);
        assert!(strings.contains("<key>NSCameraUsageDescription</key>"));
        assert!(!strings.contains("NSHighResolutionCapable"));
        // A catalog that lacks one, or gives one a variable, is refused.
        assert!(catalog("NSMicrophoneUsageDescription = x\n").is_err());
        let variable = "NSMicrophoneUsageDescription = { $x }\n";
        assert!(
            catalog(variable)
                .unwrap_err()
                .to_string()
                .contains("no variable")
        );
    }

    #[test]
    fn the_symbols_font_s_licenses_go_into_the_bundle() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let resources =
            std::env::temp_dir().join(format!("lokyr-bundle-licenses-{}", std::process::id()));
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
    fn the_copyright_is_the_license_s_line_of_the_sign() {
        let license =
            "MIT License\n\nCopyright (c) 2026 Ada Lovelace\n\nPermission is hereby granted";
        assert_eq!(copyright(license).unwrap(), "Copyright © 2026 Ada Lovelace");
        assert!(copyright("MIT License\n").is_err());
        // The workspace's own.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let own = fs::read_to_string(root.join(LICENSE)).unwrap();
        assert!(copyright(&own).unwrap().starts_with("Copyright © 2026 "));
    }

    #[test]
    fn the_info_plist_has_the_keys_of_the_bundle() {
        let plist = info_plist(
            "1.2.3-20261101",
            MOMENT,
            "Copyright © 2026 Ada",
            &catalogs(),
        )
        .unwrap();
        for (key, value) in [
            ("CFBundleIdentifier", IDENTIFIER),
            ("CFBundleExecutable", "lokyr"),
            ("CFBundleShortVersionString", "1.2.3"),
            ("CFBundleVersion", "202611011530"),
            ("LSMinimumSystemVersion", "26.0"),
            ("CFBundleIconFile", "AppIcon"),
            ("NSHumanReadableCopyright", "Copyright © 2026 Ada"),
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
        fs::write(&entitlements, entitlements_plist(&ENTITLEMENTS)).unwrap();
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
        let source = Catalog {
            usages: Vec::new(),
            types: vec!["Color preset & more".to_owned()],
            services: vec!["New Window <Here>".to_owned()],
            urls: vec!["SSH & co.".to_owned()],
        };
        let italian = Catalog {
            usages: vec![(
                "NSCameraUsageDescription".to_owned(),
                "Un programma vuole la fotocamera.".to_owned(),
            )],
            types: vec!["Preset di colori".to_owned()],
            services: vec!["Nuova finestra & co.".to_owned()],
            urls: vec!["Link SSH".to_owned()],
        };
        let strings = localized_plist(&italian, &source);
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            assert!(
                strings.contains(&format!("<key>{key}</key>\n\t<string>Lokyr</string>")),
                "{strings}"
            );
        }
        assert!(strings.contains("Un programma vuole la fotocamera."));
        // A document type's name, keyed by the source's, escaped.
        assert!(
            strings.contains(
                "<key>Color preset &amp; more</key>\n\t<string>Preset di colori</string>"
            ),
            "{strings}"
        );
        // A link's name, keyed alike.
        assert!(
            strings.contains("<key>SSH &amp; co.</key>\n\t<string>Link SSH</string>"),
            "{strings}"
        );
        // A service's title, of the services' own strings, keyed alike.
        assert!(!strings.contains("Nuova finestra"), "{strings}");
        let services = services_strings(&italian, &source);
        assert!(
            services.contains(
                "<key>New Window &lt;Here&gt;</key>\n\t<string>Nuova finestra &amp; co.</string>"
            ),
            "{services}"
        );
    }

    /// The plist reads back as macOS reads it, whole, with each key's value.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_info_plist_is_one_that_macos_reads() {
        let path = std::env::temp_dir().join(format!("lokyr-info-{}.plist", std::process::id()));
        fs::write(
            &path,
            info_plist(
                "1.2.3-20261101",
                MOMENT,
                "Copyright © 2026 Ada",
                &catalogs(),
            )
            .unwrap(),
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
        // The themes' files, iTerm2's color presets of a type that the
        // bundle declares, and TOML's, which only a drop opens.
        assert_eq!(
            extract("CFBundleDocumentTypes.0.LSItemContentTypes.0"),
            ITERMCOLORS
        );
        assert_eq!(
            extract("CFBundleDocumentTypes.0.LSHandlerRank"),
            "Alternate"
        );
        assert_eq!(
            extract("CFBundleDocumentTypes.0.CFBundleTypeRole"),
            "Viewer"
        );
        assert_eq!(
            extract("CFBundleDocumentTypes.1.LSItemContentTypes.0"),
            "public.toml"
        );
        assert_eq!(extract("CFBundleDocumentTypes.1.LSHandlerRank"), "None");
        // The folders, a window each as they are dropped on the icon, the
        // Finder's opener of folders left as it is.
        assert_eq!(
            extract("CFBundleDocumentTypes.2.LSItemContentTypes.0"),
            "public.folder"
        );
        assert_eq!(extract("CFBundleDocumentTypes.2.LSHandlerRank"), "None");
        // The scripts and the executables, of the Shell role, which the app
        // opens once it is the default terminal, and their links.
        assert_eq!(
            extract("CFBundleDocumentTypes.3.LSItemContentTypes.0"),
            TERMINAL_SCRIPT
        );
        assert_eq!(
            extract("CFBundleDocumentTypes.3.LSItemContentTypes.1"),
            "public.shell-script"
        );
        assert_eq!(extract("CFBundleDocumentTypes.3.CFBundleTypeRole"), "Shell");
        assert_eq!(
            extract("CFBundleDocumentTypes.3.LSHandlerRank"),
            "Alternate"
        );
        assert_eq!(
            extract("CFBundleDocumentTypes.4.LSItemContentTypes.0"),
            "public.unix-executable"
        );
        assert_eq!(extract("CFBundleDocumentTypes.4.CFBundleTypeRole"), "Shell");
        assert_eq!(
            extract("UTImportedTypeDeclarations.1.UTTypeIdentifier"),
            TERMINAL_SCRIPT
        );
        assert_eq!(
            extract(
                "UTImportedTypeDeclarations.1.UTTypeTagSpecification.public\\.filename-extension.1"
            ),
            "tool"
        );
        assert_eq!(
            extract("UTImportedTypeDeclarations.1.UTTypeDescription"),
            "Shell Script"
        );
        assert_eq!(extract("CFBundleURLTypes.0.CFBundleURLSchemes.0"), "ssh");
        assert_eq!(
            extract("CFBundleURLTypes.0.CFBundleURLName"),
            "Secure Shell Link"
        );
        assert_eq!(
            extract("CFBundleURLTypes.1.CFBundleURLSchemes.0"),
            "x-man-page"
        );
        // The services on the Finder's folders, on by default.
        assert_eq!(
            extract("NSServices.0.NSMenuItem.default"),
            "New Lokyr Window Here"
        );
        assert_eq!(extract("NSServices.0.NSMessage"), "openWindow");
        assert_eq!(extract("NSServices.0.NSPortName"), "Lokyr");
        assert_eq!(extract("NSServices.0.NSSendFileTypes.0"), "public.folder");
        assert_eq!(
            extract("NSServices.1.NSMenuItem.default"),
            "New Lokyr Tab Here"
        );
        assert_eq!(extract("NSServices.1.NSMessage"), "openTab");
        let context = Command::new("plutil")
            .args([
                "-extract",
                "NSServices.1.NSRequiredContext",
                "json",
                "-o",
                "-",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&context.stdout).trim(), "{}");
        assert_eq!(
            extract("UTImportedTypeDeclarations.0.UTTypeIdentifier"),
            ITERMCOLORS
        );
        assert_eq!(
            extract(
                "UTImportedTypeDeclarations.0.UTTypeTagSpecification.public\\.filename-extension.0"
            ),
            "itermcolors"
        );
        assert_eq!(
            extract("UTImportedTypeDeclarations.0.UTTypeDescription"),
            extract("CFBundleDocumentTypes.0.CFBundleTypeName")
        );
        fs::remove_file(path).unwrap();
    }
}
