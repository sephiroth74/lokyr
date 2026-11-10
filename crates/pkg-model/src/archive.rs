//! A plugin's package as an archive, a tar compressed by zstd, `.tar.zst`,
//! which Sleip and the registry's tools pack, and Sleip checks as it
//! unpacks it into the store: regular files alone, no links, no absolute
//! path and no `..`, no two paths that differ by their case alone, as the
//! Mac's file system takes them for one; 20 MB compressed and 100 MB
//! extracted at most, of a ratio between them that a bomb of zstd passes;
//! its files of the layout of its kind, and their permissions those of the
//! store, whatever the archive says.
//!
//! The layout: `plugin.toml`, the manifest; `src/`, Luau's sources of a
//! plugin of Luau, of its entry `src/init.luau`, never bytecode;
//! `themes/`, the themes of a package of themes; `schema.json`, or the
//! file that the manifest's `config-schema` names; `locales/<language>/`,
//! the catalogs of Project Fluent, `.ftl`; `assets/`, the registry's
//! icons and screenshots; and a `README`, a `LICENSE` and a `CHANGELOG`.

use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::fmt;
use std::io::{self, Read};
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::manifest::{Kind, Manifest};
use crate::store::{Digest, Store, digest_of};

/// The most bytes of an archive, compressed.
pub const MOST_COMPRESSED: u64 = 20 * 1024 * 1024;

/// The most bytes of an archive, extracted, of its tar.
pub const MOST_EXTRACTED: u64 = 100 * 1024 * 1024;

/// The most times its bytes compressed that an archive's extracted bytes
/// are, past [`RATIO_FROM`] of them: text compresses some tenfold.
pub const MOST_RATIO: u64 = 100;

/// The extracted bytes past which [`MOST_RATIO`] holds, so that a small
/// package of a few repeated bytes passes.
const RATIO_FROM: u64 = 1024 * 1024;

/// The most files of a package.
pub const MOST_FILES: usize = 10_000;

/// The largest window of zstd that an archive's frames take, past which
/// the decoder would hold that much memory: the reference compressor's
/// highest level takes 8 MiB.
const MOST_WINDOW: u64 = 8 * 1024 * 1024;

/// The manifest's path in a package.
const MANIFEST: &str = "plugin.toml";

/// The bounds of an unpack: the archive's, and its extraction's.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bounds {
    pub(crate) compressed: u64,
    pub(crate) extracted: u64,
    pub(crate) ratio: u64,
    pub(crate) files: usize,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            compressed: MOST_COMPRESSED,
            extracted: MOST_EXTRACTED,
            ratio: MOST_RATIO,
            files: MOST_FILES,
        }
    }
}

/// The entry of a plugin of Luau.
const ENTRY: &str = "src/init.luau";

