//! What Lokyr keeps between its runs that is not the user's config: the
//! directory of the last window that closed, which the first window of the
//! next run starts in when the config says so.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The file of the last window's directory, in the state's directory.
const LAST_DIRECTORY: &str = "last-directory";

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
    fs::create_dir_all(state)?;
    let temporary = state.join(format!(".{LAST_DIRECTORY}.{}.new", std::process::id()));
    let written = fs::write(&temporary, bytes(directory))
        .and_then(|()| fs::rename(&temporary, state.join(LAST_DIRECTORY)));
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
