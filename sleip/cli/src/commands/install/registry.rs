//! The registry's plugins of an install: their versions, of the registry's
//! signed index, which Sleip checks and caches, each a candidate of the
//! resolution, yanked ones left out but one that the lockfile holds; and
//! the archives of the versions picked, downloaded or of Sleip's cache,
//! each checked against the index's checksum before the store takes it.
//! The plugins asked for are resolved afresh; the others gathered, the
//! dependencies of those and the plugins that the lockfile holds but the
//! store lacks, are of the version that the lockfile holds, if it holds
//! one, unless the resolution needs another.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lokyr_pkg_model::list::{List, Wanted};
use lokyr_pkg_model::lock::{Lock, Source};
use lokyr_pkg_model::store::Digest;
use semver::Version;
use serde_json::json;
use sleip_index::client::{Client, Fetched};
use sleip_index::remote::Remote;
use sleip_index::signing::Entry;
use sleip_resolver::Candidate;

use crate::args::Options;
use crate::output::Out;
use crate::paths::Paths;

/// The registry's plugins that an install resolves afresh: their versions
/// as the registry's index publishes them.
pub(super) struct Offered {
    /// The registry's address.
    url: String,
    /// The address of a version's archive, of `{owner}`, `{name}` and
    /// `{version}`.
    dl: String,
    /// The client of the downloads, none offline.
    client: Option<Client>,
    /// Sleip's folder of the archives downloaded.
    archives: PathBuf,
    /// The versions of each plugin, by its name.
    entries: BTreeMap<String, Vec<Entry>>,
    /// The plugins asked for, resolved afresh.
    asked: BTreeSet<String>,
}

impl Offered {
    /// None, of no registry's plugin to resolve afresh, which asks the
    /// network nothing.
    pub(super) fn none() -> Self {
        Self {
            url: String::new(),
            dl: String::new(),
            client: None,
            archives: PathBuf::new(),
            entries: BTreeMap::new(),
            asked: BTreeSet::new(),
        }
    }

    /// The versions of the registry's plugins `asked`, resolved afresh,
    /// and of `also`, of the version that `lock` holds if it can, of the
    /// default registry of `list`, and of the plugins that they depend on
    /// but those that `list` takes of a folder or a git repository: of the
    /// index fetched, checked against the root keys and cached, or of the
    /// cache alone of `--offline`; none of no name. A yanked version's
    /// dependencies are gathered of the one that `lock` holds alone, and a
    /// dependency that the registry does not publish has no version, which
    /// fails the resolution alone if it needs it.
    pub(super) fn gather(
        paths: &Paths,
        options: Options,
        out: Out,
        list: &List,
        lock: Option<&Lock>,
        asked: BTreeSet<String>,
        also: &BTreeSet<String>,
    ) -> Result<Self, String> {
        if asked.is_empty() && also.is_empty() {
            return Ok(Self::none());
        }
        let url = list
            .registry("default")
            .ok_or_else(|| "plugins.toml names no default registry".to_owned())?
            .to_owned();
        let client = (!options.offline).then(Client::new);
        let remote = Remote::new(&url, &paths.cache, client.as_ref(), super::super::roots());
        let keys = remote.keys()?;
        let config = remote.config()?;
        downloads_of(&url, &config.dl)?;
        let mut entries = BTreeMap::new();
        let mut left: Vec<String> = asked.iter().chain(also).cloned().collect();
        while let Some(name) = left.pop() {
            if entries.contains_key(&name) {
                continue;
            }
            let Some(index) = remote.index(&name, &keys)? else {
                if asked.contains(&name) || also.contains(&name) {
                    return Err(format!("{url} publishes no plugin `{name}`"));
                }
                // A dependency that no version needs, unless the
                // resolution says otherwise.
                entries.insert(name, Vec::new());
                continue;
            };
            let locked = lock
                .and_then(|lock| lock.packages.get(&name))
                .map(|locked| locked.version.as_str());
            for entry in &index.entries {
                if entry.yanked && locked != Some(entry.vers.as_str()) {
                    continue;
                }
                for dependency in &entry.deps {
                    // Of a folder, a repository, or a folder that `sleip
                    // dev` links, as it is.
                    let elsewhere = matches!(
                        list.plugins.get(&dependency.name),
                        Some(Wanted::Path(_) | Wanted::Git { .. })
                    ) || lock
                        .and_then(|lock| lock.packages.get(&dependency.name))
                        .is_some_and(|locked| matches!(locked.source, Source::Dev(_)));
                    if !elsewhere && !entries.contains_key(&dependency.name) {
                        left.push(dependency.name.clone());
                    }
                }
            }
            entries.insert(name, index.entries);
        }
        for note in remote.notes() {
            out.tell(
                "note",
                json!({ "message": note }),
                &format!("Note: {note}."),
            );
        }
        Ok(Self {
            url,
            dl: config.dl,
            client,
            archives: paths.cache.join("archives"),
            entries,
            asked,
        })
    }

