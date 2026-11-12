//! Package formats shared by the app, Sleip and the registry.
//!
//! Covers plugin manifests (`plugin.toml`), the user's plugin list
//! (`plugins.toml`), the lockfile (`plugins.lock`), the store of the
//! installed packages, the packages' archives, the registry index format
//! and the distribution files that list the built-in plugins; and the
//! packages' service, Sleip's commands of `--json`, which the app runs.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

pub mod address;
#[cfg(feature = "archive")]
pub mod archive;
pub mod list;
pub mod lock;
pub mod manifest;
pub mod permissions;
pub mod schema;
pub mod service;
pub mod store;

/// Why a file of the formats does not read: where, and what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileError {
    /// The line and the column, both from 1, if the error has a place.
    pub at: Option<(usize, usize)>,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.at {
            Some((line, column)) => write!(f, "{line}:{column}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for FileError {}

/// The capability of `text`, as its `Display` writes it: its name, of a
/// parameter after a `:`, as `net:api.github.com`, which its kind takes, as
/// [`parameter_problem`] checks it.
pub fn capability_of(text: &str) -> Option<lokyr_plugin_sdk::command::Capability> {
    use lokyr_plugin_sdk::command::Capability;
    match text.split_once(':') {
        Some((name, parameter)) if parameter_problem(name, parameter).is_none() => {
            Capability::of_parameter(name, parameter.to_owned())
        }
        Some(_) => None,
        None => Capability::of_name(text),
    }
}

pub use lokyr_plugin_sdk::text::is_hidden;

/// Why `parameter` is not one of the capability `kind`, `fs-read`,
/// `fs-write`, `env-read` or `net`, as the consent's question shows it as
/// it is: a path absolute or from `~`, of no `..`; a variable's name of
/// letters, digits, `_` and `*`; a host's name of letters, digits, `.`,
/// `-` and `*`; each of no control nor hidden character and bounded; none
/// of a parameter that fits.
pub fn parameter_problem(kind: &str, parameter: &str) -> Option<String> {
    let count = parameter.chars().count();
    let plain = !parameter
        .chars()
        .any(|character| character.is_control() || is_hidden(character));
    let path = (parameter.starts_with('/') || parameter == "~" || parameter.starts_with("~/"))
        && count <= 1024
        && !parameter.split('/').any(|part| part == "..");
    let variable = (1..=128).contains(&count)
        && parameter
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '*'));
    let host = (1..=253).contains(&count)
        && parameter.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '*')
        });
    match kind {
        "fs-read" | "fs-write" if !(path && plain) => Some(format!(
            "`{kind}` lists paths, absolute or from `~`, of no `..` nor control or hidden character, of 1024 characters at most"
        )),
        "env-read" if !variable => Some(format!(
            "`{kind}` lists variables' names, of letters, digits, `_` and `*`, of 128 characters at most"
        )),
        "net" if !host => Some(format!(
            "`{kind}` lists hosts' names, of letters, digits, `.`, `-` and `*`, of 253 characters at most"
        )),
        _ => None,
    }
}

/// The error of a file of `text` that `toml` did not read, at its place.
pub(crate) fn unread(text: &str, error: &toml::de::Error) -> FileError {
    FileError {
        at: error
            .span()
            .map(|span| manifest::line_and_column(text, span.start)),
        message: error.message().to_owned(),
    }
}

/// How many links a path can lead through, each to the next.
const LINKS: usize = 32;

/// The file that `path` leads to, through its symbolic links: its target
/// when it is one that exists, and otherwise the last link's target, which
/// a write makes, so that a link stays a link.
pub fn link_target(path: &Path) -> PathBuf {
    if let Ok(target) = std::fs::canonicalize(path) {
        return target;
    }
    let mut path = path.to_owned();
    for _ in 0..LINKS {
        let Ok(target) = std::fs::read_link(&path) else {
            break;
        };
        // A relative target is the link's directory's.
        path = match path.parent() {
            Some(directory) => directory.join(target),
            None => target,
        };
    }
    path
}

/// Writes `text` to the file that `path` leads to, through its links, as
/// [`link_target`] finds it, so that a link of dotfiles stays a link,
/// whole: to a file beside it, of its permissions, flushed to the disk,
/// which takes its place at once, so that a reader reads the old text or
/// the new one, never a part, whatever stops the write; its folder made if
/// it is not there; a failure leaves no file of its own behind.
pub fn write_whole(path: &Path, text: &str) -> io::Result<()> {
    write_whole_bytes(path, text.as_bytes())
}

