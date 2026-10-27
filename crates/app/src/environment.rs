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
    let executable = std::env::current_exe().ok()?;
    let binaries = executable.parent()?;
    if binaries.file_name() != Some("MacOS".as_ref()) {
        return None;
    }
    let resources = binaries.parent()?.join("Resources");
    resources.is_dir().then_some(resources)
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

/// The UTF-8 locale for the user's `locales`, languages with regions in the
/// order of their preferences, among the system's `available` ones: the
/// first that the system has; otherwise a language's own region, as
/// `it_IT` for `it_CH`, or the first region of the language that the
/// system has, as `ja_JP` for `ja_CH`; otherwise `en_US.UTF-8`, which
/// English takes too, as its regions come in no order of use.
pub(crate) fn utf8_locale(locales: &[String], available: &[String]) -> String {
    let has = |name: &str| available.iter().any(|available| available == name);
    if let Some(locale) = locales
        .iter()
        .map(|locale| format!("{locale}{UTF8}"))
        .find(|name| has(name))
    {
        return locale;
    }
    for language in locales.iter().filter_map(|locale| locale.split_once('_')) {
        let language = language.0;
        if language == "en" {
            continue;
        }
        let own = format!("{language}_{}{UTF8}", language.to_uppercase());
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
    fn the_system_has_utf8_locales() {
        let available = system_locales();
        assert!(
            available.iter().any(|name| name == "en_US.UTF-8"),
            "{available:?}"
        );
        assert!(available.iter().all(|name| name.ends_with(UTF8)));
    }
}
