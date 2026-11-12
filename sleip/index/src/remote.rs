//! A registry as Sleip reads it: its `config.json`; its `keys.json`,
//! checked against the root keys; the list of its plugins and a plugin's
//! index file, each checked against the index keys. Each file is kept in
//! Sleip's cache once it is checked, of its ETag, so that the next request
//! asks of it as not modified and a later file older than it is refused;
//! offline, of the cache alone, which is checked as it is read. A file of
//! a time to come is refused; online, an index file or the list older
//! than [`signing::INDEX_LIFETIME`], or a `keys.json` older than
//! [`signing::KEYS_LIFETIME`], is refused as expired, and taken offline
//! with a note; a `keys.json` made before [`signing::KEYS_NOT_BEFORE`] is
//! refused.

use std::cell::RefCell;
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
    /// Whether its files expire, as a person's Sleip reads them.
    expiring: bool,
    /// What it took that is worth telling, as an expired file offline.
    notes: RefCell<Vec<String>>,
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
            expiring: true,
            notes: RefCell::new(Vec::new()),
        }
    }

    /// Of files that never expire nor come of a time to come, as the
    /// registry's own tools read the index deployed, which they keep
    /// however old, whatever its time.
    pub fn never_expiring(mut self) -> Self {
        self.expiring = false;
        self
    }

    /// What it took that is worth telling, since this was last asked.
    pub fn notes(&self) -> Vec<String> {
        std::mem::take(&mut self.notes.borrow_mut())
    }

    /// Its address.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// The file `path` of the registry, checked of `check`, which takes
    /// the time of the one held, if any, and tells the time of its own of
    /// `time`, of `lifetime`, if it expires: kept in the cache once
    /// checked; `None` of no such file.
    fn checked<T>(
        &self,
        path: &str,
        check: impl Fn(&str, Option<Timestamp>) -> Result<T, String>,
        time: impl Fn(&T) -> Timestamp,
        lifetime: Option<u64>,
    ) -> Result<Option<T>, String> {
        // Its time weighed with its signature, before the cache keeps it:
        // a file of a time to come, or expired online, is never kept, and
        // one that the cache holds so, as before this check, is no floor
        // for the next.
        let fresh = |text: &str, held: Option<Timestamp>| {
            let value = check(text, held)?;
            if let Some(lifetime) = lifetime {
                self.fresh(path, time(&value), lifetime)?;
            }
            Ok(value)
        };
        self.fetched(path, &fresh, &time)
    }

    /// Whether the file `path`, of its time `generated`, is of no time to
    /// come and, of a file that expires after `lifetime`, unexpired online;
    /// expired offline, a note.
    fn fresh(&self, path: &str, generated: Timestamp, lifetime: u64) -> Result<(), String> {
        // The registry's own tools keep its index whatever its time.
        if !self.expiring {
            return Ok(());
        }
        let now = Timestamp::now();
        if generated.0 > now.0 + signing::SKEW {
            return Err(format!(
                "`{path}` of {} was made at {generated}, a time to come: the registry's clock or this machine's is wrong",
                self.url
            ));
        }
        let expiry = Timestamp(generated.0 + lifetime);
        if now <= expiry {
            return Ok(());
        }
        if self.client.is_some() {
            return Err(format!(
                "`{path}` of {} expired at {expiry}: the registry signs its index anew every few hours, and one so old is refused, as it could be an old copy that hides a later change",
                self.url
            ));
        }
        self.notes.borrow_mut().push(format!(
            "`{path}` of {}, as the cache holds it, expired at {expiry}: a run online fetches it anew",
            self.url
        ));
        Ok(())
    }

    /// The file `path` of the registry, as [`Remote::checked`] reads it,
    /// of `check`, which weighs its time too: written to the cache once it
    /// checks, never before.
    fn fetched<T>(
        &self,
        path: &str,
        check: &impl Fn(&str, Option<Timestamp>) -> Result<T, String>,
        time: &impl Fn(&T) -> Timestamp,
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
        // The time of the one held, as it checks still, of no time to come.
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
            None,
        )?;
        config.ok_or_else(|| format!("{} holds no config.json", self.url))
    }

    /// Its index keys, of its `keys.json`, which a root key signed.
    pub fn keys(&self) -> Result<Keys, String> {
        let keys = self.checked(
            "keys.json",
            |text, held| {
                let keys = signing::verify_keys(text, &self.roots, held)?;
                if keys.generated < signing::KEYS_NOT_BEFORE {
                    return Err(format!(
                        "keys.json was made at {}, before {}, the time of the one that this Sleip was built with: an older one could list an index key since taken off",
                        keys.generated,
                        signing::KEYS_NOT_BEFORE
                    ));
                }
                Ok(keys)
            },
            |keys| keys.generated,
            Some(signing::KEYS_LIFETIME),
        )?;
        keys.ok_or_else(|| format!("{} holds no keys.json", self.url))
    }

    /// The list of its plugins, which an index key of `keys` signed.
    pub fn list(&self, keys: &Keys) -> Result<List, String> {
        let list = self.checked(
            "list",
            |text, held| signing::verify_list(text, keys, held),
            |list| list.generated,
            Some(signing::INDEX_LIFETIME),
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
            Some(signing::INDEX_LIFETIME),
        )
    }
}

/// The cache's folder of the registry at `url`: its address, its scheme
/// first and `://` a `_`, each character but letters, digits, `.` and `-`
/// a `_`, so that two schemes of a host keep apart and no address makes
/// `.` nor `..`.
fn folder_of(url: &str) -> String {
    let (scheme, bare) = url.split_once("://").unwrap_or(("", url));
    format!("{scheme}_{bare}")
        .chars()
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
        assert_eq!(folder_of("https://pkg.lokyr.dev"), "https_pkg.lokyr.dev");
        assert_eq!(folder_of("http://pkg.lokyr.dev"), "http_pkg.lokyr.dev");
        assert_eq!(folder_of("http://127.0.0.1:8123"), "http_127.0.0.1_8123");
        assert_eq!(folder_of("file:///tmp/a b/../x"), "file__tmp_a_b_.._x");
        assert_eq!(folder_of("file://.."), "file_..");
    }
}
