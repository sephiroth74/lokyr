//! `sleip add` and `sleip update`: the plugins of `plugins.toml`, of
//! folders and git repositories, fetched, resolved against the app's plugin
//! API by PubGrub, their packages checked and stored, and `plugins.lock`
//! written whole, which the app reloads; under the store's lock. A plugin
//! added goes into `plugins.toml`, edited in place, once the rest holds. A
//! failure changes nothing: the packages that it stored go, and neither
//! file is written.
//!
//! A plugin of the registry is of its versions in the registry's signed
//! index, the archive of the version picked downloaded and checked
//! against the index's checksum; one of `plugins.toml` that the lockfile
//! holds stays as the lockfile holds it, unless `update` asks for it, and
//! so do the plugins that it depends on, unless the resolution needs
//! another version. A plugin that the lockfile holds of the registry but
//! the store lacks, as of a lockfile copied to another machine, is fetched
//! again at its version, its package of the lockfile's digest; one that
//! the store holds is not downloaded again. `update --locked` installs the
//! lockfile as it is. `plugins.toml` is written before the lockfile, and
//! put back if the lockfile cannot be.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use lokyr_pkg_model::list::{GitAt, List, Wanted};
use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::Manifest;
use lokyr_pkg_model::store::Digest;
use semver::Version;
use serde_json::json;
use sleip_resolver::Candidate;

use super::git;
use crate::args::Options;

mod registry;

use crate::output::Out;
use crate::paths::{self, Paths};
use crate::store_lock;
use registry::Offered;

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
    if let Wanted::Registry(constraint) = from {
        let name = plugin.ok_or_else(|| {
            "a plugin of the registry is added by its name, as owner/name".to_owned()
        })?;
        return add_offered(paths, options, out, &text, name, constraint, &mut clones);
    }
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
    let mut fresh = others(paths, options, &list, lock.as_ref(), &name, &mut clones)?;
    fresh.insert(name.clone(), fetched);
    let offered = Offered::gather(
        paths,
        options,
        out,
        &list,
        lock.as_ref(),
        unlocked(&list, lock.as_ref(), None),
        &also(paths, &list, lock.as_ref(), &fresh),
    )?;
    install(
        paths,
        options,
        out,
        &list,
        lock.as_ref(),
        &fresh,
        &offered,
        &|_| Ok(Some(edited.clone())),
    )
    .map(|_| ())
}

/// Adds the registry's plugin `name` of `constraint`, or of the version
/// that the resolution picks if it is empty, which `plugins.toml` then
/// takes as `^<version>`, to the list of `text`, as [`add`] does.
fn add_offered(
    paths: &Paths,
    options: Options,
    out: Out,
    text: &str,
    name: &str,
    constraint: &str,
    clones: &mut Vec<Clone>,
) -> Result<(), String> {
    let list_path = paths.list();
    let resolved = if constraint.is_empty() {
        "*"
    } else {
        sleip_resolver::ranges_of(constraint)?;
        constraint
    };
    let edited = with_plugin(text, name, &Wanted::Registry(resolved.to_owned()))
        .map_err(|error| format!("{}: {error}", list_path.display()))?;
    let list = List::parse(&edited)
        .map_err(|error| format!("{} would not read: {error}", list_path.display()))?;
    let lock = read_lock(paths)?;
    let fresh = others(paths, options, &list, lock.as_ref(), name, clones)?;
    let mut names = unlocked(&list, lock.as_ref(), None);
    names.insert(name.to_owned());
    let offered = Offered::gather(
        paths,
        options,
        out,
        &list,
        lock.as_ref(),
        names,
        &also(paths, &list, lock.as_ref(), &fresh),
    )?;
    install(
        paths,
        options,
        out,
        &list,
        lock.as_ref(),
        &fresh,
        &offered,
        &|new| {
            // Of no constraint given, the version picked's, as `^0.1.8`.
            if !constraint.is_empty() {
                return Ok(Some(edited.clone()));
            }
            let version = new
                .packages
                .get(name)
                .map(|locked| locked.version.clone())
                .ok_or_else(|| format!("`{name}` was resolved of no version"))?;
            with_plugin(text, name, &Wanted::Registry(format!("^{version}")))
                .map(Some)
                .map_err(|error| format!("{}: {error}", list_path.display()))
        },
    )
    .map(|_| ())
}