/// Why an archive is refused, or could not be read or written.
#[derive(Debug)]
pub enum ArchiveError {
    /// It breaks a rule of the archives, as the message tells.
    Refused(String),
    /// The disk failed.
    Io(io::Error),
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(message) => f.write_str(message),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ArchiveError {}

impl From<io::Error> for ArchiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A refusal of `message`.
fn refused(message: impl Into<String>) -> ArchiveError {
    ArchiveError::Refused(message.into())
}

/// A package unpacked: its manifest, and its digest, of its files.
#[derive(Clone, Debug, PartialEq)]
pub struct Unpacked {
    /// Its manifest.
    pub manifest: Manifest,
    /// Its digest, which names its folder in the store.
    pub digest: Digest,
}

/// A package added to the store: as it unpacked, and whether the store
/// held none of its digest before, so that taking it away leaves the store
/// as it was.
#[derive(Clone, Debug, PartialEq)]
pub struct Stored {
    /// The package, as it unpacked.
    pub unpacked: Unpacked,
    /// Whether the store held no package of its digest before, sound or
    /// not.
    pub new: bool,
}

/// The archive of the package in `folder`: the files of its layout, in
/// their paths' order, each of the permissions and the time of none, so
/// that a package packs alike wherever and whenever it is packed; the
/// others, as a folder's `.git` or its tests, left out. Refused of a
/// manifest that does not read, a file of its layout that is a link, a
/// layout that its kind lacks, as a plugin of Luau without its entry, or
/// past the sizes.
pub fn pack(folder: &Path) -> Result<Vec<u8>, ArchiveError> {
    let manifest = read_manifest(&std::fs::read_to_string(folder.join(MANIFEST)).map_err(
        |error| {
            refused(format!(
                "{MANIFEST} cannot be read in {}: {error}",
                folder.display()
            ))
        },
    )?)?;
    let mut files = Vec::new();
    let mut left = vec![folder.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at)? {
            let entry = entry?;
            let path = entry.path();
            let name = relative(folder, &path)?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                // A folder of the tools, as `.git` or `.vscode`, is none
                // of the package's.
                if !entry.file_name().to_string_lossy().starts_with('.') {
                    left.push(path);
                }
                continue;
            }
            if place(&name, &manifest).is_err() {
                continue;
            }
            if !kind.is_file() {
                return Err(refused(format!(
                    "`{name}` is a link, or another file that is not a regular one"
                )));
            }
            files.push(name);
        }
    }
    files.sort();
    laid_out(&files, &manifest)?;
    let mut builder = tar::Builder::new(Vec::new());
    let mut extracted = 0;
    for name in &files {
        let file = std::fs::File::open(folder.join(name))?;
        let size = file.metadata()?.len();
        extracted += size;
        if extracted > MOST_EXTRACTED {
            return Err(refused(format!(
                "the package holds more than {} MB",
                MOST_EXTRACTED / (1024 * 1024)
            )));
        }
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Regular);
        header.set_size(size);
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        builder.append_data(&mut header, name, file.take(size))?;
    }
    let tar = builder.into_inner()?;
    let compressed =
        ruzstd::encoding::compress_to_vec(&tar[..], ruzstd::encoding::CompressionLevel::Fastest);
    if compressed.len() as u64 > MOST_COMPRESSED {
        return Err(refused(format!(
            "the archive is more than {} MB",
            MOST_COMPRESSED / (1024 * 1024)
        )));
    }
    unpacks(&compressed)?;
    Ok(compressed)
}

/// Checks that `archive`, which [`pack`] made, unpacks within the bounds
/// that every unpacking holds, of its files, its size with its headers and
/// its ratio, so that no package packs that no store takes: it unpacks it
/// into a folder of its own, then takes the folder away.
fn unpacks(archive: &[u8]) -> Result<(), ArchiveError> {
    static PACKS: AtomicUsize = AtomicUsize::new(0);
    let folder = std::env::temp_dir().join(format!(
        "lokyr-pack-{}-{}",
        std::process::id(),
        PACKS.fetch_add(1, Ordering::Relaxed)
    ));
    let unpacked = unpack(archive, &folder);
    let _ = std::fs::remove_dir_all(&folder);
    unpacked.map(|_| ())
}

/// Unpacks the archive `archive` into the folder `into`, which it makes:
/// the package, as the rules hold it, its files of the store's
/// permissions; refused of the first rule that it breaks, `into` taken
/// away.
pub fn unpack(archive: impl Read, into: &Path) -> Result<Unpacked, ArchiveError> {
    unpack_within(archive, into, Bounds::default())
}

/// Unpacks `archive` into `into`, as [`unpack`] does, within `bounds`.
pub(crate) fn unpack_within(
    archive: impl Read,
    into: &Path,
    bounds: Bounds,
) -> Result<Unpacked, ArchiveError> {
    if let Some(parent) = into.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::create_dir(into)?;
    let unpacked = unpack_into(archive, into, bounds);
    if unpacked.is_err() {
        let _ = std::fs::remove_dir_all(into);
    }
    unpacked
}

