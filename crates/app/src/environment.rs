//! What the app gives its programs of their environment, past what the
//! PTY sets: the bundle's resources, and the locale that `LANG` takes when
//! the app starts without one, as from the Dock, from the user's languages
//! and regions.

use std::fs;
use std::path::{Path, PathBuf};

/// Where macOS keeps its locales, a directory each.
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

/// The UTF-8 locales that the system has, by their names, in order.
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
        assert!(
            available.iter().any(|name| name == "en_US.UTF-8"),
            "{available:?}"
        );
        assert!(available.iter().all(|name| name.ends_with(UTF8)));
    }
}
