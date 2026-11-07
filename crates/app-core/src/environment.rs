//! What the app gives its programs of their environment, past what the
//! PTY sets: the bundle's resources, and the locale that `LANG` takes when
//! the app starts without one, as from the Dock, from the user's languages
//! and regions.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// Where macOS keeps its locales, a directory each.
#[cfg(target_os = "macos")]
const LOCALES: &str = "/usr/share/locale";

/// The encoding of the locales that the app gives.
const UTF8: &str = ".UTF-8";

/// The locale when the system has none of the user's languages.
const FALLBACK: &str = "en_US.UTF-8";

/// The bundle's resources, `Contents/Resources`, when the app's binary is
/// in its bundle's `Contents/MacOS`.
pub(crate) fn resources() -> Option<PathBuf> {
    resources_of(&std::env::current_exe().ok()?)
}

/// The resources of the bundle of the binary `executable`, by its real
/// path, as a link to it, as a Homebrew cask's in its `bin`, starts it by
/// the link's.
fn resources_of(executable: &Path) -> Option<PathBuf> {
    let executable = fs::canonicalize(executable).ok()?;
    let binaries = executable.parent()?;
    if binaries.file_name() != Some("MacOS".as_ref()) {
        return None;
    }
    let resources = binaries.parent()?.join("Resources");
    resources.is_dir().then_some(resources)
}

/// The directory of the app's binary, the bundle's `Contents/MacOS`, by its
/// real path, when the binary is a bundle's: the programs of its panes find
/// its command line there.
pub(crate) fn binaries() -> Option<PathBuf> {
    binaries_of(&std::env::current_exe().ok()?)
}

/// The directory of `executable`, by its real path, when it is a bundle's.
fn binaries_of(executable: &Path) -> Option<PathBuf> {
    let real = fs::canonicalize(executable).ok()?;
    in_bundle(&real).then(|| real.parent().map(Path::to_path_buf))?
}

/// The bundle's executable that the app's binary is a link to, when the
/// path that started the app is not in a bundle itself, as a package
/// manager's link in its `bin`: macOS takes the main bundle, and the
/// identifier that the notifications need, from that path, so that the app
/// runs as its bundle only from the bundle's own.
pub fn bundle_executable() -> Option<PathBuf> {
    linked_bundle_executable(&std::env::current_exe().ok()?)
}

/// The real path of `executable` when it is a bundle's, and `executable`,
/// the path that started the app, is not.
fn linked_bundle_executable(executable: &Path) -> Option<PathBuf> {
    if in_bundle(executable) {
        return None;
    }
    let real = fs::canonicalize(executable).ok()?;
    in_bundle(&real).then_some(real)
}

/// Whether `executable` is a bundle's: in the `Contents/MacOS` of a
/// directory named `.app`, whose `Contents` has its `Info.plist`.
fn in_bundle(executable: &Path) -> bool {
    let Some(binaries) = executable.parent() else {
        return false;
    };
    let Some(contents) = binaries.parent() else {
        return false;
    };
    binaries.file_name() == Some("MacOS".as_ref())
        && contents.file_name() == Some("Contents".as_ref())
        && contents.parent().and_then(Path::extension) == Some("app".as_ref())
        && contents.join("Info.plist").is_file()
}

/// The directory of the terminal's own terminfo entry, `xterm-lokyr`: the
/// bundle's, in its `resources`, or, outside a bundle, as `cargo run`
/// starts the app, the source tree's, which `cargo xtask terminfo`
/// compiles; `None` where neither has the entry.
pub(crate) fn terminfo(resources: Option<&Path>) -> Option<PathBuf> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/terminfo");
    terminfo_in(
        resources.map(|resources| resources.join("terminfo")),
        source,
    )
}

/// The directory of the shells' integration, its scripts for zsh, bash and
/// fish: the bundle's, in its `resources`, or, outside a bundle, as `cargo
/// run` starts the app, the source tree's; `None` where neither has them.
pub(crate) fn shell_integration(resources: Option<&Path>) -> Option<PathBuf> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../pty/shell-integration");
    resources
        .map(|resources| resources.join("shell-integration"))
        .into_iter()
        .chain([source])
        .find(|directory| directory.join("zsh").join(".zshenv").is_file())
}

