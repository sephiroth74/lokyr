//! The folders that the sessions' switcher lists of the config's `[sessions]
//! folders`: each a path from `/` or `~`, a folder of its own, or of a last
//! part `*`, the folders in it, of one level, but those whose names start
//! with a dot, in their names' order.

use std::path::{Path, PathBuf};

use crate::config::expanded;

/// The most folders that the switcher lists of the config's.
pub const MOST_FOLDERS: usize = 500;

/// The folders of `patterns`, of `home` for `~`, each once, in the
/// patterns' order, [`MOST_FOLDERS`] at most: a path that is no folder,
/// or of no `/` nor `~` at its start, lists nothing.
pub fn listed(patterns: &[String], home: Option<&Path>) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    for pattern in patterns {
        if !(pattern.starts_with('/') || pattern == "~" || pattern.starts_with("~/")) {
            continue;
        }
        let (base, every) = match pattern.strip_suffix("/*") {
            Some(base) => (if base.is_empty() { "/" } else { base }, true),
            None => (pattern.as_str(), false),
        };
        let Some(base) = expanded(base, home) else {
            continue;
        };
        let found: Vec<PathBuf> = if every {
            let Ok(entries) = std::fs::read_dir(&base) else {
                continue;
            };
            let mut found: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect();
            found.sort();
            found
        } else if base.is_dir() {
            vec![base]
        } else {
            Vec::new()
        };
        for folder in found {
            if folders.len() == MOST_FOLDERS {
                return folders;
            }
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    folders
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn a_pattern_lists_its_folder_or_the_folders_in_it_of_one_level() {
        let home =
            std::env::temp_dir().join(format!("lokyr-session-folders-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        for folder in [
            "src/api",
            "src/web",
            "src/.hidden",
            "src/web/deep",
            ".config",
        ] {
            fs::create_dir_all(home.join(folder)).unwrap();
        }
        fs::write(home.join("src/notes.txt"), "a file").unwrap();
        let patterns = [
            "~/src/*",
            "~/.config",
            "~/src/api",
            "~/missing/*",
            "~/missing",
            "relative/*",
            "~/src/notes.txt",
        ]
        .map(str::to_owned);
        let listed = listed(&patterns, Some(&home));
        assert_eq!(
            listed,
            [
                home.join("src/api"),
                home.join("src/web"),
                home.join(".config")
            ]
        );
        // Of no home, the patterns of `~` list nothing.
        assert!(super::listed(&patterns, None).is_empty());
        let _ = fs::remove_dir_all(&home);
    }
}
