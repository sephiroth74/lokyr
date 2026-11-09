//! The plugins that the user asked for, `plugins.toml`, in the config's
//! folder: each by its name, of a version's constraint of the registry, of
//! a folder, or of a git repository at a tag or a commit; and the
//! registries by their names. Sleip resolves it into the lockfile.

use std::collections::BTreeMap;
use std::path::Path;

use crate::FileError;
use crate::manifest::{is_name, is_part, is_requirement, line_and_column};

/// The file's name, in the config's folder.
pub const FILE: &str = "plugins.toml";

/// The registry that Sleip reads unless the file names another as
/// `default`.
pub const REGISTRY: &str = "https://pkg.lokyr.dev";

/// The plugins that the user asked for, and the registries.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct List {
    /// The plugins, by their names, `owner/name`.
    pub plugins: BTreeMap<String, Wanted>,
    /// The registries' addresses, by their names, `default` among them if
    /// the file names it.
    pub registries: BTreeMap<String, String>,
}

/// Where a plugin comes from, as the user asked for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Wanted {
    /// The registry's, of a version's constraint, as `^0.1`.
    Registry(String),
    /// A folder's, of its path, absolute or from `~`.
    Path(String),
    /// A git repository's, of its address, at a tag or a commit.
    Git {
        /// Its address: `https://`, `ssh://`, `git@host:path` or `file://`.
        url: String,
        /// The tag or the commit.
        at: GitAt,
    },
}

/// Where in a git repository a plugin is taken.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitAt {
    /// At a tag.
    Tag(String),
    /// At a commit, by its hash, of 7 hexadecimal digits at least.
    Rev(String),
}

impl List {
    /// The list of `text`, or why it does not read, and where.
    pub fn parse(text: &str) -> Result<Self, FileError> {
        let at = |span: std::ops::Range<usize>| Some(line_and_column(text, span.start));
        let fail = |span: std::ops::Range<usize>, message: String| FileError {
            at: at(span),
            message,
        };
        let file: File = toml::from_str(text).map_err(|error| crate::unread(text, &error))?;
        let mut list = Self::default();
        for (name, wanted) in file.plugins {
            if !is_name(name.get_ref()) {
                return Err(fail(
                    name.span(),
                    format!(
                        "`{}` is not a plugin's name, `owner/name`, of lowercase letters, digits and `-`",
                        name.get_ref()
                    ),
                ));
            }
            let span = wanted.span();
            let wanted =
                self::wanted(wanted.into_inner()).map_err(|message| fail(span, message))?;
            list.plugins.insert(name.into_inner(), wanted);
        }
        for (name, url) in file.registries {
            if !is_part(name.get_ref()) {
                return Err(fail(
                    name.span(),
                    format!(
                        "`{}` is not a registry's name, of lowercase letters, digits and `-`",
                        name.get_ref()
                    ),
                ));
            }
            if !is_registry(url.get_ref()) {
                return Err(fail(
                    url.span(),
                    format!(
                        "`{}` is not a registry's address, `https://` or `file://`",
                        url.get_ref()
                    ),
                ));
            }
            list.registries.insert(name.into_inner(), url.into_inner());
        }
        Ok(list)
    }

    /// The list of the file at `path`: an empty one of a file that is not
    /// there; or why it does not read, the file's path before its place.
    pub fn read(path: &Path) -> Result<Self, FileError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => {
                return Err(FileError {
                    at: None,
                    message: format!("{} cannot be read: {error}", path.display()),
                });
            }
        };
        Self::parse(&text).map_err(|error| FileError {
            message: format!("{}: {}", path.display(), error.message),
            ..error
        })
    }

    /// The address of the registry `name`: the file's, or the default
    /// registry's of `default`.
    pub fn registry(&self, name: &str) -> Option<&str> {
        self.registries
            .get(name)
            .map(String::as_str)
            .or((name == "default").then_some(REGISTRY))
    }

    /// The list as the file writes it, which [`List::parse`] reads back.
    pub fn text(&self) -> String {
        let quoted = |text: &str| toml::Value::String(text.to_owned()).to_string();
        let mut text = String::from("[plugins]\n");
        for (name, wanted) in &self.plugins {
            let value = match wanted {
                Wanted::Registry(constraint) => quoted(constraint),
                Wanted::Path(path) => format!("{{ path = {} }}", quoted(path)),
                Wanted::Git { url, at } => {
                    let (key, value) = match at {
                        GitAt::Tag(tag) => ("tag", tag),
                        GitAt::Rev(rev) => ("rev", rev),
                    };
                    format!("{{ git = {}, {key} = {} }}", quoted(url), quoted(value))
                }
            };
            text.push_str(&format!("{} = {value}\n", quoted(name)));
        }
        if !self.registries.is_empty() {
            text.push_str("\n[registries]\n");
            for (name, url) in &self.registries {
                text.push_str(&format!("{name} = {}\n", quoted(url)));
            }
        }
        text
    }

    /// Writes the list to `path`, whole.
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        crate::write_whole(path, &self.text())
    }
}

