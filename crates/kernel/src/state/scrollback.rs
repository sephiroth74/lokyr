//! The panes' scrollback kept as the app quits, beside the windows that it
//! keeps, which each pane reads back as it opens again: a file a pane, of
//! the bytes that write its rows again, in a folder of the state's, of
//! their owner alone, as what the panes showed may be what no one else
//! should read.
//!
//! A file starts with a line of its format's version; one of another
//! version, or that does not read, is left aside. A file is removed once
//! read, and every file goes as the app keeps the panes' scrollback anew.

use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The folder of the files, in the state's directory.
pub const FOLDER: &str = "scrollback";

/// The first line of a file, of its format's version, which a later format
/// that this one cannot read changes.
const HEADER: &[u8] = b"lokyr-scrollback 1\n";

/// The most bytes that a file holds, past which it is left aside, as no
/// pane's scrollback of a limit that the config takes reaches them.
pub const MOST_BYTES: u64 = 1 << 30;

/// A pane's scrollback kept: the bytes that write its rows again, and when
/// it was kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    /// The bytes that write its rows again.
    pub rows: Vec<u8>,
    /// When its file was written, as the app quit.
    pub time: SystemTime,
}

/// Why a pane's file is left aside.
#[derive(Debug)]
pub enum Unread {
    /// It is of another format's version, or of no header.
    Version,
    /// It does not read, or is past [`MOST_BYTES`].
    Invalid(String),
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Version => f.write_str("it is of another format's version"),
            Self::Invalid(error) => write!(f, "it does not read: {error}"),
        }
    }
}

/// The folder of the files of `state`.
pub fn folder(state: &Path) -> PathBuf {
    state.join(FOLDER)
}

/// The name of the file of the pane at `index` among the panes kept, in
/// their order.
pub fn name(index: usize) -> String {
    format!("{index}.scrollback")
}

/// Whether `name` is a file's name as [`name`] gives it, of digits and its
/// extension alone, so that a name that the windows' file holds names no
/// file but one of the folder's.
fn is_name(name: &str) -> bool {
    name.strip_suffix(".scrollback")
        .is_some_and(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
}

/// Keeps `kept`, each pane's file's name and its bytes, in `state`'s
/// folder, which it makes of its owner alone, in place of the files that it
/// held: each written whole, of its owner alone.
pub fn write(state: &Path, kept: &[(String, Vec<u8>)]) -> io::Result<()> {
    if let Some((name, _)) = kept.iter().find(|(name, _)| !is_name(name)) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} is no pane's file"),
        ));
    }
    forget(state)?;
    if kept.is_empty() {
        return Ok(());
    }
    let folder = folder(state);
    make_folder(&folder)?;
    for (name, bytes) in kept {
        write_private(&folder.join(name), bytes)?;
    }
    Ok(())
}

/// The pane's scrollback of the file `name` in `state`'s folder, its
/// header taken off, and the file removed, as a pane reads it once; `None`
/// of no such file.
pub fn take(state: &Path, name: &str) -> Result<Option<Kept>, Unread> {
    if !is_name(name) {
        return Err(Unread::Invalid(format!("{name} is no pane's file")));
    }
    let path = folder(state).join(name);
    let read = read_bounded(&path);
    let _ = fs::remove_file(&path);
    let (bytes, time) = match read {
        Ok(read) => read,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Unread::Invalid(error.to_string())),
    };
    match bytes.strip_prefix(HEADER) {
        Some(rows) => Ok(Some(Kept {
            rows: rows.to_vec(),
            time,
        })),
        None => Err(Unread::Version),
    }
}

/// Forgets the panes' scrollback that `state` keeps: every file of its
/// folder, and the folder.
pub fn forget(state: &Path) -> io::Result<()> {
    match fs::remove_dir_all(folder(state)) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The bytes of the file at `path`, up to [`MOST_BYTES`], and the time
/// that it was written.
fn read_bounded(path: &Path) -> io::Result<(Vec<u8>, SystemTime)> {
    let metadata = fs::metadata(path)?;
    let size = metadata.len();
    if size > MOST_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("it holds {size} bytes, past the {MOST_BYTES} that a pane's may"),
        ));
    }
    let time = metadata.modified().unwrap_or_else(|_| SystemTime::now());
    Ok((fs::read(path)?, time))
}

