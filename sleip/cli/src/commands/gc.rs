//! `sleip gc`: the packages of the store that the lockfile does not name
//! removed, and what an install that stopped left there, under the store's
//! lock; refused of a lockfile that does not read, whose packages it could
//! not tell.

use std::collections::BTreeSet;

use lokyr_pkg_model::lock::Lock;
use lokyr_pkg_model::store::Digest;
use serde_json::json;

use super::{bytes_text, size_of};
use crate::output::Out;
use crate::paths::Paths;
use crate::store_lock;

/// Collects the store of `paths`, as `out` tells.
pub fn gc(paths: &Paths, out: Out) -> Result<(), String> {
    let _held = store_lock::lock(paths, out)?;
    let lock = Lock::read(&paths.lock()).map_err(|error| error.to_string())?;
    let named: BTreeSet<Digest> = lock
        .iter()
        .flat_map(|lock| lock.packages.values())
        .filter_map(|locked| locked.checksum)
        .collect();
    let store = &paths.store;
    let (mut collected, mut freed) = (0_u64, 0_u64);
    let digests = store
        .digests()
        .map_err(|error| format!("{} cannot be read: {error}", store.root().display()))?;
    for digest in digests.into_iter().filter(|digest| !named.contains(digest)) {
        let bytes = size_of(&store.folder(&digest));
        store
            .remove(&digest)
            .map_err(|error| format!("{digest} cannot be removed: {error}"))?;
        collected += 1;
        freed += bytes;
        out.tell(
            "collected",
            json!({ "digest": digest.to_string(), "bytes": bytes }),
            &format!("Removed {digest}, {}", bytes_text(bytes)),
        );
    }
    // What an install that stopped left, as no other Sleip unpacks now.
    if let Ok(entries) = std::fs::read_dir(store.root()) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(".unpack-") && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                let bytes = size_of(&entry.path());
                std::fs::remove_dir_all(entry.path())
                    .map_err(|error| format!("{name} cannot be removed: {error}"))?;
                collected += 1;
                freed += bytes;
                out.tell(
                    "collected",
                    json!({ "leftover": name, "bytes": bytes }),
                    &format!("Removed {name}, left by an install that stopped"),
                );
            }
        }
    }
    let summary = if collected == 0 {
        "Nothing to remove.".to_owned()
    } else {
        format!("Removed {collected}, {}.", bytes_text(freed))
    };
    out.tell(
        "done",
        json!({ "collected": collected, "bytes": freed }),
        &summary,
    );
    Ok(())
}
