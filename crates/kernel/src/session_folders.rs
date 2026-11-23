//! The folders that the sessions' switcher lists of the config's `[sessions]
//! folders`: each a path from `/` or `~`, a folder of its own, or of a last
//! part `*`, the folders in it, of one level, but those whose names start
//! with a dot, in their names' order; and, beside them, the recent ones,
//! each once and none that a session was opened in, as their real paths
//! tell, which a link or a shell's own path to a folder names otherwise.

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

pub use lokyr_plugin_sdk::host::without_sessions;

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

    #[cfg(unix)]
    #[test]
    fn a_folder_of_a_session_or_listed_already_goes_whatever_its_path() {
        let root = std::env::temp_dir().join(format!("lokyr-switcher-real-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for folder in ["src/api", "src/web", "notes"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        // A link to `src`, as `/tmp` is to `/private/tmp`.
        std::os::unix::fs::symlink(root.join("src"), root.join("link")).unwrap();
        let path = |folder: &str| root.join(folder).display().to_string();
        let (configured, recent) = without_sessions(
            vec![path("src/api"), path("src/web"), path("link/api")],
            vec![path("link/web"), path("notes"), path("notes"), path("gone")],
            &[&path("link/web")],
        );
        // `web` is the session's, by either path; `api` is listed once, and
        // a folder that is not there still lists of its name.
        assert_eq!(configured, [path("src/api")]);
        assert_eq!(recent, [path("notes"), path("gone")]);
        let _ = fs::remove_dir_all(&root);
    }
}