/// The file as serde reads it.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    plugins: BTreeMap<toml::Spanned<String>, toml::Spanned<toml::Value>>,
    #[serde(default)]
    registries: BTreeMap<toml::Spanned<String>, toml::Spanned<String>>,
}

/// Where a plugin comes from of `value`, a version's constraint or a table
/// of a folder or a git repository; or why not.
fn wanted(value: toml::Value) -> Result<Wanted, String> {
    let table = match value {
        toml::Value::String(constraint) if is_requirement(&constraint) => {
            return Ok(Wanted::Registry(constraint));
        }
        toml::Value::String(constraint) => {
            return Err(format!(
                "`{constraint}` is not a version's constraint, as `^0.1`"
            ));
        }
        toml::Value::Table(table) => table,
        other => {
            return Err(format!(
                "a plugin is a version's constraint, as `\"^0.1\"`, or a table of its `path` or its `git`, not a {}",
                other.type_str()
            ));
        }
    };
    let text = |key: &str| match table.get(key) {
        None => Ok(None),
        Some(toml::Value::String(text)) if !text.is_empty() => Ok(Some(text.clone())),
        Some(_) => Err(format!("`{key}` is a text, not empty")),
    };
    if let Some(key) = table
        .keys()
        .find(|key| !["path", "git", "tag", "rev"].contains(&key.as_str()))
    {
        return Err(format!(
            "`{key}` is not a key of a plugin: `path`, or `git` and `tag` or `rev`"
        ));
    }
    match (text("path")?, text("git")?, text("tag")?, text("rev")?) {
        (Some(path), None, None, None) => {
            if path.starts_with('/') || path.starts_with("~/") || path == "~" {
                Ok(Wanted::Path(path))
            } else {
                Err(format!(
                    "`{path}` is not a folder's path, absolute or from `~`"
                ))
            }
        }
        (None, Some(url), tag, rev) => {
            if !is_git(&url) {
                return Err(format!(
                    "`{url}` is not a git repository's address, `https://`, `ssh://`, `git@host:path` or `file://`"
                ));
            }
            let at = match (tag, rev) {
                (Some(tag), None) => GitAt::Tag(tag),
                (None, Some(rev)) if is_rev(&rev) => GitAt::Rev(rev),
                (None, Some(rev)) => {
                    return Err(format!(
                        "`{rev}` is not a commit, of 7 to 40 hexadecimal digits"
                    ));
                }
                _ => {
                    return Err(
                        "a git repository's plugin is at a `tag` or a `rev`, one".to_owned()
                    );
                }
            };
            Ok(Wanted::Git { url, at })
        }
        _ => Err("a plugin is of a `path`, or of a `git` repository, one".to_owned()),
    }
}

/// Whether `url` is a git repository's address that Sleip reads: of a
/// transport that authenticates its server, or a folder's.
fn is_git(url: &str) -> bool {
    let rest = ["https://", "ssh://", "file://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme));
    match rest {
        Some(rest) => !rest.is_empty() && !rest.contains(char::is_whitespace),
        None => url
            .strip_prefix("git@")
            .and_then(|rest| rest.split_once(':'))
            .is_some_and(|(host, path)| !host.is_empty() && !path.is_empty()),
    }
}

/// Whether `rev` is a commit's hash, whole or of its first 7 digits or more.
fn is_rev(rev: &str) -> bool {
    (7..=40).contains(&rev.len()) && rev.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Whether `url` is a registry's address: `https://`, or `file://` of a
/// folder's, as a mirror of the tests.
fn is_registry(url: &str) -> bool {
    ["https://", "file://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
        .is_some_and(|rest| !rest.is_empty() && !rest.contains(char::is_whitespace))
}

#[cfg(test)]
mod tests;