/// The first of `bundled` and `source` that holds the entry, by the
/// letter's directory or, as macOS's `tic` writes it, by its hex code's.
fn terminfo_in(bundled: Option<PathBuf>, source: PathBuf) -> Option<PathBuf> {
    let has = |directory: &Path| {
        ["78", "x"]
            .iter()
            .any(|first| directory.join(first).join("xterm-lokyr").is_file())
    };
    bundled
        .into_iter()
        .chain([source])
        .find(|directory| has(directory))
}

/// The directories of the system's terminfo database, where its own
/// library looks past the user's.
#[cfg(target_os = "macos")]
const SYSTEM_TERMINFO: &[&str] = &["/usr/share/terminfo"];

/// The directories of the system's terminfo database, where its own
/// library looks past the user's, as ncurses's builds name them.
#[cfg(not(target_os = "macos"))]
const SYSTEM_TERMINFO: &[&str] = &[
    "/etc/terminfo",
    "/lib/terminfo",
    "/usr/share/terminfo",
    "/usr/lib/terminfo",
];

/// Where the programs that the app starts look for a terminal's terminfo
/// entry, in ncurses's order (<https://invisible-island.net/ncurses/man/terminfo.5.html#h3-Fetching-Compiled-Descriptions>):
/// `.terminfo` in `home`, the directories of `dirs`, as `TERMINFO_DIRS`
/// gives them, an empty one standing for the system's, then the
/// system's. Never `TERMINFO`, which the programs do not inherit.
pub(crate) fn terminfo_dirs(home: Option<&Path>, dirs: Option<&OsStr>) -> Vec<PathBuf> {
    let system = || SYSTEM_TERMINFO.iter().map(PathBuf::from);
    let mut found: Vec<PathBuf> = home
        .map(|home| home.join(".terminfo"))
        .into_iter()
        .collect();
    if let Some(dirs) = dirs {
        for dir in std::env::split_paths(dirs) {
            if dir.as_os_str().is_empty() {
                found.extend(system());
            } else {
                found.push(dir);
            }
        }
    }
    found.extend(system());
    found
}

/// Whether one of `dirs` holds the entry of `term`: in the directory of its
/// first character, or, as macOS's `tic` writes it, of that character's hex
/// code.
pub(crate) fn describes(dirs: &[PathBuf], term: &str) -> bool {
    let Some(first) = term.chars().next() else {
        return false;
    };
    let letter = first.to_string();
    let hex = format!("{:02x}", u32::from(first));
    dirs.iter().any(|dir| {
        [&letter, &hex]
            .iter()
            .any(|first| dir.join(first).join(term).is_file())
    })
}

/// The UTF-8 locales that the system has, by their names, in order: on
/// macOS, the directories of `/usr/share/locale`.
#[cfg(target_os = "macos")]
pub(crate) fn system_locales() -> Vec<String> {
    let Ok(entries) = fs::read_dir(Path::new(LOCALES)) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.ends_with(UTF8))
        .collect();
    names.sort();
    names
}

/// The UTF-8 locales that the system has, by their names, in order: those
/// that `locale -a` lists, as glibc compiles them into an archive rather
/// than a directory of each.
#[cfg(not(target_os = "macos"))]
pub(crate) fn system_locales() -> Vec<String> {
    let Ok(listed) = std::process::Command::new("locale").arg("-a").output() else {
        return Vec::new();
    };
    utf8_names(String::from_utf8_lossy(&listed.stdout).lines())
}

/// The UTF-8 locales of `names`, as `locale -a` writes them, `en_US.utf8`,
/// in the form that the app gives, `en_US.UTF-8`, which glibc takes too,
/// in order; those of a modifier, as `ca_ES.utf8@valencia`, left out.
#[cfg(any(test, not(target_os = "macos")))]
fn utf8_names<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut found: Vec<String> = names
        .filter_map(|name| {
            let (locale, codeset) = name.trim().split_once('.')?;
            matches!(codeset.to_ascii_lowercase().as_str(), "utf8" | "utf-8")
                .then(|| format!("{locale}{UTF8}"))
        })
        .collect();
    found.sort();
    found.dedup();
    found
}