/// The registry's plugins that an install gathers beside those asked
/// for, of the versions that `lock` holds if it holds them: those that it
/// holds of the registry but the store of `paths` lacks, as of a lockfile
/// copied to another machine, fetched again, but those that `list` takes
/// of a folder or a repository, and those of `fresh`; and those that the
/// plugins of folders, repositories and `sleip dev` folders need, of
/// `fresh` and of the lockfile, but those that `list` takes so.
fn also(
    paths: &Paths,
    list: &List,
    lock: Option<&Lock>,
    fresh: &BTreeMap<String, Fetched>,
) -> BTreeSet<String> {
    let elsewhere = |name: &str| {
        fresh.contains_key(name)
            || matches!(
                list.plugins.get(name),
                Some(Wanted::Path(_) | Wanted::Git { .. })
            )
            || lock.is_some_and(|lock| linked(lock, name))
    };
    let mut also = BTreeSet::new();
    if let Some(lock) = lock {
        also.extend(
            lock.packages
                .iter()
                .filter(|(_, locked)| matches!(locked.source, Source::Registry(_)))
                .filter(|(name, locked)| !elsewhere(name) && !stored_as(paths, name, locked))
                .map(|(name, _)| name.clone()),
        );
    }
    let mut manifests: Vec<Manifest> = fresh
        .values()
        .map(|fetched| fetched.manifest.clone())
        .collect();
    if let Some(lock) = lock {
        manifests.extend(
            lock.packages
                .iter()
                .filter(|(name, locked)| {
                    !fresh.contains_key(*name) && !matches!(locked.source, Source::Registry(_))
                })
                .filter_map(|(_, locked)| stored_manifest(paths, locked)),
        );
    }
    for manifest in &manifests {
        also.extend(
            manifest
                .dependencies
                .keys()
                .filter(|needed| !elsewhere(needed))
                .cloned(),
        );
    }
    also
}

/// The plugins of `list` of a folder or a git repository but `except`
/// that `lock` lacks, fetched, `clones` holding the clones.
fn others(
    paths: &Paths,
    options: Options,
    list: &List,
    lock: Option<&Lock>,
    except: &str,
    clones: &mut Vec<Clone>,
) -> Result<BTreeMap<String, Fetched>, String> {
    let mut fresh = BTreeMap::new();
    for (other, wanted) in &list.plugins {
        if other == except || matches!(wanted, Wanted::Registry(_)) {
            continue;
        }
        // The others of the list as the lockfile holds them, if it holds
        // them as the list asks.
        if lock.is_some_and(|lock| held_as(lock, other, wanted)) {
            continue;
        }
        fresh.insert(
            other.clone(),
            fetch(paths, options, Some(other), wanted, clones)?,
        );
    }
    Ok(fresh)
}