    /// Whether it offers the plugin `name`.
    pub(super) fn holds(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// The lockfile's source of its plugins.
    pub(super) fn source(&self) -> Source {
        Source::Registry(self.url.clone())
    }

    /// The candidates of the resolution: each version not yanked, and a
    /// yanked one that `lock` holds, as a yank keeps the installs of it; of
    /// `keep`, a plugin not asked for that `lock` holds of the registry, of
    /// the version that it holds alone.
    pub(super) fn candidates(
        &self,
        lock: Option<&Lock>,
        keep: bool,
    ) -> Result<Vec<Candidate>, String> {
        let mut candidates = Vec::new();
        for (name, entries) in &self.entries {
            let locked = lock
                .and_then(|lock| lock.packages.get(name))
                .filter(|locked| matches!(locked.source, Source::Registry(_)))
                .map(|locked| locked.version.as_str());
            let only = locked.filter(|_| keep && !self.asked.contains(name));
            for entry in entries {
                if entry.yanked && locked != Some(entry.vers.as_str()) {
                    continue;
                }
                if only.is_some_and(|only| only != entry.vers) {
                    continue;
                }
                candidates.push(Candidate {
                    name: name.clone(),
                    version: Version::parse(&entry.vers)
                        .map_err(|error| format!("`{name}` {}: {error}", entry.vers))?,
                    api: entry.api.clone(),
                    dependencies: entry
                        .deps
                        .iter()
                        .map(|dependency| (dependency.name.clone(), dependency.req.clone()))
                        .collect(),
                });
            }
        }
        Ok(candidates)
    }

    /// Whether the candidates of the plugin `name` are every version of
    /// the registry's, rather than the one that `lock` holds, which `keep`
    /// keeps of a plugin not asked for, as [`Offered::candidates`] gives
    /// them.
    pub(super) fn offers_all(&self, name: &str, lock: Option<&Lock>, keep: bool) -> bool {
        let held = lock
            .and_then(|lock| lock.packages.get(name))
            .is_some_and(|locked| matches!(locked.source, Source::Registry(_)));
        self.entries.contains_key(name) && !(keep && held && !self.asked.contains(name))
    }

    /// The versions that `lock` holds of the registry of the plugins not
    /// asked for, which the resolution prefers.
    pub(super) fn preferred(&self, lock: Option<&Lock>) -> BTreeMap<String, Version> {
        let Some(lock) = lock else {
            return BTreeMap::new();
        };
        self.entries
            .keys()
            .filter(|name| !self.asked.contains(*name))
            .filter_map(|name| {
                let locked = lock.packages.get(name)?;
                matches!(locked.source, Source::Registry(_))
                    .then(|| Version::parse(&locked.version).ok())
                    .flatten()
                    .map(|version| (name.clone(), version))
            })
            .collect()
    }

    /// The archive of the version `version` of the plugin `name`, of
    /// Sleip's cache or downloaded, its SHA-256 the index's, as `out` tells
    /// a download.
    pub(super) fn archive(&self, name: &str, version: &str, out: Out) -> Result<Vec<u8>, String> {
        let entry = self
            .entries
            .get(name)
            .and_then(|entries| entries.iter().find(|entry| entry.vers == version))
            .ok_or_else(|| format!("{} publishes no `{name}` {version}", self.url))?;
        let (owner, plugin) = name.split_once('/').unwrap_or((name, name));
        let cached = self
            .archives
            .join(format!("{owner}.{plugin}.{version}.tar.zst"));
        if let Ok(bytes) = std::fs::read(&cached)
            && Digest::of_bytes(&bytes).to_string() == entry.cksum
        {
            return Ok(bytes);
        }
        let url = self
            .dl
            .replace("{owner}", owner)
            .replace("{name}", plugin)
            .replace("{version}", version);
        let Some(client) = &self.client else {
            return Err(format!(
                "`{name}` {version} is downloaded from {url}, which `--offline` keeps from it"
            ));
        };
        out.tell(
            "downloading",
            json!({ "name": name, "version": version, "url": url }),
            &format!("Downloading {name} {version}…"),
        );
        let bytes =
            match client.get_within(&url, None, lokyr_pkg_model::archive::MOST_COMPRESSED)? {
                Fetched::Body { bytes, .. } => bytes,
                Fetched::NotModified | Fetched::Missing => {
                    return Err(format!("{url} holds no archive of `{name}` {version}"));
                }
            };
        let digest = Digest::of_bytes(&bytes).to_string();
        if digest != entry.cksum {
            return Err(format!(
                "the archive of `{name}` {version} is of {digest}, not of {}, which the registry's signed index gives",
                entry.cksum
            ));
        }
        // Kept for a later install, offline too, as it checked.
        let _ = std::fs::create_dir_all(&self.archives)
            .and_then(|()| lokyr_pkg_model::write_whole_bytes(&cached, &bytes));
        Ok(bytes)
    }
}

/// Whether `dl`, the address of the archives that the `config.json` of the
/// registry at `url` names, which no key signs, is of the registry's own
/// scheme and of no user, of the machine's own `http://` alone, so that it
/// leads to no file, device nor service of the machine's that the
/// registry's does not: an archive is checked against the signed index's
/// checksum wherever it comes from.
fn downloads_of(url: &str, dl: &str) -> Result<(), String> {
    use lokyr_pkg_model::address::Address;

    let sample = dl
        .replace("{owner}", "owner")
        .replace("{name}", "name")
        .replace("{version}", "0.0.0");
    let (Some(registry), Some(downloads)) = (Address::parse(url), Address::parse(&sample)) else {
        return Err(format!(
            "{url}'s config.json names its archives at {dl}, which is no address of no user"
        ));
    };
    let allowed = downloads.scheme == registry.scheme
        && match downloads.scheme {
            "https" => !downloads.host.is_empty(),
            "http" => downloads.is_loopback(),
            "file" => downloads.is_registry(),
            _ => false,
        };
    if allowed {
        Ok(())
    } else {
        Err(format!(
            "{url}'s config.json names its archives at {dl}, which is not of the registry's own scheme, `{}://`",
            registry.scheme
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archives_are_of_the_registry_s_own_scheme() {
        let dl = "https://github.com/acme/registry/releases/download/{owner}.{name}.{version}/{name}-{version}.tar.zst";
        assert!(downloads_of("https://pkg.lokyr.dev", dl).is_ok());
        assert!(downloads_of("file:///srv/mirror", "file:///srv/mirror/{name}").is_ok());
        assert!(downloads_of("http://127.0.0.1:8123", "http://127.0.0.1:8123/{name}").is_ok());
        for (url, dl) in [
            ("https://pkg.lokyr.dev", "file:///dev/zero"),
            ("https://pkg.lokyr.dev", "http://127.0.0.1:22/{name}"),
            ("http://127.0.0.1:8123", "http://intranet.example/{name}"),
            ("http://127.0.0.1:8123", "file:///etc/{name}"),
            ("https://pkg.lokyr.dev", "https://user@evil.example/{name}"),
        ] {
            assert!(downloads_of(url, dl).is_err(), "{url} {dl}");
        }
    }
}
