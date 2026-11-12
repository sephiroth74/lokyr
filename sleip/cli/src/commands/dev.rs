//! `sleip dev`: the plugin of the current folder checked, as `sleip check`
//! checks it, and linked into the app's plugins, an entry of `plugins.lock`
//! of the folder, `dev+`, of no checksum, which the app watches and
//! reloads at each save; checked again at each change of its files, its
//! faults told; and taken back as the user interrupts it, the entry of the
//! plugin's name that it took the place of, an installed one, given back.
//! The lockfile is changed under the store's lock, which the command does
//! not hold meanwhile.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::{Kind, Manifest};
use serde_json::json;

use super::check;
use crate::output::Out;
use crate::paths::{self, Paths};
use crate::store_lock;

/// How often the folder's files are looked at for a change.
const LOOK_EVERY: Duration = Duration::from_millis(250);

/// Links the plugin of the current folder, of `paths`, until the user
/// interrupts it, as `out` tells.
pub fn dev(paths: &Paths, out: Out) -> Result<(), String> {
    let folder = std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .map_err(|error| format!("the current folder cannot be read: {error}"))?;
    let text = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| format!("plugin.toml cannot be read, as no plugin is in this folder or the file is not there: {error}"))?;
    let manifest = Manifest::parse(&text).map_err(|error| format!("plugin.toml:{error}"))?;
    if manifest.kind != Kind::Luau {
        return Err(format!(
            "`{}` is not a plugin of Luau, which `sleip dev` links",
            manifest.name
        ));
    }
    // The first Ctrl-C ends it, its entry taken back; a second one, as that
    // hangs, at once.
    let stop = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register_conditional_shutdown(signal, 1, Arc::clone(&stop))
            .and_then(|_| signal_hook::flag::register(signal, Arc::clone(&stop)))
            .map_err(|error| format!("the interruption cannot be caught: {error}"))?;
    }
    // The files as they are before the first check, so that a save during
    // the check or the link, or right after it is told, is checked again.
    let mut seen = fingerprint(&folder);
    let problems = check::problems_in(&folder, out);
    out.tell(
        "checked",
        json!({ "problems": problems }),
        &check::summary(problems),
    );
    let (replaced, made) = link(paths, out, &manifest, &folder)?;
    let name = &manifest.name;
    let instead = replaced.as_ref().map_or_else(String::new, |locked| {
        format!(
            ", in place of the installed {name} {} until it ends",
            locked.version
        )
    });
    out.tell(
        "linked",
        json!({
            "name": name,
            "folder": folder.display().to_string(),
            "replaced": replaced.as_ref().map(|locked| locked.version.clone()),
        }),
        &format!(
            "Linked {name} of {}{instead}: Lokyr reloads it at each save. Ctrl-C ends it.",
            folder.display()
        ),
    );
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(LOOK_EVERY);
        let now = fingerprint(&folder);
        if now == seen {
            continue;
        }
        seen = now;
        let problems = check::problems_in(&folder, out);
        out.tell(
            "checked",
            json!({ "problems": problems }),
            &check::summary(problems),
        );
    }
    unlink(paths, out, (name, &folder), replaced, made)?;
    out.tell(
        "unlinked",
        json!({ "name": name }),
        &format!("Unlinked {name}."),
    );
    Ok(())
}

/// Adds the entry of the plugin of `manifest`, of the folder `folder`, to
/// the lockfile of `paths`, under the store's lock: the entry of its name
/// that it takes the place of, if one was there, and whether it made the
/// lockfile, as none was there.
fn link(
    paths: &Paths,
    out: Out,
    manifest: &Manifest,
    folder: &Path,
) -> Result<(Option<Locked>, bool), String> {
    let _held = store_lock::lock(paths, out)?;
    let path = paths.lock();
    let read = Lock::read(&path).map_err(|error| error.to_string())?;
    let made = read.is_none();
    let mut lock = read.unwrap_or_else(|| Lock {
        api: paths::app_api()
            .map_or_else(|| lokyr_plugin_sdk::API_VERSION.to_owned(), |(api, _)| api),
        packages: BTreeMap::new(),
    });
    let entry = Locked {
        version: manifest
            .version
            .clone()
            .unwrap_or_else(|| "0.0.0".to_owned()),
        kind: Kind::Luau,
        source: Source::Dev(folder.display().to_string()),
        checksum: None,
        capabilities: manifest.capabilities.clone(),
        dependencies: BTreeMap::new(),
    };
    let replaced = lock.packages.insert(manifest.name.clone(), entry);
    // A folder linked already, as of a `sleip dev` that did not end, is
    // none to give back.
    let replaced = replaced.filter(|locked| !matches!(locked.source, Source::Dev(_)));
    lock.write(&path)
        .map_err(|error| format!("{} cannot be written: {error}", path.display()))?;
    Ok((replaced, made))
}

/// Takes the entry of `name`, of the folder `folder`, out of the lockfile of
/// `paths`, under the store's lock, `replaced` given back if it was one;
/// an entry that another command changed since stays. A lockfile that the
/// link `made`, of no plugin left, goes, as none was there.
fn unlink(
    paths: &Paths,
    out: Out,
    (name, folder): (&str, &Path),
    replaced: Option<Locked>,
    made: bool,
) -> Result<(), String> {
    let _held = store_lock::lock(paths, out)?;
    let path = paths.lock();
    let Some(mut lock) = Lock::read(&path).map_err(|error| error.to_string())? else {
        return Ok(());
    };
    let ours = lock.packages.get(name).is_some_and(
        |locked| matches!(&locked.source, Source::Dev(linked) if Path::new(linked) == folder),
    );
    if !ours {
        return Ok(());
    }
    match replaced {
        Some(locked) => {
            lock.packages.insert(name.to_owned(), locked);
        }
        None => {
            lock.packages.remove(name);
        }
    }
    if made && lock.packages.is_empty() {
        return std::fs::remove_file(&path)
            .map_err(|error| format!("{} cannot be removed: {error}", path.display()));
    }
    lock.write(&path)
        .map_err(|error| format!("{} cannot be written: {error}", path.display()))
}

/// The files of the plugin's package in `folder`, `plugin.toml`, `src/`,
/// `tests/` and `locales/`, each its path, its size and the time of its
/// change, whose change a save makes.
fn fingerprint(folder: &Path) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
    let mut files = Vec::new();
    let mut left = vec![
        folder.join("plugin.toml"),
        folder.join("src"),
        folder.join("tests"),
        folder.join("locales"),
    ];
    while let Some(at) = left.pop() {
        let Ok(metadata) = std::fs::symlink_metadata(&at) else {
            continue;
        };
        if metadata.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&at) {
                left.extend(entries.flatten().map(|entry| entry.path()));
            }
        } else if metadata.is_file() {
            files.push((at, metadata.len(), metadata.modified().ok()));
        }
    }
    files.sort();
    files
}
