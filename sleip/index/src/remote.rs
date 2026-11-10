//! A registry as Sleip reads it: its `config.json`; its `keys.json`,
//! checked against the root keys; the list of its plugins and a plugin's
//! index file, each checked against the index keys. Each file is kept in
//! Sleip's cache once it is checked, of its ETag, so that the next request
//! asks of it as not modified and a later file older than it is refused;
//! offline, of the cache alone, which is checked as it is read.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::client::{Client, Fetched};
use crate::signing::{self, Index, Keys, List, PublicKey, Timestamp};

/// A registry's `config.json`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Config {
    /// Its version.
    pub v: u32,
    /// The address of a version's archive, of `{owner}`, `{name}` and
    /// `{version}`.
    pub dl: String,
}

/// A registry, at its address, of Sleip's cache.
pub struct Remote<'a> {
    url: String,
    cache: PathBuf,
    client: Option<&'a Client>,
    roots: Vec<PublicKey>,
}

impl<'a> Remote<'a> {
    /// The registry at `url`, of the cache of Sleip in `cache`, its
    /// requests of `client`, or none offline, its keys checked against the
    /// root keys `roots`.
    pub fn new(url: &str, cache: &Path, client: Option<&'a Client>, roots: Vec<PublicKey>) -> Self {
        let url = url.trim_end_matches('/').to_owned();
        let cache = cache.join("index").join(folder_of(&url));
        Self {
            url,
            cache,
            client,
            roots,
        }
    }

    /// Its address.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The file `path` of the registry, checked of `check`, which takes
    /// the time of the one held, if any, and tells the time of its own of
    /// `time`: kept in the cache once checked; `None` of no such file.
    fn checked<T>(
        &self,
        path: &str,
        check: impl Fn(&str, Option<Timestamp>) -> Result<T, String>,
        time: impl Fn(&T) -> Timestamp,
    ) -> Result<Option<T>, String> {
        let file = self.cache.join(path);
        let etag_file = self.cache.join(format!("{path}.etag"));
        let cached = std::fs::read_to_string(&file).ok();
        let Some(client) = self.client else {
            return match cached {
                Some(text) => check(&text, None).map(Some),
                None => Err(format!(
                    "the cache holds no `{path}` of {}: a run online fetches it",
                    self.url
                )),
            };
        };
        // The time of the one held, as it checks still.
        let held = cached
            .as_deref()
            .and_then(|text| check(text, None).ok())
            .map(|value| time(&value));
        let etag = cached
            .as_ref()
            .and_then(|_| std::fs::read_to_string(&etag_file).ok());
        match client.get(&format!("{}/{path}", self.url), etag.as_deref())? {
            Fetched::Body { bytes, etag } => {
                let text = String::from_utf8(bytes)
                    .map_err(|_| format!("`{path}` of {} is not of UTF-8", self.url))?;
                let value = check(&text, held)?;
                let written =
                    lokyr_pkg_model::write_whole(&file, &text).and_then(|()| match &etag {
                        Some(etag) => lokyr_pkg_model::write_whole(&etag_file, etag),
                        None => match std::fs::remove_file(&etag_file) {
                            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                                Err(error)
                            }
                            _ => Ok(()),
                        },
                    });
                written
                    .map_err(|error| format!("{} cannot be written: {error}", file.display()))?;
                Ok(Some(value))
            }
            Fetched::NotModified => match cached {
                Some(text) => check(&text, None).map(Some),
                None => Err(format!(
                    "{} tells `{path}` as not modified, which the cache does not hold",
                    self.url
                )),
            },
            Fetched::Missing => Ok(None),
        }
    }

    /// Its `config.json`.
    pub fn config(&self) -> Result<Config, String> {
        let config = self.checked(
            "config.json",
            |text, _| {
                let config: Config = serde_json::from_str(text)
                    .map_err(|error| format!("config.json does not read: {error}"))?;
                if config.v == signing::VERSION {
                    Ok(config)
                } else {
                    Err(format!(
                        "config.json is of version {}, which this Sleip does not read: a newer Sleip does",
                        config.v
                    ))
                }
            },
            |_| Timestamp(0),
        )?;
        config.ok_or_else(|| format!("{} holds no config.json", self.url))
    }

    /// Its index keys, of its `keys.json`, which a root key signed.
    pub fn keys(&self) -> Result<Keys, String> {
        let keys = self.checked(
            "keys.json",
            |text, held| signing::verify_keys(text, &self.roots, held),
            |keys| keys.generated,
        )?;
        keys.ok_or_else(|| format!("{} holds no keys.json", self.url))
    }

    /// The list of its plugins, which an index key of `keys` signed.
    pub fn list(&self, keys: &Keys) -> Result<List, String> {
        let list = self.checked(
            "list",
            |text, held| signing::verify_list(text, keys, held),
            |list| list.generated,
        )?;
        list.ok_or_else(|| format!("{} holds no list of its plugins", self.url))
    }

    /// The index file of the plugin `name`, `owner/name`, which an index
    /// key of `keys` signed: `None` of a plugin that it does not publish.
    pub fn index(&self, name: &str, keys: &Keys) -> Result<Option<Index>, String> {
        let Some((owner, plugin)) = name.split_once('/') else {
            return Err(format!("`{name}` is not `owner/name`"));
        };
        if !crate::registry::is_part(owner) || !crate::registry::is_part(plugin) {
            return Err(format!("`{name}` names no plugin of a registry"));
        }
        self.checked(
            &format!("index/{owner}/{plugin}"),
            |text, held| signing::verify_index(text, name, keys, held),
            |index| index.generated,
        )
    }
}

/// The cache's folder of the registry at `url`: its address of no scheme,
/// each character but letters, digits, `.` and `-` a `_`.
fn folder_of(url: &str) -> String {
    let bare = ["https://", "http://", "file://"]
        .iter()
        .find_map(|scheme| url.strip_prefix(scheme))
        .unwrap_or(url);
    bare.chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '.' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registry_s_cache_is_a_folder_of_its_address() {
        assert_eq!(folder_of("https://pkg.lokyr.dev"), "pkg.lokyr.dev");
        assert_eq!(folder_of("http://127.0.0.1:8123"), "127.0.0.1_8123");
        assert_eq!(folder_of("file:///tmp/a b/../x"), "_tmp_a_b_.._x");
    }
}
