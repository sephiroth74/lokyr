//! Package formats shared by the app, Sleip and the registry.
//!
//! Covers plugin manifests (`plugin.toml`), the user's plugin list
//! (`plugins.toml`), the lockfile (`plugins.lock`), the store of the
//! installed packages, the packages' archives, the registry index format
//! and the distribution files that list the built-in plugins.

#![forbid(unsafe_code)]

use std::fmt;
use std::io::{self, Write as _};
use std::path::Path;

#[cfg(feature = "archive")]
pub mod archive;
pub mod list;
pub mod lock;
pub mod manifest;
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

/// The error of a file of `text` that `toml` did not read, at its place.
pub(crate) fn unread(text: &str, error: &toml::de::Error) -> FileError {
    FileError {
        at: error
            .span()
            .map(|span| manifest::line_and_column(text, span.start)),
        message: error.message().to_owned(),
    }
}

/// Writes `text` to `path` whole: to a file beside it, flushed to the disk,
/// which takes `path`'s place at once, so that a reader of `path` reads the
/// old text or the new one, never a part, whatever stops the write.
pub fn write_whole(path: &Path, text: &str) -> io::Result<()> {
    let folder = path
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty());
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "a file's path"))?;
    let mut temporary = name.to_owned();
    temporary.push(format!(".{}.tmp", std::process::id()));
    let temporary = folder.map_or_else(
        || temporary.clone().into(),
        |folder| folder.join(&temporary),
    );
    let written = (|| {
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)
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