/// The registry's plugins of `list` that `lock` does not hold as the list
/// asks, and `asked`, of `update`, each of them if `Some(None)`, or the
/// one named; none that `sleip dev` links, which stays as it is.
fn unlocked(list: &List, lock: Option<&Lock>, asked: Option<Option<&str>>) -> BTreeSet<String> {
    list.plugins
        .iter()
        .filter(|(_, wanted)| matches!(wanted, Wanted::Registry(_)))
        .filter(|(name, _)| !lock.is_some_and(|lock| linked(lock, name)))
        .filter(|(name, wanted)| {
            let locked = lock.is_some_and(|lock| held_as(lock, name, wanted));
            let updated = asked.is_some_and(|asked| asked.is_none_or(|asked| asked == *name));
            !locked || updated
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Whether `lock` holds `name` of a folder that `sleip dev` links.
fn linked(lock: &Lock, name: &str) -> bool {
    lock.packages
        .get(name)
        .is_some_and(|locked| matches!(locked.source, Source::Dev(_)))
}

/// Whether `lock` holds `name` as `wanted` asks: of the registry, of a
/// version that its constraint takes; of the folder that it names; of the
/// repository that it names, at a commit that its `rev` begins, a tag
/// taken as it was locked; of a folder that `sleip dev` links, as it is; of
/// a folder that is no longer there, as it was locked.
/// One that the list's entry, edited by hand, no longer asks for is
/// resolved afresh.
fn held_as(lock: &Lock, name: &str, wanted: &Wanted) -> bool {
    let Some(locked) = lock.packages.get(name) else {
        return false;
    };
    match (wanted, &locked.source) {
        (_, Source::Dev(_)) => true,
        (Wanted::Registry(constraint), Source::Registry(_)) => {
            match (
                sleip_resolver::ranges_of(constraint),
                Version::parse(&locked.version),
            ) {
                (Ok(ranges), Ok(version)) => ranges.contains(&version),
                _ => false,
            }
        }
        (Wanted::Path(path), Source::Path(held)) => match expand(path) {
            Some(folder) => match std::fs::canonicalize(folder) {
                Ok(folder) => folder.display().to_string() == *held,
                // A folder moved or removed since, of which nothing can be
                // fetched: the lockfile's package stays, as the store
                // holds it, until the plugin is asked for by name.
                Err(_) => true,
            },
            None => false,
        },
        (Wanted::Git { url, at }, Source::Git { url: held, commit }) => {
            url == held
                && match at {
                    GitAt::Rev(rev) => commit.starts_with(rev.as_str()),
                    GitAt::Tag(_) => true,
                }
        }
        _ => false,
    }
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
            .is_some_and(|lock| held_as(lock, name, wanted));
        // One that `sleip dev` links stays as it is.
        let dev = lock.as_ref().is_some_and(|lock| linked(lock, name));
        if matches!(wanted, Wanted::Registry(_)) || (!asked && locked) || dev {
            continue;
        }
        fresh.insert(
            name.clone(),
            fetch(paths, options, Some(name), wanted, &mut clones)?,
        );
    }
    // Of `--locked`, the lockfile as it is, nothing asked for afresh but
    // what it lacks, which then fails.
    let asked = if options.locked {
        unlocked(&list, lock.as_ref(), None)
    } else {
        let mut asked = unlocked(&list, lock.as_ref(), Some(plugin));
        // Of every plugin, the ones that they depend on too.
        if plugin.is_none()
            && let Some(lock) = &lock
        {
            asked.extend(
                lock.packages
                    .iter()
                    .filter(|(_, locked)| matches!(locked.source, Source::Registry(_)))
                    .map(|(name, _)| name.clone()),
            );
        }
        asked
    };
    let offered = Offered::gather(
        paths,
        options,
        out,
        &list,
        lock.as_ref(),
        asked,
        &also(paths, &list, lock.as_ref(), &fresh),
    )?;
    install(
        paths,
        options,
        out,
        &list,
        lock.as_ref(),
        &fresh,
        &offered,
        &|_| Ok(None),
    )
    .map(|_| ())
}

/// The lockfile of `paths`, if there is one.
fn read_lock(paths: &Paths) -> Result<Option<Lock>, String> {
    Lock::read(&paths.lock()).map_err(|error| error.to_string())
}

/// Resolves the plugins of `list`, of `fresh`, those fetched, and of the
/// others as `lock` holds them, against the app's plugin API; stores the
/// packages of `fresh`; and writes the lockfile of `paths`, the folders that
/// `sleip dev` links kept: the lockfile in force, as `--locked` refuses a
/// change. The registry's plugins of `offered` are of their versions'
/// candidates, the archive of the one picked downloaded unless the store
/// holds it as `lock` does. `plugins.toml` is written of `edit`, of the
/// lockfile resolved, if it gives a text, before the lockfile, and put
/// back if the lockfile cannot be written. A failure takes the packages
/// that it stored away.
#[allow(clippy::too_many_arguments)]
fn install(
    paths: &Paths,
    options: Options,
    out: Out,
    list: &List,
    lock: Option<&Lock>,
    fresh: &BTreeMap<String, Fetched>,
    offered: &Offered,
    edit: &dyn Fn(&Lock) -> Result<Option<String>, String>,
) -> Result<Lock, String> {
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
                .filter(|(name, _)| !fresh.contains_key(*name) && !offered.holds(name))
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
    // The registry's plugins not asked for of the versions that the
    // lockfile holds, first; of any, if the resolution needs another, but
    // of `--locked`.
    // The registry's plugins not asked for of the versions that the
    // lockfile holds, as long as they hold: a conflict moves its plugins
    // alone; of `--locked`, of those versions alone.
    candidates.extend(offered.candidates(lock, options.locked)?);
    let preferred = if options.locked {
        BTreeMap::new()
    } else {
        offered.preferred(lock)
    };
    let picked =
        sleip_resolver::resolve_preferring(&api_version, &wanted, &candidates, &preferred)?;
    // The packages stored, those new to the store taken away of a failure.
    let mut stored: Vec<Digest> = Vec::new();
    let made = (|| {
        let mut packages = BTreeMap::new();
        for (name, version) in &picked {
            let entry = match (fresh.get(name), kept.get(name)) {
                (Some(fetched), _) => {
                    let archive = lokyr_pkg_model::archive::pack(&fetched.folder)
                        .map_err(|error| format!("`{name}`'s package: {error}"))?;
                    let added = paths
                        .store
                        .add(archive.as_slice())
                        .map_err(|error| format!("`{name}`'s package: {error}"))?;
                    let unpacked = added.unpacked;
                    if added.new {
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
                (None, _) if offered.holds(name) => {
                    let version = version.to_string();
                    let held = lock
                        .and_then(|lock| lock.packages.get(name))
                        .filter(|locked| {
                            locked.version == version && locked.source == offered.source()
                        });
                    // As the lockfile holds it and the store too: no
                    // download.
                    if let Some(held) = held.filter(|held| stored_as(paths, name, held)) {
                        packages.insert(name.clone(), of_picked(held, &picked));
                        continue;
                    }
                    let archive = offered.archive(name, &version, out)?;
                    let added = paths
                        .store
                        .add(archive.as_slice())
                        .map_err(|error| format!("`{name}` {version}'s package: {error}"))?;
                    let unpacked = added.unpacked;
                    if added.new {
                        stored.push(unpacked.digest);
                    }
                    let manifest = &unpacked.manifest;
                    if manifest.name != *name
                        || manifest.version.as_deref() != Some(version.as_str())
                    {
                        return Err(format!(
                            "the archive of `{name}` {version} holds `{}` {}",
                            manifest.name,
                            manifest.version.as_deref().unwrap_or("of no version")
                        ));
                    }
                    // Fetched again as the lockfile holds it: of its digest.
                    if let Some(digest) = held.and_then(|held| held.checksum.as_ref())
                        && *digest != unpacked.digest
                    {
                        return Err(format!(
                            "the package of `{name}` {version} is of {}, not of {digest}, which plugins.lock holds: the registry's archive is not the one locked",
                            unpacked.digest
                        ));
                    }
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
                        version,
                        kind: manifest.kind,
                        source: offered.source(),
                        checksum: Some(unpacked.digest),
                        capabilities: manifest.capabilities.clone(),
                        dependencies,
                    }
                }
                (None, Some(locked)) => of_picked(locked, &picked),
                (None, None) => return Err(format!("`{name}` was resolved of no package")),
            };
            packages.insert(name.clone(), entry);
        }
        // The folders that `sleip dev` links, which no list names, nor the
        // resolution took, as it takes them as they are.
        if let Some(lock) = lock {
            for (name, locked) in &lock.packages {
                if matches!(locked.source, Source::Dev(_))
                    && !list.plugins.contains_key(name)
                    && !packages.contains_key(name)
                {
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
    // `plugins.toml` first, put back if the lockfile cannot be written.
    let list_path = paths.list();
    let old_list = std::fs::read_to_string(&list_path).ok();
    let edited = match edit(&new) {
        Ok(edited) => edited.filter(|text| old_list.as_deref() != Some(text.as_str())),
        Err(error) => {
            undo(paths, &stored);
            return Err(error);
        }
    };
    if let Some(text) = &edited
        && let Err(error) = lokyr_pkg_model::write_whole(&list_path, text)
    {
        undo(paths, &stored);
        return Err(format!(
            "{} cannot be written: {error}",
            list_path.display()
        ));
    }
    for name in &stored_again(lock, &new, &stored) {
        out.tell(
            "restored",
            json!({ "name": name, "version": new.packages[name].version }),
            &format!(
                "Fetched {name} {} again, as plugins.lock holds it.",
                new.packages[name].version
            ),
        );
    }
    if lock == Some(&new) {
        out.tell("done", json!({ "changed": false }), "Nothing changed.");
        return Ok(new);
    }
    if let Err(error) = new.write(&paths.lock()) {
        if edited.is_some() {
            let _ = match &old_list {
                Some(text) => lokyr_pkg_model::write_whole(&list_path, text),
                None => std::fs::remove_file(&list_path),
            };
        }
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
    Ok(new)
}

/// The plugins that `new` holds as `lock` does, whose packages, new to the
/// store, are of `stored`: fetched again.
fn stored_again(lock: Option<&Lock>, new: &Lock, stored: &[Digest]) -> Vec<String> {
    new.packages
        .iter()
        .filter(|(name, locked)| {
            lock.and_then(|lock| lock.packages.get(*name)) == Some(*locked)
                && locked
                    .checksum
                    .as_ref()
                    .is_some_and(|digest| stored.contains(digest))
        })
        .map(|(name, _)| name.clone())
        .collect()
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

/// `locked`, its dependencies of the versions that the resolution `picked`
/// took, as one of them may have moved.
fn of_picked(locked: &Locked, picked: &BTreeMap<String, Version>) -> Locked {
    Locked {
        dependencies: locked
            .dependencies
            .keys()
            .filter_map(|needed| {
                picked
                    .get(needed)
                    .map(|version| (needed.clone(), version.to_string()))
            })
            .collect(),
        ..locked.clone()
    }
}

/// Whether the store of `paths` holds the package of `locked`, the plugin
/// `name`, of its name and version: a digest of another plugin's package,
/// as of a lockfile changed by hand, is none.
fn stored_as(paths: &Paths, name: &str, locked: &Locked) -> bool {
    stored_manifest(paths, locked).is_some_and(|manifest| {
        manifest.name == name && manifest.version.as_deref() == Some(locked.version.as_str())
    })
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
                "`{}` is of the registry, whose plugins this Sleip cannot fetch yet: add it from a folder or a git repository",
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
