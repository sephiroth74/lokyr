//! `sleip remove <plugin>`: a plugin taken out of `plugins.toml`, in place,
//! its comments and order kept, and out of `plugins.lock`, written whole,
//! under the store's lock; refused of a plugin that another one needs, and
//! of `--locked`. Its package stays in the store until `sleip gc`.

use lokyr_pkg_model::lock::Lock;
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
    if let Some(listed) = listed {
        // What the edit leaves still reads, as the app reads it.
        lokyr_pkg_model::list::List::parse(&listed)
            .map_err(|error| format!("{} would not read: {error}", list_path.display()))?;
        lokyr_pkg_model::write_whole(&list_path, &listed)
            .map_err(|error| format!("{} cannot be written: {error}", list_path.display()))?;
    }
    if let Some(mut lock) = lock.filter(|_| locked) {
        lock.packages.remove(plugin);
        lock.write(&paths.lock())
            .map_err(|error| format!("{} cannot be written: {error}", paths.lock().display()))?;
    }
    out.tell(
        "removed",
        json!({ "name": plugin }),
        &format!("Removed {plugin}; its package stays in the store until `sleip gc`."),
    );
    Ok(())
}