/// Unpacks `archive` into `into`, which is there, as [`unpack`] does,
/// within `bounds`.
fn unpack_into(archive: impl Read, into: &Path, bounds: Bounds) -> Result<Unpacked, ArchiveError> {
    let refusal = Rc::new(RefCell::new(None));
    let read = Rc::new(Cell::new(0));
    let compressed = Counted {
        inner: archive,
        count: Rc::clone(&read),
        most: move |count: u64, _: u64| {
            (count > bounds.compressed)
                .then(|| format!("the archive is more than {}", size(bounds.compressed)))
        },
        refusal: Rc::clone(&refusal),
        other: Rc::new(Cell::new(0)),
    };
    let decoder =
        ruzstd::decoding::StreamingDecoder::new_with_max_window_size(compressed, MOST_WINDOW)
            .map_err(|error| {
                refusal.borrow_mut().take().map_or_else(
                    || refused(format!("the archive is not of zstd: {error}")),
                    refused,
                )
            })?;
    let extracted = Counted {
        inner: decoder,
        count: Rc::new(Cell::new(0)),
        most: move |count: u64, compressed: u64| {
            if count > bounds.extracted {
                Some(format!(
                    "the archive holds more than {}",
                    size(bounds.extracted)
                ))
            } else if count > RATIO_FROM && count > compressed.saturating_mul(bounds.ratio) {
                Some(format!(
                    "the archive extracts to more than {} times its size, as a bomb would",
                    bounds.ratio
                ))
            } else {
                None
            }
        },
        refusal: Rc::clone(&refusal),
        other: read,
    };
    let mut tar = tar::Archive::new(extracted);
    let entries = (|| -> Result<Vec<String>, ArchiveError> {
        let mut files = Vec::new();
        let mut seen = Seen::default();
        for entry in tar.entries()? {
            let mut entry = entry?;
            let bytes = entry.path_bytes();
            let path = std::str::from_utf8(&bytes)
                .map_err(|_| refused("a path of the archive is not UTF-8"))?
                .to_owned();
            let kind = entry.header().entry_type();
            // The package's own folder, as tools write it.
            if kind.is_dir() && (path == "./" || path == ".") {
                continue;
            }
            let path = normal(&path)?;
            if kind.is_dir() {
                seen.folder(&path)?;
                continue;
            }
            if !kind.is_file() {
                return Err(refused(format!(
                    "`{path}` is a link, or another entry that is not a file nor a folder"
                )));
            }
            seen.file(&path)?;
            if files.len() == bounds.files {
                return Err(refused(format!(
                    "the archive holds more than {} files",
                    bounds.files
                )));
            }
            write_file(&mut entry, &into.join(&path))?;
            files.push(path);
        }
        Ok(files)
    })();
    let files = entries.map_err(|error| match refusal.borrow_mut().take() {
        Some(message) => refused(message),
        None => error,
    })?;
    let text = std::fs::read_to_string(into.join(MANIFEST))
        .map_err(|_| refused(format!("the archive has no {MANIFEST}")))?;
    let manifest = read_manifest(&text)?;
    let mut files = files;
    files.sort();
    laid_out(&files, &manifest)?;
    let digest = digest_of(into)?;
    Ok(Unpacked { manifest, digest })
}

impl Store {
    /// Unpacks the archive `archive` into the store, as [`unpack`] does,
    /// beside its packages, its files and folders flushed to the disk, then
    /// under its digest at once, so that the store holds a package whole or
    /// not at all; one that it held already stays as it was, unless its
    /// files are no longer those of its digest, as of a file that the
    /// Finder or a power cut left, which the new one takes the place of.
    /// The store is made if it is not there. Whether it is new tells what
    /// a failure that follows may take away.
    pub fn add(&self, archive: impl Read) -> Result<Stored, ArchiveError> {
        static UNPACKS: AtomicUsize = AtomicUsize::new(0);
        std::fs::create_dir_all(self.root())?;
        let aside = |what: &str| {
            self.root().join(format!(
                ".{what}-{}-{}",
                std::process::id(),
                UNPACKS.fetch_add(1, Ordering::Relaxed)
            ))
        };
        let temporary = aside("unpack");
        let unpacked = unpack(archive, &temporary)?;
        let placed = (|| {
            sync_folders(&temporary)?;
            let folder = self.folder(&unpacked.digest);
            let held = folder.is_dir();
            if held {
                if self.check(&unpacked.digest).is_ok() {
                    std::fs::remove_dir_all(&temporary)?;
                    return Ok(false);
                }
                // Changed since it was stored: set aside, then gone.
                let stale = aside("stale");
                std::fs::rename(&folder, &stale)?;
                let _ = std::fs::remove_dir_all(&stale);
            }
            // Of another rename that placed the same package first, this
            // one finds it there, and it is not new of this one.
            let renamed = std::fs::rename(&temporary, &folder).is_ok();
            if !renamed && !folder.is_dir() {
                return Err(io::Error::other("its folder could not be renamed"));
            }
            let _ = std::fs::remove_dir_all(&temporary);
            sync_folder(self.root());
            Ok(renamed && !held)
        })();
        match placed {
            Ok(new) => Ok(Stored { unpacked, new }),
            Err(error) => {
                let _ = std::fs::remove_dir_all(&temporary);
                Err(refused(format!(
                    "the package {} could not take its place in the store: {error}",
                    unpacked.digest
                )))
            }
        }
    }
}