/// Makes the folder at `path`, of its owner alone.
fn make_folder(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

/// Writes the header and `rows` to the file at `path`, of its owner alone,
/// whole: into a file of its own beside it, renamed over it.
fn write_private(path: &Path, rows: &[u8]) -> io::Result<()> {
    let directory = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temporary = directory.join(format!(".{name}.{}.new", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options
        .open(&temporary)
        .and_then(|mut file| {
            file.write_all(HEADER)?;
            file.write_all(rows)
        })
        .and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A state's directory of its own, under the system's temporary one.
    fn state(name: &str) -> PathBuf {
        let state =
            std::env::temp_dir().join(format!("lokyr-scrollback-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&state);
        fs::create_dir_all(&state).unwrap();
        state
    }

    #[test]
    fn a_file_kept_is_read_once_and_of_its_bytes() {
        let state = state("once");
        let kept = vec![(name(0), b"rows".to_vec()), (name(1), b"more".to_vec())];
        write(&state, &kept).unwrap();
        let rows = |kept: Option<Kept>| kept.map(|kept| kept.rows);
        assert_eq!(
            rows(take(&state, &name(1)).unwrap()),
            Some(b"more".to_vec())
        );
        assert_eq!(take(&state, &name(1)).unwrap(), None);
        let kept = take(&state, &name(0)).unwrap().unwrap();
        assert_eq!(kept.rows, b"rows".to_vec());
        // Its time, the file's, as the app quit.
        assert!(kept.time.elapsed().unwrap_or_default().as_secs() < 60);
        let _ = fs::remove_dir_all(&state);
    }

    #[cfg(unix)]
    #[test]
    fn the_folder_and_its_files_are_of_their_owner_alone() {
        use std::os::unix::fs::PermissionsExt;
        let state = state("modes");
        write(&state, &[(name(0), b"rows".to_vec())]).unwrap();
        let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&folder(&state)), 0o700);
        assert_eq!(mode(&folder(&state).join(name(0))), 0o600);
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn keeping_anew_takes_the_files_before_away() {
        let state = state("anew");
        write(
            &state,
            &[(name(0), b"old".to_vec()), (name(1), b"old".to_vec())],
        )
        .unwrap();
        write(&state, &[(name(0), b"new".to_vec())]).unwrap();
        assert_eq!(take(&state, &name(1)).unwrap(), None);
        assert_eq!(
            take(&state, &name(0)).unwrap().map(|kept| kept.rows),
            Some(b"new".to_vec())
        );
        // Nothing to keep: the folder goes.
        write(&state, &[]).unwrap();
        assert!(!folder(&state).exists());
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn a_file_of_another_version_is_left_aside_and_removed() {
        let state = state("version");
        make_folder(&folder(&state)).unwrap();
        fs::write(folder(&state).join(name(0)), b"lokyr-scrollback 9\nrows").unwrap();
        assert!(matches!(take(&state, &name(0)), Err(Unread::Version)));
        assert!(!folder(&state).join(name(0)).exists());
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn a_name_of_another_path_names_no_file() {
        let state = state("names");
        for name in [
            "../windows.json",
            "0",
            ".scrollback",
            "a.scrollback",
            "/0.scrollback",
        ] {
            assert!(take(&state, name).is_err(), "{name}");
            assert!(
                write(&state, &[(name.to_owned(), Vec::new())]).is_err(),
                "{name}"
            );
        }
        assert!(is_name("12.scrollback"));
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn forgetting_takes_every_file_away() {
        let state = state("forget");
        write(&state, &[(name(0), b"rows".to_vec())]).unwrap();
        forget(&state).unwrap();
        assert!(!folder(&state).exists());
        // Nothing to forget.
        forget(&state).unwrap();
        let _ = fs::remove_dir_all(&state);
    }
}
