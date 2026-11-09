//! `sleip add` and `sleip update`: the plugins of `plugins.toml`, of
//! folders and git repositories, fetched, resolved against the app's plugin
//! API by PubGrub, their packages checked and stored, and `plugins.lock`
//! written whole, which the app reloads; under the store's lock. A plugin
//! added goes into `plugins.toml`, edited in place, once the rest holds. A
//! failure changes nothing: the packages that it stored go, and neither
//! file is written.
//!
//! The registry's plugins come with the registry: one of `plugins.toml`
//! that the lockfile holds stays as the lockfile holds it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use lokyr_pkg_model::list::{GitAt, List, Wanted};
use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::Manifest;
use lokyr_pkg_model::store::Digest;
use semver::Version;
use serde_json::json;
use sleip_resolver::Candidate;

use crate::args::Options;
use crate::output::Out;
use crate::paths::{self, Paths};
use crate::store_lock;

/// A plugin fetched of its folder or its repository: its manifest, the
/// folder of its package, and its lockfile's source.
struct Fetched {
    manifest: Manifest,
    folder: PathBuf,
    source: Source,
}

/// A folder of the cache that a git repository is cloned into, taken away
/// as it drops.
struct Clone(PathBuf);

impl Drop for Clone {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Adds the plugin `plugin`, its manifest's name if `None`, of `from`, to
/// the installation of `paths`, as `options` and `out` say.
pub fn add(
    paths: &Paths,
    options: Options,
    out: Out,
    plugin: Option<&str>,
    from: &Wanted,
) -> Result<(), String> {
    let _held = store_lock::lock(paths, out)?;
    let list_path = paths.list();
    let text = match std::fs::read_to_string(&list_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(format!("{} cannot be read: {error}", list_path.display())),
    };
    let mut clones = Vec::new();
    // The plugin's name, its manifest's, as the folder or the repository
    // holds it.
    let fetched = fetch(paths, options, plugin, from, &mut clones)?;
    let name = fetched.manifest.name.clone();
    // A folder of a relative path, as `.`, of its whole path, which the
    // list takes; one from `~` as it is written, as it holds on another
    // account.
    let written = match from {
        Wanted::Path(path) if !path.starts_with('/') && !path.starts_with('~') => {
            Wanted::Path(fetched.folder.display().to_string())
        }
        _ => from.clone(),
    };
    let edited = with_plugin(&text, &name, &written)
        .map_err(|error| format!("{}: {error}", list_path.display()))?;
    let list = List::parse(&edited)
        .map_err(|error| format!("{} would not read: {error}", list_path.display()))?;
    let lock = read_lock(paths)?;
    let mut fresh = BTreeMap::from([(name.clone(), fetched)]);
    for (other, wanted) in &list.plugins {
        if *other == name || matches!(wanted, Wanted::Registry(_)) {
            continue;
        }
        // The others of the list as the lockfile holds them, if it does.
        if lock
            .as_ref()
            .is_some_and(|lock| lock.packages.contains_key(other))
        {
            continue;
        }
        fresh.insert(
            other.clone(),
            fetch(paths, options, Some(other), wanted, &mut clones)?,
        );
    }
    let installed = install(paths, options, out, &list, lock.as_ref(), &fresh)?;
    if !installed {
        return Ok(());
    }
    lokyr_pkg_model::write_whole(&list_path, &edited)
        .map_err(|error| format!("{} cannot be written: {error}", list_path.display()))
}

/// Fetches the plugins of `plugins.toml` of `paths` again, or `plugin`
/// alone, as `options` and `out` say.
pub fn update(
    paths: &Paths,
    options: Options,
    out: Out,
    plugin: Option<&str>,
) -> Result<(), String> {
    let _held = store_lock::lock(paths, out)?;
    let list_path = paths.list();
    let list = match List::read(&list_path) {
        Ok(list) => list,
        Err(error) => return Err(error.to_string()),
    };
    if let Some(plugin) = plugin
        && !list.plugins.contains_key(plugin)
    {
        return Err(format!("`{plugin}` is not in plugins.toml"));
    }
    let lock = read_lock(paths)?;
    let mut clones = Vec::new();
    let mut fresh = BTreeMap::new();
    for (name, wanted) in &list.plugins {
        let asked = plugin.is_none_or(|plugin| plugin == name);
        let locked = lock
            .as_ref()
            .is_some_and(|lock| lock.packages.contains_key(name));
        if matches!(wanted, Wanted::Registry(_)) || (!asked && locked) {
            continue;
        }
        fresh.insert(
            name.clone(),
            fetch(paths, options, Some(name), wanted, &mut clones)?,
        );
    }
    install(paths, options, out, &list, lock.as_ref(), &fresh).map(|_| ())
}

/// The lockfile of `paths`, if there is one.
fn read_lock(paths: &Paths) -> Result<Option<Lock>, String> {
    Lock::read(&paths.lock()).map_err(|error| error.to_string())
}

/// Resolves the plugins of `list`, of `fresh`, those fetched, and of the
/// others as `lock` holds them, against the app's plugin API; stores the
/// packages of `fresh`; and writes the lockfile of `paths`, the folders that
/// `sleip dev` links kept: whether it wrote it, as `--locked` refuses a
/// change. A failure takes the packages that it stored away.
fn install(
    paths: &Paths,
    options: Options,
    out: Out,
    list: &List,
    lock: Option<&Lock>,
    fresh: &BTreeMap<String, Fetched>,
) -> Result<bool, String> {
    let api = match paths::app_api() {
        Some((api, _)) => api,
        None => {
            let api = lokyr_plugin_sdk::API_VERSION.to_owned();
            out.tell(
                "note",
                json!({ "message": format!("no Lokyr found: the plugin API {api}, this Sleip's, taken") }),
                &format!("No Lokyr found: the plugin API {api}, this Sleip's, is taken."),
            );
            api
        }
    };
    let api_version = Version::parse(&api)
        .map_err(|error| format!("`{api}` is no plugin API's version: {error}"))?;
    // Each plugin's candidate: the fetched's, or the lockfile's of its store.
    let kept: BTreeMap<String, Locked> = lock
        .map(|lock| {
            lock.packages
                .iter()
                .filter(|(name, _)| !fresh.contains_key(*name))
                .map(|(name, locked)| (name.clone(), locked.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut candidates = Vec::new();
    for fetched in fresh.values() {
        candidates.push(candidate(&fetched.manifest)?);
    }
    for (name, locked) in &kept {
        if let Some(manifest) = stored_manifest(paths, locked) {
            candidates.push(candidate(&manifest)?);
        } else {
            return Err(format!(
                "`{name}`'s package is not in the store: `sleip update {name}` fetches it again"
            ));
        }
    }
    let wanted: Vec<(String, String)> = list
        .plugins
        .iter()
        .map(|(name, wanted)| {
            let constraint = match wanted {
                Wanted::Registry(constraint) => constraint.clone(),
                Wanted::Path(_) | Wanted::Git { .. } => "*".to_owned(),
            };
            (name.clone(), constraint)
        })
        .collect();
    let picked = sleip_resolver::resolve(&api_version, &wanted, &candidates)?;
    // The packages stored, those new to the store taken away of a failure.
    let mut stored: Vec<Digest> = Vec::new();
    let made = (|| {
        let mut packages = BTreeMap::new();
        for (name, version) in &picked {
            let entry = match (fresh.get(name), kept.get(name)) {
                (Some(fetched), _) => {
                    let archive = lokyr_pkg_model::archive::pack(&fetched.folder)
                        .map_err(|error| format!("`{name}`'s package: {error}"))?;
                    let digest = lokyr_pkg_model::store::digest_of(&fetched.folder)
                        .map_err(|error| format!("`{name}`'s package: {error}"))?;
                    let held = paths.store.folder(&digest).is_dir();
                    let unpacked = paths
                        .store
                        .add(archive.as_slice())
                        .map_err(|error| format!("`{name}`'s package: {error}"))?;
                    if !held {
                        stored.push(unpacked.digest);
                    }
                    let manifest = &fetched.manifest;
                    let dependencies = manifest
                        .dependencies
                        .keys()
                        .filter_map(|needed| {
                            picked
                                .get(needed)
                                .map(|version| (needed.clone(), version.to_string()))
                        })
                        .collect();
                    Locked {
                        version: version.to_string(),
                        kind: manifest.kind,
                        source: fetched.source.clone(),
                        checksum: Some(unpacked.digest),
                        capabilities: manifest.capabilities.clone(),
                        dependencies,
                    }
                }
                (None, Some(locked)) => locked.clone(),
                (None, None) => return Err(format!("`{name}` was resolved of no package")),
            };
            packages.insert(name.clone(), entry);
        }
        // The folders that `sleip dev` links, which no list names.
        if let Some(lock) = lock {
            for (name, locked) in &lock.packages {
                if matches!(locked.source, Source::Dev(_)) && !list.plugins.contains_key(name) {
                    packages.insert(name.clone(), locked.clone());
                }
            }
        }
        let new = Lock {
            api: api.clone(),
            packages,
        };
        if options.locked && lock != Some(&new) {
            return Err("the plugins would change plugins.lock, which `--locked` keeps".to_owned());
        }
        Ok(new)
    })();
    let new = match made {
        Ok(new) => new,
        Err(error) => {
            undo(paths, &stored);
            return Err(error);
        }
    };
    if lock == Some(&new) {
        out.tell("done", json!({ "changed": false }), "Nothing changed.");
        return Ok(true);
    }
    if let Err(error) = new.write(&paths.lock()) {
        undo(paths, &stored);
        return Err(format!(
            "{} cannot be written: {error}",
            paths.lock().display()
        ));
    }
    for (name, locked) in &new.packages {
        let was = lock.and_then(|lock| lock.packages.get(name));
        if was == Some(locked) {
            continue;
        }
        let capabilities: Vec<String> = locked
            .capabilities
            .iter()
            .map(ToString::to_string)
            .collect();
        let asks = if capabilities.is_empty() {
            String::new()
        } else {
            format!(", which asks for {}", capabilities.join(", "))
        };
        out.tell(
            "installed",
            json!({
                "name": name,
                "version": locked.version,
                "source": locked.source.to_string(),
                "capabilities": capabilities,
            }),
            &format!("Installed {name} {}{asks}.", locked.version),
        );
    }
    out.tell(
        "done",
        json!({ "changed": true }),
        "Lokyr loads them at once.",
    );
    Ok(true)
}

/// Takes the packages of `stored`, new to the store of `paths`, away.
fn undo(paths: &Paths, stored: &[Digest]) {
    for digest in stored {
        let _ = paths.store.remove(digest);
    }
}

/// The manifest of the package of `locked`, of the store of `paths` or of
/// the folder that `sleip dev` links.
fn stored_manifest(paths: &Paths, locked: &Locked) -> Option<Manifest> {
    let folder = match (&locked.source, &locked.checksum) {
        (Source::Dev(folder), _) => PathBuf::from(folder),
        (_, Some(digest)) => paths.store.folder(digest),
        (_, None) => return None,
    };
    let text = std::fs::read_to_string(folder.join("plugin.toml")).ok()?;
    Manifest::parse(&text).ok()
}

/// The resolution's candidate of the plugin of `manifest`.
fn candidate(manifest: &Manifest) -> Result<Candidate, String> {
    let version = manifest.version.as_deref().unwrap_or("0.0.0");
    Ok(Candidate {
        name: manifest.name.clone(),
        version: Version::parse(version)
            .map_err(|error| format!("`{}`'s version: {error}", manifest.name))?,
        api: manifest.api.clone(),
        dependencies: manifest.dependencies.clone(),
    })
}

/// Fetches the plugin `name`, its manifest's if `None`, of `wanted`: of its
/// folder, or of its git repository at its tag or its commit, cloned into
/// Sleip's cache, `clones` holding the clone until the install ends.
fn fetch(
    paths: &Paths,
    options: Options,
    name: Option<&str>,
    wanted: &Wanted,
    clones: &mut Vec<Clone>,
) -> Result<Fetched, String> {
    let (folder, source) = match wanted {
        Wanted::Path(path) => {
            let folder = expand(path)
                .and_then(|folder| std::fs::canonicalize(&folder).ok())
                .ok_or_else(|| format!("{path} is no folder"))?;
            let source = Source::Path(folder.display().to_string());
            (folder, source)
        }
        Wanted::Git { url, at } => {
            if options.offline && !url.starts_with("file://") {
                return Err(format!(
                    "{url} is fetched over the network, which `--offline` keeps from it"
                ));
            }
            let (folder, commit) = clone(paths, url, at, clones)?;
            (
                folder,
                Source::Git {
                    url: url.clone(),
                    commit,
                },
            )
        }
        Wanted::Registry(_) => {
            return Err(format!(
                "`{}` is of the registry, whose plugins come with registry v0: add it from a folder or a git repository",
                name.unwrap_or_default()
            ));
        }
    };
    let text = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| format!("{} holds no plugin.toml: {error}", describe_source(&source)))?;
    let manifest = Manifest::parse(&text)
        .map_err(|error| format!("the plugin.toml of {}: {error}", describe_source(&source)))?;
    if let Some(name) = name
        && manifest.name != name
    {
        return Err(format!(
            "{} holds `{}`, not `{name}`",
            describe_source(&source),
            manifest.name
        ));
    }
    Ok(Fetched {
        manifest,
        folder,
        source,
    })
}

/// `path`, absolute or of `~`, as an absolute path.
fn expand(path: &str) -> Option<PathBuf> {
    match path.strip_prefix('~') {
        Some(rest) => std::env::home_dir().map(|home| home.join(rest.trim_start_matches('/'))),
        None => Some(PathBuf::from(path)),
    }
}

/// Clones the git repository at `url` into Sleip's cache of `paths`, at
/// `at`, its tag or its commit: the folder of its files, of `clones`, and
/// the commit's whole hash.
fn clone(
    paths: &Paths,
    url: &str,
    at: &GitAt,
    clones: &mut Vec<Clone>,
) -> Result<(PathBuf, String), String> {
    static CLONES: AtomicUsize = AtomicUsize::new(0);
    let folder = paths.cache.join("git").join(format!(
        "{}-{}",
        std::process::id(),
        CLONES.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder)
        .map_err(|error| format!("{} cannot be made: {error}", folder.display()))?;
    clones.push(Clone(folder.clone()));
    git(
        None,
        &[
            "clone",
            "--quiet",
            "--no-checkout",
            "--",
            url,
            &folder.display().to_string(),
        ],
    )
    .map_err(|error| format!("{url} cannot be cloned: {error}"))?;
    let wanted = match at {
        GitAt::Tag(tag) => format!("refs/tags/{tag}^{{commit}}"),
        GitAt::Rev(rev) => format!("{rev}^{{commit}}"),
    };
    let commit = git(
        Some(&folder),
        &["rev-parse", "--verify", "--quiet", &wanted],
    )
    .map_err(|_| match at {
        GitAt::Tag(tag) => format!("{url} has no tag `{tag}`"),
        GitAt::Rev(rev) => format!("{url} has no commit `{rev}`"),
    })?;
    git(Some(&folder), &["checkout", "--quiet", "--detach", &commit])
        .map_err(|error| format!("{url} at {commit} cannot be checked out: {error}"))?;
    Ok((folder, commit))
}

/// Runs git of `args`, in `folder` if given, of no question at the
/// terminal: its output's first line, or its error.
fn git(folder: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut command = Command::new("git");
    if let Some(folder) = folder {
        command.arg("-C").arg(folder);
    }
    let output = command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("git cannot run: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned())
}

/// `source` as a message tells it.
fn describe_source(source: &Source) -> String {
    match source {
        Source::Path(path) | Source::Dev(path) => path.clone(),
        Source::Git { url, commit } => format!("{url} at {commit}"),
        Source::Registry(url) => url.clone(),
    }
}

/// `text`, a `plugins.toml`, of the plugin `name` of `from`, its comments
/// and order kept: in place of an entry of its name, or after the others.
fn with_plugin(text: &str, name: &str, from: &Wanted) -> Result<String, String> {
    let mut document: toml_edit::DocumentMut = text.parse().map_err(|error| format!("{error}"))?;
    let plugins = document
        .entry("plugins")
        .or_insert_with(toml_edit::table)
        .as_table_like_mut()
        .ok_or_else(|| "its `plugins` is no table".to_owned())?;
    let mut entry = toml_edit::InlineTable::new();
    match from {
        Wanted::Path(path) => {
            entry.insert("path", path.as_str().into());
        }
        Wanted::Git { url, at } => {
            entry.insert("git", url.as_str().into());
            match at {
                GitAt::Tag(tag) => entry.insert("tag", tag.as_str().into()),
                GitAt::Rev(rev) => entry.insert("rev", rev.as_str().into()),
            };
        }
        Wanted::Registry(constraint) => {
            plugins.insert(name, toml_edit::value(constraint.as_str()));
            return Ok(document.to_string());
        }
    }
    plugins.insert(name, toml_edit::value(entry));
    Ok(document.to_string())
}
