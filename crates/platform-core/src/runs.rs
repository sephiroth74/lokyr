//! The files that a system runs, installs or follows elsewhere, rather than
//! opens, by what the file itself tells: its mode, its extension, a bundle
//! with a program. A backend adds what only its system knows, as the
//! application that would open a file.

use std::ffi::OsStr;
use std::fs::Metadata;
use std::path::Path;

/// The extensions of the files and the bundles that macOS runs, installs or
/// follows elsewhere, rather than opens: applications, scripts of a
/// terminal, Java archives, Automator's workflows and actions, installers,
/// preference panes, screen savers, configuration profiles, and the
/// Finder's locations, whose URL can be of any scheme.
pub const RUNS: [&str; 20] = [
    "app",
    "command",
    "tool",
    "terminal",
    "jar",
    "workflow",
    "action",
    "pkg",
    "mpkg",
    "prefpane",
    "saver",
    "mobileconfig",
    "fileloc",
    "webloc",
    "inetloc",
    "afploc",
    "ftploc",
    "atloc",
    "mailloc",
    "vncloc",
];

/// Whether the file at `path`, of `metadata`, which a link does not lead
/// from, runs rather than opens by what it tells itself: a program, by its
/// mode, or a file of one of [`RUNS`]' extensions; a directory of one of
/// those extensions, or a bundle with a program.
pub fn runs_by_itself(path: &Path, metadata: &Metadata) -> bool {
    let extension = path
        .extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| RUNS.iter().any(|runs| runs.eq_ignore_ascii_case(extension)));
    if metadata.is_dir() {
        return extension || path.join("Contents/MacOS").is_dir();
    }
    extension || executable(metadata)
}

/// Whether a file of `metadata` is a program, by its mode.
#[cfg(unix)]
fn executable(metadata: &Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

/// Whether a file of `metadata` is a program: on a system without modes,
/// its extension tells.
#[cfg(not(unix))]
fn executable(_: &Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn a_file_runs_by_its_mode_its_extension_or_its_bundle() {
        let directory = std::env::temp_dir().join(format!("lokyr-runs-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join("Tool.bundle/Contents/MacOS")).unwrap();
        fs::create_dir_all(directory.join("Some.App")).unwrap();
        fs::create_dir_all(directory.join("notes")).unwrap();
        for file in [
            "a.txt",
            "b.command",
            "c.JAR",
            "d.webloc",
            "e.ftploc",
            "program",
            "f.sh",
        ] {
            fs::write(directory.join(file), "").unwrap();
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory.join("program"), fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let runs = |name: &str| {
            let path = directory.join(name);
            runs_by_itself(&path, &fs::metadata(&path).unwrap())
        };
        for name in [
            "Tool.bundle",
            "Some.App",
            "b.command",
            "c.JAR",
            "d.webloc",
            "e.ftploc",
        ] {
            assert!(runs(name), "{name}");
        }
        assert_eq!(runs("program"), cfg!(unix));
        for name in ["notes", "a.txt", "f.sh"] {
            assert!(!runs(name), "{name}");
        }
        fs::remove_dir_all(&directory).unwrap();
    }
}
