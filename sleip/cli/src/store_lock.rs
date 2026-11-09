//! The store's lock, which a command that changes the store or the
//! lockfile holds, so that two Sleips never change them at once: the
//! second waits for the first, as it tells.

use std::fs::{File, OpenOptions, TryLockError};

use crate::output::Out;
use crate::paths::Paths;

/// The store's lock, held until it is dropped.
pub struct StoreLock {
    _file: File,
}

/// Takes the store's lock of `paths`, waiting for another Sleip that holds
/// it, as `out` tells; the error of a lock that cannot be taken.
pub fn lock(paths: &Paths, out: Out) -> Result<StoreLock, String> {
    let path = paths.store_lock();
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)
            .map_err(|error| format!("{} cannot be made: {error}", folder.display()))?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|error| format!("{} cannot be opened: {error}", path.display()))?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            out.waiting("the store's lock, which another Sleip holds");
            file.lock()
                .map_err(|error| format!("the store's lock cannot be taken: {error}"))?;
        }
        Err(TryLockError::Error(error)) => {
            return Err(format!("the store's lock cannot be taken: {error}"));
        }
    }
    Ok(StoreLock { _file: file })
}