/// Writes `bytes` to the file that `path` leads to, whole, as
/// [`write_whole`] writes a text.
pub fn write_whole_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_whole_marked(path, bytes, |_| Ok(()))
}

/// The temporary files that this process made, which tell one another
/// apart.
static TEMPORARIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Writes `bytes` to the file that `path` leads to, whole, as
/// [`write_whole`] writes a text, `mark` applied to the new file before it
/// takes the old one's place, as an extended attribute that it keeps: of
/// its error, nothing is written.
///
/// The new file is a temporary one beside it, of a short name of its own,
/// whatever the file's name's length, which no other file nor link has: it
/// is made anew, and a link planted at its name fails it rather than leads
/// the bytes elsewhere.
pub fn write_whole_marked(
    path: &Path,
    bytes: &[u8],
    mark: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    /// The names tried before the write gives up, each taken already.
    const TRIES: u32 = 16;
    let path = link_target(path);
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty());
    if path.file_name().is_none() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "a file's path"));
    }
    if let Some(folder) = folder {
        std::fs::create_dir_all(folder)?;
    }
    let mut made = None;
    for _ in 0..TRIES {
        let name = format!(
            ".lokyr-{}-{}.tmp",
            std::process::id(),
            TEMPORARIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let temporary = folder.map_or_else(|| name.clone().into(), |folder| folder.join(&name));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => {
                made = Some((file, temporary));
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    let Some((mut file, temporary)) = made else {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no temporary file's name is free beside it",
        ));
    };
    let written = (|| {
        file.write_all(bytes)?;
        if let Ok(metadata) = std::fs::metadata(&path) {
            file.set_permissions(metadata.permissions())?;
        }
        mark(&temporary)?;
        file.sync_all()?;
        std::fs::rename(&temporary, &path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    written?;
    // The rename itself on the disk, of the folder's entry.
    if let Some(folder) = folder
        && let Ok(folder) = std::fs::File::open(folder)
    {
        let _ = folder.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    /// A folder of its own for the test `name`.
    fn folder(name: &str) -> std::path::PathBuf {
        let folder =
            std::env::temp_dir().join(format!("lokyr-whole-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn a_file_of_the_longest_name_is_written_whole_of_a_temporary_file_of_a_short_one() {
        let folder = folder("long");
        let path = folder.join("x".repeat(255));
        write_whole_bytes(&path, b"one").unwrap();
        write_whole_bytes(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert_eq!(
            std::fs::read_dir(&folder).unwrap().count(),
            1,
            "no temporary file left"
        );
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn the_mark_is_the_new_file_s_before_it_takes_the_old_one_s_place() {
        let folder = folder("mark");
        let path = folder.join("shot.png");
        std::fs::write(&path, b"old").unwrap();
        let mut marked = None;
        write_whole_marked(&path, b"new", |temporary| {
            assert_eq!(std::fs::read(&path).unwrap(), b"old", "not in place yet");
            marked = Some(std::fs::metadata(temporary)?.ino());
            Ok(())
        })
        .unwrap();
        assert_eq!(marked, Some(std::fs::metadata(&path).unwrap().ino()));
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        // A mark that fails writes nothing, and leaves no temporary file.
        let failed = write_whole_marked(&path, b"newer", |_| {
            Err(io::Error::new(io::ErrorKind::Unsupported, "no attributes"))
        });
        assert_eq!(failed.unwrap_err().kind(), io::ErrorKind::Unsupported);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(folder).unwrap();
    }

    #[test]
    fn a_link_planted_at_a_temporary_name_is_never_followed() {
        let folder = folder("planted");
        let elsewhere = folder.join("elsewhere");
        std::fs::write(&elsewhere, b"kept").unwrap();
        // Links at every name that the next writes would take, whatever
        // the other tests write meanwhile.
        let next = TEMPORARIES.load(std::sync::atomic::Ordering::Relaxed);
        for made in next..next + 4096 {
            let name = format!(".lokyr-{}-{made}.tmp", std::process::id());
            std::os::unix::fs::symlink(&elsewhere, folder.join(name)).unwrap();
        }
        let path = folder.join("shot.png");
        assert!(write_whole_bytes(&path, b"bytes").is_err());
        assert_eq!(std::fs::read(&elsewhere).unwrap(), b"kept");
        assert!(!path.exists());
        std::fs::remove_dir_all(folder).unwrap();
    }
}