/// The language of `locale`, as `it` of `it_CH`.
fn language_of(locale: &str) -> &str {
    locale
        .split_once('_')
        .map_or(locale, |(language, _)| language)
}

/// The UTF-8 locale for the user's `locales`, languages with regions in the
/// order of their preferences, among the system's `available` ones: of the
/// first language that the system has a locale of, the first of the user's
/// regions of it that the system has, as `en_GB` of `en_CH` and `en_GB`;
/// otherwise the language's own region, as `it_IT` for `it_CH`, and
/// `en_US` for English; otherwise the first region of it that the system
/// has, as `ja_JP` for `ja_CH`. A later language comes only after: `en_CH`
/// and `it_CH` give `en_US`. Without any, `en_US.UTF-8`.
pub(crate) fn utf8_locale(locales: &[String], available: &[String]) -> String {
    let has = |name: &str| available.iter().any(|available| available == name);
    let mut languages: Vec<&str> = Vec::new();
    for locale in locales {
        let language = language_of(locale);
        if !languages.contains(&language) {
            languages.push(language);
        }
    }
    for language in languages {
        if let Some(locale) = locales
            .iter()
            .filter(|locale| language_of(locale) == language)
            .map(|locale| format!("{locale}{UTF8}"))
            .find(|name| has(name))
        {
            return locale;
        }
        let own = if language == "en" {
            FALLBACK.to_owned()
        } else {
            format!("{language}_{}{UTF8}", language.to_uppercase())
        };
        if has(&own) {
            return own;
        }
        let prefix = format!("{language}_");
        if let Some(first) = available.iter().find(|name| name.starts_with(&prefix)) {
            return first.clone();
        }
    }
    FALLBACK.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminfo_entry_is_found_where_programs_look_and_nowhere_else() {
        let root = std::env::temp_dir().join(format!("lokyr-entries-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("home");
        let (letters, hexes) = (root.join("letters"), root.join("hexes"));
        let entry = |dir: &Path, first: &str, term: &str| {
            fs::create_dir_all(dir.join(first)).unwrap();
            fs::write(dir.join(first).join(term), b"entry").unwrap();
        };
        entry(&home.join(".terminfo"), "m", "mine");
        entry(&letters, "x", "xterm-letters");
        entry(&hexes, "78", "xterm-hexes");
        // TERMINFO_DIRS's own, an empty one standing for the system's.
        let joined =
            std::env::join_paths([letters.as_os_str(), OsStr::new(""), hexes.as_os_str()]).unwrap();
        let dirs = terminfo_dirs(Some(&home), Some(&joined));
        assert_eq!(dirs[0], home.join(".terminfo"));
        assert_eq!(dirs[1], letters);
        assert_eq!(dirs[2], PathBuf::from(SYSTEM_TERMINFO[0]));
        assert_eq!(dirs[1 + 1 + SYSTEM_TERMINFO.len()], hexes);
        assert_eq!(
            dirs.last(),
            Some(&PathBuf::from(*SYSTEM_TERMINFO.last().unwrap()))
        );
        for term in ["mine", "xterm-letters", "xterm-hexes"] {
            assert!(describes(&dirs, term), "{term}");
        }
        for term in ["xtermxterm-256color", "", "xterm-letter"] {
            assert!(!describes(&dirs, term), "{term}");
        }
        // Without `TERMINFO_DIRS`, its directories hold nothing.
        let dirs = terminfo_dirs(Some(&home), None);
        assert!(describes(&dirs, "mine"));
        assert!(!describes(&dirs, "xterm-hexes"));
        // A directory, not an entry.
        fs::create_dir_all(letters.join("x").join("xterm-dir")).unwrap();
        assert!(!describes(&[letters], "xterm-dir"));
        fs::remove_dir_all(&root).unwrap();
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|&name| name.to_owned()).collect()
    }

    #[test]
    fn the_first_of_the_user_s_locales_that_the_system_has_is_taken() {
        let available = names(&[
            "de_CH.UTF-8",
            "en_AU.UTF-8",
            "en_US.UTF-8",
            "it_CH.UTF-8",
            "it_IT.UTF-8",
            "ja_JP.UTF-8",
        ]);
        let chosen = |locales: &[&str]| utf8_locale(&names(locales), &available);
        // English in Switzerland, which the system lacks, then American
        // English.
        assert_eq!(chosen(&["en_CH", "en_US", "it_CH"]), "en_US.UTF-8");
        assert_eq!(chosen(&["it_CH", "en_US"]), "it_CH.UTF-8");
        // A language's own region, then its first.
        assert_eq!(chosen(&["it_US"]), "it_IT.UTF-8");
        assert_eq!(chosen(&["ja_CH"]), "ja_JP.UTF-8");
        // English in a region that the system lacks takes American English,
        // not the first region that the system has.
        assert_eq!(chosen(&["en_CH"]), "en_US.UTF-8");
        assert_eq!(chosen(&["xx_YY"]), "en_US.UTF-8");
        assert_eq!(chosen(&[]), "en_US.UTF-8");
    }

    #[test]
    fn the_first_language_wins_over_a_later_one_s_exact_region() {
        let available = names(&[
            "en_GB.UTF-8",
            "en_US.UTF-8",
            "it_CH.UTF-8",
            "it_IT.UTF-8",
            "ja_JP.UTF-8",
        ]);
        let chosen = |locales: &[&str]| utf8_locale(&names(locales), &available);
        // English first, though the system lacks its region and has the
        // Italian one's.
        assert_eq!(chosen(&["en_CH", "it_CH"]), "en_US.UTF-8");
        // Another of the user's regions of the language, before its own.
        assert_eq!(chosen(&["en_CH", "en_GB", "it_CH"]), "en_GB.UTF-8");
        assert_eq!(chosen(&["it_US", "en_GB", "it_CH"]), "it_CH.UTF-8");
        // A language that the system lacks gives way to the next.
        assert_eq!(chosen(&["xx_YY", "it_CH"]), "it_CH.UTF-8");
        assert_eq!(chosen(&["ja_CH", "it_CH"]), "ja_JP.UTF-8");
        // A language without its region.
        assert_eq!(chosen(&["it"]), "it_IT.UTF-8");
    }

    #[test]
    fn the_bundle_s_resources_are_found_through_a_link_to_its_binary() {
        let root = std::env::temp_dir().join(format!("lokyr-bundle-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let contents = root.join("Lokyr.app").join("Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::create_dir_all(contents.join("Resources")).unwrap();
        let binary = contents.join("MacOS").join("lokyr");
        fs::write(&binary, "").unwrap();
        let resources = fs::canonicalize(contents.join("Resources")).unwrap();
        assert_eq!(resources_of(&binary), Some(resources.clone()));
        // As a cask links it from Homebrew's `bin`.
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let link = bin.join("lokyr");
        std::os::unix::fs::symlink(&binary, &link).unwrap();
        assert_eq!(resources_of(&link), Some(resources));
        // A binary outside a bundle has none.
        let loose = bin.join("other");
        fs::write(&loose, "").unwrap();
        assert_eq!(resources_of(&loose), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_bundle_s_binaries_are_its_contents_macos() {
        let root = std::env::temp_dir().join(format!("lokyr-binaries-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let contents = root.join("Lokyr.app").join("Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::write(contents.join("Info.plist"), "").unwrap();
        let binary = contents.join("MacOS").join("lokyr");
        fs::write(&binary, "").unwrap();
        let macos = fs::canonicalize(contents.join("MacOS")).unwrap();
        assert_eq!(binaries_of(&binary), Some(macos.clone()));
        // Through a link, as a package manager's `bin` has, the bundle's.
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        std::os::unix::fs::symlink(&binary, bin.join("lokyr")).unwrap();
        assert_eq!(binaries_of(&bin.join("lokyr")), Some(macos));
        // A binary outside a bundle, as `cargo run` starts it, has none.
        fs::write(bin.join("other"), "").unwrap();
        assert_eq!(binaries_of(&bin.join("other")), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_link_to_the_bundle_s_binary_gives_the_binary_s_own_path() {
        let root = std::env::temp_dir().join(format!("lokyr-linked-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let contents = root.join("Lokyr.app").join("Contents");
        fs::create_dir_all(contents.join("MacOS")).unwrap();
        fs::write(contents.join("Info.plist"), "").unwrap();
        let binary = contents.join("MacOS").join("lokyr");
        fs::write(&binary, "").unwrap();
        // Started from the bundle, it stays.
        assert_eq!(linked_bundle_executable(&binary), None);
        // Through a link from a package manager's `bin`, the bundle's.
        let bin = root.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let link = bin.join("lokyr");
        std::os::unix::fs::symlink(&binary, &link).unwrap();
        let real = fs::canonicalize(&binary).unwrap();
        assert_eq!(linked_bundle_executable(&link), Some(real));
        // Through a link to a directory above the bundle, the path is the
        // bundle's already.
        let apps = root.join("apps");
        std::os::unix::fs::symlink(&root, &apps).unwrap();
        let through = apps.join("Lokyr.app/Contents/MacOS/lokyr");
        assert_eq!(linked_bundle_executable(&through), None);
        // A binary outside a bundle, and a link to one, have none.
        let loose = bin.join("other");
        fs::write(&loose, "").unwrap();
        std::os::unix::fs::symlink(&loose, bin.join("other-link")).unwrap();
        assert_eq!(linked_bundle_executable(&loose), None);
        assert_eq!(linked_bundle_executable(&bin.join("other-link")), None);
        // Nor does a link into what is no bundle: a binary of a directory
        // without its `Info.plist`, not named `.app`, or out of the
        // bundle's `Contents/MacOS`.
        let layouts = [
            ("Bare.app/Contents/MacOS", None),
            ("Lokyr.bundle/Contents/MacOS", Some("Lokyr.bundle/Contents")),
            (
                "Resources.app/Contents/Resources",
                Some("Resources.app/Contents"),
            ),
            ("Other.app/Other/MacOS", Some("Other.app/Other")),
        ];
        for (binaries, plist) in layouts {
            let binaries = root.join(binaries);
            fs::create_dir_all(&binaries).unwrap();
            if let Some(plist) = plist {
                fs::write(root.join(plist).join("Info.plist"), "").unwrap();
            }
            fs::write(binaries.join("Lokyr"), "").unwrap();
            let link = bin.join(binaries.to_string_lossy().replace('/', "-"));
            std::os::unix::fs::symlink(binaries.join("Lokyr"), &link).unwrap();
            assert_eq!(
                linked_bundle_executable(&link),
                None,
                "{}",
                binaries.display()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_terminfo_entry_is_the_bundle_s_or_the_source_tree_s() {
        let root = std::env::temp_dir().join(format!("lokyr-terminfo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (bundled, source) = (root.join("bundle"), root.join("source"));
        let write = |directory: &Path, first: &str| {
            fs::create_dir_all(directory.join(first)).unwrap();
            fs::write(directory.join(first).join("xterm-lokyr"), "").unwrap();
        };
        assert_eq!(terminfo_in(Some(bundled.clone()), source.clone()), None);
        write(&source, "x");
        assert_eq!(
            terminfo_in(Some(bundled.clone()), source.clone()),
            Some(source.clone())
        );
        write(&bundled, "78");
        assert_eq!(
            terminfo_in(Some(bundled.clone()), source.clone()),
            Some(bundled)
        );
        assert_eq!(terminfo_in(None, source.clone()), Some(source));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_system_has_utf8_locales() {
        let available = system_locales();
        assert!(!available.is_empty());
        assert!(available.iter().all(|name| name.ends_with(UTF8)));
        // macOS has every language's; a Linux system, those that it
        // generated, `C.UTF-8` at least.
        if cfg!(target_os = "macos") {
            assert!(
                available.iter().any(|name| name == "en_US.UTF-8"),
                "{available:?}"
            );
        }
    }

    #[test]
    fn locale_s_list_reads_as_the_app_names_its_locales() {
        let listed = "C\nC.utf8\nPOSIX\nen_US.utf8\nde_DE.UTF-8\nca_ES.utf8@valencia\nja_JP.eucjp\nen_US.utf8\n";
        assert_eq!(
            utf8_names(listed.lines()),
            ["C.UTF-8", "de_DE.UTF-8", "en_US.UTF-8"]
        );
    }
}
