//! Watching the config file: its directory rather than the file, as editors
//! save by writing another file and renaming it over the config; the file
//! that a symbolic link of it points to, as dotfiles managers make; and,
//! while its directory does not exist, the directory's parent.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use bolt_kernel::config::Error;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

/// A watch of the config file, which calls back, from a thread of its own,
/// when the file may have changed.
pub struct Watcher {
    _watcher: RecommendedWatcher,
    /// The directories watched.
    watched: Vec<PathBuf>,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("watched", &self.watched)
            .finish_non_exhaustive()
    }
}

impl Watcher {
    /// Watches the config file at `path`, and calls `changed`, from the
    /// watch's thread, when an entry of a watched directory that names the
    /// file, its link's target or its directory changes.
    pub fn new(path: &Path, changed: impl Fn() + Send + 'static) -> Result<Self, Error> {
        let error = |message: String| Error {
            path: path.to_owned(),
            position: None,
            message,
        };
        let (watched, names) = targets(path);
        if watched.is_empty() {
            return Err(error(
                "it cannot be watched, as neither its directory nor that one's parent exists"
                    .to_owned(),
            ));
        }
        let mut watcher =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else {
                    return;
                };
                let named = event.paths.iter().any(|path| {
                    path.file_name()
                        .is_some_and(|name| names.iter().any(|known| known == name))
                });
                if named && !matches!(event.kind, EventKind::Access(_)) {
                    changed();
                }
            })
            .map_err(|e| error(format!("it cannot be watched: {e}")))?;
        for directory in &watched {
            watcher
                .watch(directory, RecursiveMode::NonRecursive)
                .map_err(|e| error(format!("it cannot be watched: {e}")))?;
        }
        Ok(Self {
            _watcher: watcher,
            watched,
        })
    }

    /// Whether the config at `path` needs another watch than this one: its
    /// directory came into being, or its link points elsewhere.
    pub fn is_stale(&self, path: &Path) -> bool {
        targets(path).0 != self.watched
    }
}

/// The directories to watch for the config at `path`, and the names of
/// their entries that count: the file's directory and its name, with its
/// link's target's; or, while that directory does not exist, its parent
/// and the directory's name.
fn targets(path: &Path) -> (Vec<PathBuf>, Vec<OsString>) {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return (Vec::new(), Vec::new());
    };
    if directory.is_dir() {
        let mut watched = vec![directory.to_owned()];
        let mut names = vec![name.to_owned()];
        let linked = fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink());
        if linked
            && let Ok(target) = fs::canonicalize(path)
            && let (Some(directory), Some(name)) = (target.parent(), target.file_name())
        {
            watched.push(directory.to_owned());
            names.push(name.to_owned());
        }
        return (watched, names);
    }
    match (directory.parent(), directory.file_name()) {
        (Some(parent), Some(name)) if parent.is_dir() => {
            (vec![parent.to_owned()], vec![name.to_owned()])
        }
        _ => (Vec::new(), Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{self, Receiver};
    use std::time::Duration;

    use super::*;

    /// How long a change may take to come, on a busy machine.
    const COMES: Duration = Duration::from_secs(5);

    /// A directory of its own for `test`, empty.
    fn scratch(test: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("bolt-watch-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        // FSEvents tells the real paths, without the links of `/var`.
        fs::canonicalize(directory).unwrap()
    }

    fn watch(path: &Path) -> (Watcher, Receiver<()>) {
        let (sender, receiver) = mpsc::channel();
        let watcher = Watcher::new(path, move || {
            let _ = sender.send(());
        })
        .unwrap();
        (watcher, receiver)
    }

    /// Waits for a change, then for those that come with it.
    fn changed(receiver: &Receiver<()>) -> bool {
        let came = receiver.recv_timeout(COMES).is_ok();
        while receiver.recv_timeout(Duration::from_millis(300)).is_ok() {}
        came
    }

    #[test]
    fn a_save_by_renaming_and_a_write_call_back() {
        let directory = scratch("save");
        let path = directory.join("config.toml");
        fs::write(&path, "[font]\n").unwrap();
        let (watcher, receiver) = watch(&path);
        assert!(!watcher.is_stale(&path));
        // As vim saves: another file, renamed over the config.
        let swap = directory.join(".config.toml.swp");
        fs::write(&swap, "[font]\nsize = 15\n").unwrap();
        fs::rename(&swap, &path).unwrap();
        assert!(changed(&receiver));
        fs::write(&path, "[font]\nsize = 16\n").unwrap();
        assert!(changed(&receiver));
        // Another file of the directory does not count.
        fs::write(directory.join("other.toml"), "").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(500)).is_err());
        drop(watcher);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn a_directory_that_comes_into_being_calls_back() {
        let parent = scratch("directory");
        let path = parent.join("bolt").join("config.toml");
        let (watcher, receiver) = watch(&path);
        fs::create_dir(parent.join("bolt")).unwrap();
        assert!(changed(&receiver));
        // The new watch is the directory's.
        assert!(watcher.is_stale(&path));
        let (watcher, receiver) = watch(&path);
        assert!(!watcher.is_stale(&path));
        fs::write(&path, "").unwrap();
        assert!(changed(&receiver));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn a_link_s_target_is_watched_too() {
        let home = scratch("link");
        let dotfiles = home.join("dotfiles");
        let config = home.join("config");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(&config).unwrap();
        let target = dotfiles.join("bolt.toml");
        fs::write(&target, "").unwrap();
        let path = config.join("config.toml");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let (_watcher, receiver) = watch(&path);
        fs::write(&target, "[cursor]\nshape = \"bar\"\n").unwrap();
        assert!(changed(&receiver));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_config_without_a_parent_cannot_be_watched() {
        let missing = scratch("missing").join("a").join("b").join("config.toml");
        let error = Watcher::new(&missing, || {}).unwrap_err();
        assert!(error.message.starts_with("it cannot be watched"), "{error}");
    }
}
