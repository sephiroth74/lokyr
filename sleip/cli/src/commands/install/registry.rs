//! The registry's plugins of an install: their versions, of the registry's
//! signed index, which Sleip checks and caches, each a candidate of the
//! resolution, yanked ones left out but one that the lockfile holds; and
//! the archives of the versions picked, downloaded or of Sleip's cache,
//! each checked against the index's checksum before the store takes it.

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
        }
    }

    /// The versions of the registry's plugins `names` of the default
    /// registry of `list`, and of the plugins that they depend on but those
    /// that `list` takes of a folder or a git repository, of the index
    /// fetched, checked against the root keys and cached, or of the cache
    /// alone of `--offline`; none of no name.
    pub(super) fn gather(
        paths: &Paths,
        options: Options,
        list: &List,
        names: BTreeSet<String>,
    ) -> Result<Self, String> {
        if names.is_empty() {
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
        let mut entries = BTreeMap::new();
        let mut left: Vec<String> = names.into_iter().collect();
        while let Some(name) = left.pop() {
            if entries.contains_key(&name) {
                continue;
            }
            let index = remote
                .index(&name, &keys)?
                .ok_or_else(|| format!("{url} publishes no plugin `{name}`"))?;
            for entry in &index.entries {
                for dependency in &entry.deps {
                    let elsewhere = matches!(
                        list.plugins.get(&dependency.name),
                        Some(Wanted::Path(_) | Wanted::Git { .. })
                    );
                    if !elsewhere && !entries.contains_key(&dependency.name) {
                        left.push(dependency.name.clone());
                    }
                }
            }
            entries.insert(name, index.entries);
        }
        Ok(Self {
            url,
            dl: config.dl,
            client,
            archives: paths.cache.join("archives"),
            entries,
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
    /// yanked one that `lock` holds, as a yank keeps the installs of it.
    pub(super) fn candidates(&self, lock: Option<&Lock>) -> Result<Vec<Candidate>, String> {
        let mut candidates = Vec::new();
        for (name, entries) in &self.entries {
            let locked = lock
                .and_then(|lock| lock.packages.get(name))
                .map(|locked| locked.version.as_str());
            for entry in entries {
                if entry.yanked && locked != Some(entry.vers.as_str()) {
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
