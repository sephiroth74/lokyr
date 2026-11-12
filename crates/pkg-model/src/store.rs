//! The store of the installed packages: a folder each, which Sleip unpacks
//! and never changes, named by its digest, the SHA-256 of its files, so
//! that the app, which reads the store, can check a package as it loads it:
//! `~/Library/Application Support/lokyr/store/<digest>/` on macOS.
//!
//! A package's digest is of its files alone, in the byte order of their
//! paths, of `/` between their folders: each its path, a zero byte, its
//! size as eight bytes, most significant first, and its bytes. Its folders,
//! their order and its files' permissions and times count for nothing, so
//! that a package unpacked anew, or packed again, keeps its digest.

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use sha2::{Digest as _, Sha256};

/// The prefix of a digest's text.
const PREFIX: &str = "sha256:";

/// A SHA-256, as `sha256:` and its 64 hexadecimal digits write it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    /// Its 64 hexadecimal digits, of lowercase letters, which name its
    /// package's folder in the store.
    pub fn hex(&self) -> String {
        self.0.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// The digest of the 64 hexadecimal digits `hex`, of lowercase letters.
    pub fn of_hex(hex: &str) -> Option<Self> {
        if hex.len() != 64
            || !hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        let mut bytes = [0; 32];
        for (at, byte) in bytes.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&hex[2 * at..2 * at + 2], 16).ok()?;
        }
        Some(Self(bytes))
    }

    /// The SHA-256 of `bytes`.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{}", self.hex())
    }
}

impl FromStr for Digest {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        text.strip_prefix(PREFIX)
            .and_then(Self::of_hex)
            .ok_or_else(|| format!("`{text}` is not `sha256:` and 64 hexadecimal digits"))
    }
}

/// The digest of the package in `folder`, of its files; an error of a
/// folder that holds a link, or another file that is not a regular one,
/// as no package holds, or a name that is not UTF-8.
pub fn digest_of(folder: &Path) -> io::Result<Digest> {
    let mut files = Vec::new();
    let mut left = vec![folder.to_path_buf()];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                left.push(path);
            } else if kind.is_file() {
                files.push(relative(folder, &path)?);
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{} is not a regular file nor a folder", path.display()),
                ));
            }
        }
    }
    files.sort();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    for name in files {
        let mut file = std::fs::File::open(folder.join(&name))?;
        let size = file.metadata()?.len();
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_be_bytes());
        let mut read = 0;
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            read += count as u64;
            hasher.update(&buffer[..count]);
        }
        if read != size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{name} changed as it was read"),
            ));
        }
    }
    Ok(Digest(hasher.finalize().into()))
}

/// The path of `path` in `folder`, of `/` between its folders.
fn relative(folder: &Path, path: &Path) -> io::Result<String> {
    let inner = path.strip_prefix(folder).map_err(io::Error::other)?;
    let parts: Option<Vec<&str>> = inner.iter().map(|part| part.to_str()).collect();
    parts.map(|parts| parts.join("/")).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{} is not of a UTF-8 name", path.display()),
        )
    })
}

/// The store of the installed packages, at its folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The user's store, of the home folder `home`: in `Library/Application
    /// Support/lokyr/store` on macOS, in the data folder of the XDG's base
    /// directories elsewhere, `data` if it is given, as
    /// `$XDG_DATA_HOME`, or `.local/share` in `home`.
    pub fn of_user(home: &Path, data: Option<&Path>) -> Self {
        let base = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else {
            data.filter(|data| data.is_absolute())
                .map_or_else(|| home.join(".local/share"), Path::to_path_buf)
        };
        Self::new(base.join("lokyr/store"))
    }

    /// Its folder.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder of the package of `digest`, whether it holds it or not.
    pub fn folder(&self, digest: &Digest) -> PathBuf {
        self.root.join(digest.hex())
    }

    /// Whether it holds the package of `digest`.
    pub fn holds(&self, digest: &Digest) -> bool {
        self.folder(digest).is_dir()
    }

    /// The digests of the packages that it holds, in their order; none of
    /// a store that is not there. The folders of other names, as those of
    /// unpacks under way, are not packages.
    pub fn digests(&self) -> io::Result<Vec<Digest>> {
        let entries = match std::fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut digests = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && let Some(digest) = entry.file_name().to_str().and_then(Digest::of_hex)
            {
                digests.push(digest);
            }
        }
        digests.sort();
        Ok(digests)
    }

    /// Checks the package of `digest`, as the app does as it loads it:
    /// that the store holds it, and that its files' digest is its own.
    pub fn check(&self, digest: &Digest) -> io::Result<()> {
        let found = digest_of(&self.folder(digest))?;
        if found == *digest {
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "the package {digest} of the store holds {found}, changed since it was stored"
                ),
            ))
        }
    }

    /// Removes the package of `digest`, as no lockfile names it.
    pub fn remove(&self, digest: &Digest) -> io::Result<()> {
        std::fs::remove_dir_all(self.folder(digest))
    }
}

#[cfg(test)]
mod tests;
