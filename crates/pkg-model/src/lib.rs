//! Package formats shared by the app, Sleip and the registry.
//!
//! Covers plugin manifests (`plugin.toml`), the user's plugin list
//! (`plugins.toml`), the lockfile (`plugins.lock`), the store of the
//! installed packages, the packages' archives, the registry index format
//! and the distribution files that list the built-in plugins.

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
/// parameter after a `:`, as `net:api.github.com`.
pub fn capability_of(text: &str) -> Option<lokyr_plugin_sdk::command::Capability> {
    use lokyr_plugin_sdk::command::Capability;
    match text.split_once(':') {
        Some((name, parameter))
            if !parameter.is_empty() && !parameter.chars().any(char::is_control) =>
        {
            Capability::of_parameter(name, parameter.to_owned())
        }
        Some(_) => None,
        None => Capability::of_name(text),
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
    let path = link_target(path);
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty());
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a file's path"))?;
    if let Some(folder) = folder {
        std::fs::create_dir_all(folder)?;
    }
    let mut temporary = std::ffi::OsString::from(".");
    temporary.push(name);
    temporary.push(format!(".{}.tmp", std::process::id()));
    let temporary = folder.map_or_else(
        || temporary.clone().into(),
        |folder| folder.join(&temporary),
    );
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        if let Ok(metadata) = std::fs::metadata(&path) {
            file.set_permissions(metadata.permissions())?;
        }
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
