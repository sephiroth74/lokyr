//! Watching the config file: its directory rather than the file, as editors
//! save by writing another file and renaming it over the config; the file
//! that a symbolic link of it points to, as dotfiles managers make; and the
//! directory's parent, which tells when the directory goes, and, while it
//! does not exist, when it comes. FSEvents watches a directory with all
//! that it holds, so that the home directory, whose `Library` never rests,
//! is never watched.

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
    /// watch's thread, when the file, its link's target or its directory
    /// changes: an entry of a watched directory, by its directory and its
    /// name, as FSEvents tells the real paths, and the changes deeper in the
    /// directories that it watches count for nothing.
    pub fn new(path: &Path, changed: impl Fn() + Send + 'static) -> Result<Self, Error> {
        let error = |message: String| Error {
            path: path.to_owned(),
            position: None,
            message,
        };
        let (watched, names) = targets(path);
        // The entries that count, by their directory's real path.
        let entries: Vec<(PathBuf, OsString)> = watched
            .iter()
            .zip(&names)
            .map(|(directory, name)| {
                let real = fs::canonicalize(directory).unwrap_or_else(|_| directory.clone());
                (real, name.clone())
            })
            .collect();
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
                    entries.iter().any(|(directory, name)| {
                        path.parent() == Some(directory.as_path())
                            && path.file_name() == Some(name.as_os_str())
                    })
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

/// Makes the directory of the config at `path`, empty, when it does not
/// exist, so that a config written there later applies at once, as its
/// directory's watch tells it.
pub(crate) fn make_directory(path: &Path) -> std::io::Result<()> {
    match path.parent() {
        Some(directory) if !directory.exists() => fs::create_dir_all(directory),
        _ => Ok(()),
    }
}

/// The directories to watch for the config at `path`, and the names of
/// their entries that count: the file's directory and its name, with its
/// link's target's, and the directory's parent and its name; or, while
/// that directory does not exist, its parent alone. The home directory is
/// never one of them.
fn targets(path: &Path) -> (Vec<PathBuf>, Vec<OsString>) {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return (Vec::new(), Vec::new());
    };
    let home = std::env::home_dir();
    let watchable = |directory: &Path| directory.is_dir() && home.as_deref() != Some(directory);
    let parent = match (directory.parent(), directory.file_name()) {
        (Some(parent), Some(name)) if watchable(parent) => Some((parent, name)),
        _ => None,
    };
    let mut watched = Vec::new();
    let mut names = Vec::new();
    if watchable(directory) {
        watched.push(directory.to_owned());
        names.push(name.to_owned());
        let linked = fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink());
        if linked
            && let Ok(target) = fs::canonicalize(path)
            && let (Some(directory), Some(name)) = (target.parent(), target.file_name())
        {
            watched.push(directory.to_owned());
            names.push(name.to_owned());
        }
    }
    if let Some((parent, name)) = parent {
        watched.push(parent.to_owned());
        names.push(name.to_owned());
    }
    (watched, names)
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
    fn an_entry_of_the_same_name_elsewhere_counts_for_nothing() {
        // `config.toml` of another program, beside the config's directory.
        let parent = scratch("elsewhere");
        let path = parent.join("bolt").join("config.toml");
        make_directory(&path).unwrap();
        fs::write(&path, "").unwrap();
        fs::create_dir_all(parent.join("other")).unwrap();
        let (_watcher, receiver) = watch(&path);
        // FSEvents can tell the config's write of a moment before the watch
        // started: it goes first.
        while receiver.recv_timeout(Duration::from_millis(300)).is_ok() {}
        fs::write(parent.join("other").join("config.toml"), "").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(500)).is_err());
        fs::write(&path, "[font]\n").unwrap();
        assert!(changed(&receiver));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn a_directory_that_goes_calls_back() {
        let parent = scratch("gone");
        let path = parent.join("bolt").join("config.toml");
        make_directory(&path).unwrap();
        fs::write(&path, "").unwrap();
        let (watcher, receiver) = watch(&path);
        fs::remove_dir_all(parent.join("bolt")).unwrap();
        assert!(changed(&receiver));
        // The new watch is the parent's alone.
        assert!(watcher.is_stale(&path));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn the_directory_is_made_when_it_does_not_exist() {
        let parent = scratch("made");
        let path = parent.join("a").join("bolt").join("config.toml");
        make_directory(&path).unwrap();
        assert!(path.parent().unwrap().is_dir());
        assert!(!path.exists());
        // An existing one is left as it is.
        make_directory(&path).unwrap();
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn a_config_without_a_parent_cannot_be_watched() {
        let missing = scratch("missing").join("a").join("b").join("config.toml");
        let error = Watcher::new(&missing, || {}).unwrap_err();
        assert!(error.message.starts_with("it cannot be watched"), "{error}");
    }
}