/// Flushes the files of `folder`, and its folders, their entries, to the
/// disk: a few milliseconds a file on an Apple disk, whose flush empties
/// the drive's cache too.
fn sync_folders(folder: &Path) -> io::Result<()> {
    let mut left = vec![folder.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                left.push(entry.path());
            } else {
                std::fs::File::open(entry.path())?.sync_all()?;
            }
        }
        std::fs::File::open(&at)?.sync_all()?;
    }
    Ok(())
}

/// Flushes the entries of `folder` to the disk, as far as it can.
fn sync_folder(folder: &Path) {
    if let Ok(folder) = std::fs::File::open(folder) {
        let _ = folder.sync_all();
    }
}

/// `bytes`, in megabytes, or in bytes below one.
fn size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MB", bytes / (1024 * 1024))
    } else {
        format!("{bytes} bytes")
    }
}

/// The manifest of `text`, a package's; refused of one that does not read,
/// or of a built-in plugin's, which no package holds.
fn read_manifest(text: &str) -> Result<Manifest, ArchiveError> {
    let manifest = Manifest::parse(text).map_err(|error| refused(format!("{MANIFEST}:{error}")))?;
    if manifest.kind == Kind::Builtin {
        return Err(refused(
            "a built-in plugin is linked in the app, never packed",
        ));
    }
    Ok(manifest)
}

/// Writes the file of `entry`'s bytes to `path`, of the store's
/// permissions, its folders made, of theirs.
fn write_file(entry: &mut impl Read, path: &Path) -> Result<(), ArchiveError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    io::copy(entry, &mut file)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

/// The path of `path` in `folder`, of `/` between its folders.
fn relative(folder: &Path, path: &Path) -> Result<String, ArchiveError> {
    let inner = path.strip_prefix(folder).map_err(io::Error::other)?;
    let parts: Option<Vec<&str>> = inner.iter().map(|part| part.to_str()).collect();
    parts
        .map(|parts| parts.join("/"))
        .ok_or_else(|| refused(format!("{} is not of a UTF-8 name", path.display())))
}

/// `path`, an archive's entry's, of no `./` before it nor `/` after a
/// folder's; refused of an absolute path, a `..`, an empty part, a `.`
/// within it, or a backslash, which another system takes for a folder's
/// end.
fn normal(path: &str) -> Result<String, ArchiveError> {
    let trimmed = path.strip_prefix("./").unwrap_or(path);
    let trimmed = trimmed.strip_suffix('/').unwrap_or(trimmed);
    if trimmed.is_empty()
        || trimmed.starts_with('/')
        || trimmed.contains('\\')
        || trimmed
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(refused(format!(
            "`{path}` is not a path within the package: absolute, of `..`, or of an empty part"
        )));
    }
    Ok(trimmed.to_owned())
}

/// The paths of an archive met so far, of their case alone, as the Mac's
/// file system takes them: those of files, and those of their folders.
#[derive(Default)]
struct Seen {
    files: BTreeSet<String>,
    folders: BTreeSet<String>,
}

impl Seen {
    /// Takes the file `path`: refused of a path met before, or that a file
    /// met before holds, or that holds one, of their case alone.
    fn file(&mut self, path: &str) -> Result<(), ArchiveError> {
        let lower = path.to_lowercase();
        if self.files.contains(&lower) || self.folders.contains(&lower) {
            return Err(refused(format!(
                "`{path}` is twice in the archive, of its case alone or as a folder's too"
            )));
        }
        self.folder_of(&lower, path)?;
        self.files.insert(lower);
        Ok(())
    }

