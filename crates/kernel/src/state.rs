//! What Lokyr keeps between its runs that is not the user's config: the
//! directory of the last window that closed, which the first window of the
//! next run starts in when the config says so, the windows that were open
//! as it quit, which it opens again, and their panes' scrollback, which
//! they show again.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub mod scrollback;
pub mod windows;

/// The file of the last window's directory, in the state's directory.
const LAST_DIRECTORY: &str = "last-directory";

/// The file of the recent folders, which the sessions' switcher lists, in
/// the state's directory: a line each, the latest first.
const RECENT_FOLDERS: &str = "recent-folders";

/// The directory of the app's state: `~/Library/Application Support/lokyr/
/// state/` on macOS; `$XDG_STATE_HOME/lokyr/`, or `~/.local/state/lokyr/`,
/// elsewhere. `None` without a home directory.
pub fn directory() -> Option<PathBuf> {
    let home = std::env::home_dir();
    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library/Application Support/lokyr/state"));
    }
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|state| state.is_absolute())
        .or_else(|| home.map(|home| home.join(".local/state")))
        .map(|state| state.join("lokyr"))
}

/// The directory that the last window that closed was in, as `state` keeps
/// it; `None` when it keeps none.
pub fn last_directory(state: &Path) -> Option<PathBuf> {
    let bytes = fs::read(state.join(LAST_DIRECTORY)).ok()?;
    (!bytes.is_empty()).then(|| PathBuf::from(os_string(bytes)))
}

/// Keeps `directory` as the last window's, in `state`, which it makes: a
/// file written whole, so that a write cut short leaves the one before.
pub fn keep_last_directory(state: &Path, directory: &Path) -> io::Result<()> {
    write_whole(&state.join(LAST_DIRECTORY), &bytes(directory))
}

/// The recent folders that `state` keeps, the latest first; none of no
/// file.
pub fn recent_folders(state: &Path) -> Vec<String> {
    fs::read_to_string(state.join(RECENT_FOLDERS))
        .map(|text| {
            text.lines()
                .filter(|line| line.starts_with('/'))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Keeps `folders`, the recent ones, in `state`, which it makes, a line
/// each, those of a line break left out: a file written whole.
pub fn keep_recent_folders(state: &Path, folders: &[String]) -> io::Result<()> {
    let mut text = String::new();
    for folder in folders.iter().filter(|folder| !folder.contains('\n')) {
        text.push_str(folder);
        text.push('\n');
    }
    write_whole(&state.join(RECENT_FOLDERS), text.as_bytes())
}

/// Writes `contents` to the file at `path`, whose directory it makes,
/// whole: into a file of its own beside it, renamed over it, so that a
/// write cut short leaves the one before, and no file of its own.
pub fn write_whole(path: &Path, contents: &[u8]) -> io::Result<()> {
    let directory = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(directory)?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = directory.join(format!(".{name}.{}.new", std::process::id()));
    let written = fs::write(&temporary, contents).and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

/// The bytes of `path`, as the system names it, which any byte but NUL
/// can be in, a newline too.
fn bytes(path: &Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().into_owned().into_bytes()
    }
}

/// The path of `bytes`, as [`bytes`] gives them.
fn os_string(bytes: Vec<u8>) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(bytes)
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(&bytes).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_directory_reads_back_as_it_was_kept() {
        let state = std::env::temp_dir().join(format!("lokyr-state-{}", std::process::id()));
        let _ = fs::remove_dir_all(&state);
        assert_eq!(last_directory(&state), None);
        // Made with its directory, then in place of the one before.
        keep_last_directory(&state, Path::new("/tmp/a b")).unwrap();
        assert_eq!(last_directory(&state), Some(PathBuf::from("/tmp/a b")));
        let odd = Path::new("/tmp/line\nbreak");
        keep_last_directory(&state, odd).unwrap();
        assert_eq!(last_directory(&state).as_deref(), Some(odd));
        // Nothing of the write is left past it.
        let names: Vec<_> = fs::read_dir(&state)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(names, [LAST_DIRECTORY]);
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn the_recent_folders_read_back_as_they_were_kept() {
        let state = std::env::temp_dir().join(format!("lokyr-state-recent-{}", std::process::id()));
        let _ = fs::remove_dir_all(&state);
        assert!(recent_folders(&state).is_empty());
        let folders = [
            "/tmp/a b".to_owned(),
            "/tmp/line\nbreak".to_owned(),
            "/src".to_owned(),
        ];
        keep_recent_folders(&state, &folders).unwrap();
        assert_eq!(recent_folders(&state), ["/tmp/a b", "/src"]);
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn the_state_is_in_the_system_s_place() {
        let Some(state) = directory() else {
            return;
        };
        if cfg!(target_os = "macos") {
            assert!(state.ends_with("Library/Application Support/lokyr/state"));
        } else {
            assert!(state.ends_with("lokyr"));
        }
    }
}
