//! The icon's variants: where the app finds them, and the file that shows
//! the one that the config names. Each variant is `lokyr-` and its name,
//! with `.icns`, in the bundle's resources, or in the source tree when the
//! app runs outside its bundle, as `cargo run` starts it.

use std::fs;
use std::path::{Path, PathBuf};

use lokyr_kernel::config::AppConfig;

/// What starts a variant's file's name.
const PREFIX: &str = "lokyr-";

/// What ends a variant's file's name.
const EXTENSION: &str = ".icns";

/// The directory of the icon's variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Icons {
    directory: PathBuf,
    /// Whether it is the bundle's, whose own icon is the default variant.
    bundled: bool,
}

impl Icons {
    /// The variants in `directory`, the bundle's when `bundled`.
    pub(crate) fn new(directory: PathBuf, bundled: bool) -> Self {
        Self { directory, bundled }
    }

    /// The bundle's variants, in `Contents/Resources/icons`, when the app
    /// runs from its bundle; otherwise the source tree's, when the tree is
    /// where the app was built.
    pub(crate) fn find() -> Option<Self> {
        if let Some(directory) = crate::environment::resources().map(|dir| dir.join("icons"))
            && directory.is_dir()
        {
            return Some(Self::new(directory, true));
        }
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/resources/icons");
        directory.is_dir().then(|| Self::new(directory, false))
    }

    /// Whether the directory is the bundle's.
    pub(crate) fn bundled(&self) -> bool {
        self.bundled
    }

    /// The variants' names, in order.
    pub(crate) fn variants(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(|entry| {
                let name = entry.ok()?.file_name().into_string().ok()?;
                let variant = name.strip_prefix(PREFIX)?.strip_suffix(EXTENSION)?;
                (!variant.is_empty()).then(|| variant.to_owned())
            })
            .collect();
        names.sort();
        names
    }

    /// What shows `variant`, one of [`Icons::variants`]: its file, or
    /// `None` for the bundle's own icon, the default variant's.
    pub(crate) fn file(&self, variant: &str) -> Option<PathBuf> {
        (!self.bundled || variant != AppConfig::DEFAULT_ICON)
            .then(|| self.directory.join(format!("{PREFIX}{variant}{EXTENSION}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_variants_are_the_icon_files_by_their_names() {
        let directory = std::env::temp_dir().join(format!("lokyr-icons-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        for file in [
            "lokyr-laguz.icns",
            "lokyr-jormungandr.icns",
            "lokyr-laguz.svg",
        ] {
            fs::write(directory.join(file), "").unwrap();
        }
        // Neither another file nor a name without its variant counts.
        for file in ["other.icns", "lokyr-.icns", "lokyr-raidho.png"] {
            fs::write(directory.join(file), "").unwrap();
        }
        let icons = Icons::new(directory.clone(), true);
        assert_eq!(icons.variants(), ["jormungandr", "laguz"]);
        // The bundle shows its own icon for the default variant.
        assert_eq!(icons.file("jormungandr"), None);
        assert_eq!(
            icons.file("laguz"),
            Some(directory.join("lokyr-laguz.icns"))
        );
        let tree = Icons::new(directory.clone(), false);
        assert_eq!(
            tree.file("jormungandr"),
            Some(directory.join("lokyr-jormungandr.icns"))
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn the_source_tree_has_the_default_variant_which_the_bundle_takes() {
        let icons = Icons::find().expect("the source tree's icons");
        assert!(!icons.bundled());
        let variants = icons.variants();
        assert!(
            variants.iter().any(|name| name == AppConfig::DEFAULT_ICON),
            "{variants:?}"
        );
        // The bundle's own icon is the default variant's.
        let bundle = include_str!("../../../tools/xtask/src/bundle.rs");
        let named = format!(
            "const DEFAULT_ICON: &str = \"{PREFIX}{}\";",
            AppConfig::DEFAULT_ICON
        );
        assert!(bundle.contains(&named), "the bundle has {named}");
    }
}
