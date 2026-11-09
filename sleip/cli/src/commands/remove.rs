//! `sleip remove <plugin>`: a plugin taken out of `plugins.toml`, in place,
//! its comments and order kept, and out of `plugins.lock`, written whole,
//! with the plugins that it needed and that no other plugin needs, under
//! the store's lock; refused of a plugin that another one needs, and of
//! `--locked`. Their packages stay in the store until `sleip gc`.

use std::collections::BTreeSet;

use lokyr_pkg_model::lock::{Lock, Source};
use serde_json::json;

use crate::args::Options;
use crate::output::Out;
use crate::paths::Paths;
use crate::store_lock;

/// Removes `plugin` of the installation of `paths`, as `options` and `out`
/// say.
pub fn remove(paths: &Paths, options: Options, out: Out, plugin: &str) -> Result<(), String> {
    let _held = store_lock::lock(paths, out)?;
    let lock = Lock::read(&paths.lock()).map_err(|error| error.to_string())?;
    let list_path = paths.list();
    let text = match std::fs::read_to_string(&list_path) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(format!("{} cannot be read: {error}", list_path.display())),
    };
    let mut listed = None;
    if let Some(text) = &text {
        let mut document: toml_edit::DocumentMut = text
            .parse()
            .map_err(|error| format!("{}: {error}", list_path.display()))?;
        let removed = document
            .get_mut("plugins")
            .and_then(toml_edit::Item::as_table_like_mut)
            .and_then(|plugins| plugins.remove(plugin))
            .is_some();
        if removed {
            listed = Some(document.to_string());
        }
    }
    let locked = lock
        .as_ref()
        .is_some_and(|lock| lock.packages.contains_key(plugin));
    if listed.is_none() && !locked {
        return Err(format!("`{plugin}` is not installed"));
    }
    if let Some(lock) = &lock
        && let Some((needer, _)) = lock
            .packages
            .iter()
            .find(|(_, locked)| locked.dependencies.contains_key(plugin))
    {
        return Err(format!(
            "`{plugin}` is needed by `{needer}`, which goes first"
        ));
    }
    if options.locked && locked {
        return Err(format!(
            "removing `{plugin}` changes plugins.lock, which `--locked` keeps"
        ));
    }
    // The plugins that the user asked for, as the edit leaves them.
    let wanted = match listed.as_deref().or(text.as_deref()) {
        // What the edit leaves still reads, as the app reads it.
        Some(text) => Some(
            lokyr_pkg_model::list::List::parse(text)
                .map_err(|error| format!("{} would not read: {error}", list_path.display()))?,
        ),
        None => None,
    };
    if let Some(listed) = &listed {
        lokyr_pkg_model::write_whole(&list_path, listed)
            .map_err(|error| format!("{} cannot be written: {error}", list_path.display()))?;
    }
    let mut needless = Vec::new();
    if let Some(mut lock) = lock.filter(|_| locked) {
        let needed = needs(&lock, plugin);
        lock.packages.remove(plugin);
        if let Some(wanted) = &wanted {
            needless = unreached(&mut lock, wanted, &needed);
        }
        lock.write(&paths.lock())
            .map_err(|error| format!("{} cannot be written: {error}", paths.lock().display()))?;
    }
    out.tell(
        "removed",
        json!({ "name": plugin }),
        &format!("Removed {plugin}; its package stays in the store until `sleip gc`."),
    );
    for name in needless {
        out.tell(
            "removed",
            json!({ "name": name, "needed": false }),
            &format!("Removed {name}, which no plugin needs now."),
        );
    }
    Ok(())
}

/// The plugins of `lock` that `from` needs, and those that they need, by
/// their names, `from` left out.
fn needs(lock: &Lock, from: &str) -> BTreeSet<String> {
    reached(
        lock,
        lock.packages
            .get(from)
            .map(|locked| locked.dependencies.keys().cloned().collect())
            .unwrap_or_default(),
    )
}

/// The plugins of `lock` of `roots` and those that they need, by their
/// names.
fn reached(lock: &Lock, mut left: Vec<String>) -> BTreeSet<String> {
    let mut reached = BTreeSet::new();
    while let Some(name) = left.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        if let Some(locked) = lock.packages.get(&name) {
            left.extend(locked.dependencies.keys().cloned());
        }
    }
    reached
}

/// Takes out of `lock` the plugins of `needed`, a removed plugin's, that
/// none of `wanted`, the user's, nor one of a folder that `sleip dev`
/// links, needs: their names. A plugin that the lockfile holds of none of
/// these stays, as the user's own edit.
fn unreached(
    lock: &mut Lock,
    wanted: &lokyr_pkg_model::list::List,
    needed: &BTreeSet<String>,
) -> Vec<String> {
    let roots = lock
        .packages
        .iter()
        .filter(|(name, locked)| {
            wanted.plugins.contains_key(*name) || matches!(locked.source, Source::Dev(_))
        })
        .map(|(name, _)| name.clone())
        .collect();
    let kept = reached(lock, roots);
    let needless: Vec<String> = needed
        .iter()
        .filter(|name| !kept.contains(*name) && lock.packages.contains_key(*name))
        .cloned()
        .collect();
    for name in &needless {
        lock.packages.remove(name);
    }
    needless
}
