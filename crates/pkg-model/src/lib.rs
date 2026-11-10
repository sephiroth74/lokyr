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

/// Whether `character` hides or moves the text around it, which a text that
/// a person reads, as a consent's question or a terminal, would show
/// otherwise than it is: a format character of Unicode (`Cf`), the changes
/// of direction and the characters of no width among them, but the
/// joiners, which scripts and emoji take, and the marks of direction,
/// which override no strong character and Arabic and Hebrew texts take;
/// and the separators of lines and paragraphs.
pub fn is_hidden(character: char) -> bool {
    if matches!(
        character,
        '\u{200C}' | '\u{200D}' | '\u{200E}' | '\u{200F}' | '\u{061C}'
    ) {
        return false;
    }
    matches!(
        u32::from(character),
        0x00AD
            | 0x0600..=0x0605
            | 0x06DD
            | 0x070F
            | 0x0890..=0x0891
            | 0x08E2
            | 0x180E
            | 0x200B..=0x200F
            | 0x2028..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFEFF
            | 0xFFF9..=0xFFFB
            | 0x110BD
            | 0x110CD
            | 0x13430..=0x1343F
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0001
            | 0xE0020..=0xE007F
    )
}

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