    /// Takes the folder `path`, as a file's within it would.
    fn folder(&mut self, path: &str) -> Result<(), ArchiveError> {
        let lower = path.to_lowercase();
        self.folder_of(&format!("{lower}/"), path)
    }

    /// Takes the folders of `lower`, `path`'s of its case alone.
    fn folder_of(&mut self, lower: &str, path: &str) -> Result<(), ArchiveError> {
        for (at, _) in lower.match_indices('/') {
            let folder = &lower[..at];
            if self.files.contains(folder) {
                return Err(refused(format!("`{path}` is within a file of the archive")));
            }
            self.folders.insert(folder.to_owned());
        }
        Ok(())
    }
}

/// Where `path`, a package's file's, is of its layout, of `manifest`'s
/// kind and config's schema; or why it has none.
fn place(path: &str, manifest: &Manifest) -> Result<(), String> {
    let (first, rest) = path.split_once('/').unwrap_or((path, ""));
    let schema = manifest.contributes.config_schema.as_deref();
    let fits = match (first, rest.is_empty()) {
        (MANIFEST | "schema.json", true) => true,
        (name, true)
            if ["README", "LICENSE", "CHANGELOG"]
                .iter()
                .any(|start| name.starts_with(start)) =>
        {
            true
        }
        ("src", false) => manifest.kind == Kind::Luau && rest.ends_with(".luau"),
        ("themes", false) => {
            manifest.kind == Kind::Themes
                && (rest.ends_with(".toml") || rest.ends_with(".itermcolors"))
        }
        ("assets", false) => true,
        ("locales", false) => rest.split_once('/').is_some_and(|(language, file)| {
            !language.is_empty()
                && language
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !file.contains('/')
                && file.len() > ".ftl".len()
                && file.ends_with(".ftl")
        }),
        _ => false,
    };
    if fits || schema == Some(path) {
        Ok(())
    } else {
        Err(format!(
            "`{path}` has no place in a package of the kind `{}`: `plugin.toml`, `src/` of `.luau` of a plugin of Luau, `themes/` of a package of themes, `schema.json`, `locales/<language>/*.ftl`, `assets/`, a `README`, a `LICENSE` or a `CHANGELOG`",
            kind_name(manifest.kind)
        ))
    }
}

/// The manifest's name of `kind`.
fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Luau => "luau",
        Kind::Themes => "themes",
        Kind::Builtin => "builtin",
    }
}

/// Checks that `files`, a package's, are of its layout: each of its place,
/// a plugin of Luau of its entry, a package of themes of a theme, and the
/// config's schema that the manifest names among them.
fn laid_out(files: &[String], manifest: &Manifest) -> Result<(), ArchiveError> {
    for file in files {
        place(file, manifest).map_err(refused)?;
    }
    let holds = |path: &str| files.iter().any(|file| file == path);
    match manifest.kind {
        Kind::Luau if !holds(ENTRY) => {
            return Err(refused(format!(
                "a plugin of Luau holds its entry, `{ENTRY}`"
            )));
        }
        Kind::Themes if !files.iter().any(|file| file.starts_with("themes/")) => {
            return Err(refused(
                "a package of themes holds its themes, in `themes/`",
            ));
        }
        _ => {}
    }
    if let Some(schema) = &manifest.contributes.config_schema
        && !holds(schema)
    {
        return Err(refused(format!(
            "the manifest's `config-schema`, `{schema}`, is not in the package"
        )));
    }
    Ok(())
}

/// A reader that counts its bytes and stops, of `most`'s refusal, as they
/// pass a bound, which may read the count of `other`.
struct Counted<R, F> {
    inner: R,
    count: Rc<Cell<u64>>,
    most: F,
    refusal: Rc<RefCell<Option<String>>>,
    other: Rc<Cell<u64>>,
}

impl<R: Read, F: Fn(u64, u64) -> Option<String>> Read for Counted<R, F> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        let total = self.count.get() + count as u64;
        self.count.set(total);
        if let Some(message) = (self.most)(total, self.other.get()) {
            *self.refusal.borrow_mut() = Some(message.clone());
            return Err(io::Error::new(io::ErrorKind::InvalidData, message));
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests;
