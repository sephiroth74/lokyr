//! Watching the config file: its directory rather than the file, as editors
//! save by writing another file and renaming it over the config; the file
//! that a symbolic link of it points to, as dotfiles managers make, while it
//! exists and after it goes, as a checkout of the dotfiles removes it and
//! writes it again; and the directory's parent, which tells when the
//! directory goes, and, while it does not exist, when it comes. FSEvents
//! watches a directory with all that it holds, so that the home directory,
//! whose `Library` never rests, is never watched.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use lokyr_kernel::config::Error;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

/// A watch of the config file, which calls back, from a thread of its own,
/// when the file may have changed.
pub struct Watcher {
    _watcher: RecommendedWatcher,
    /// What the watch looks at.
    targets: Targets,
}

impl std::fmt::Debug for Watcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watcher")
            .field("targets", &self.targets)
            .finish_non_exhaustive()
    }
}

/// The directories that a watch of the config looks at, and the entries in
/// them that count, each by its directory's path and its name: the path as
/// watched, which inotify tells, and, where a link stands for it, its real
/// one, which FSEvents tells.
#[derive(Debug, PartialEq, Eq)]
struct Targets {
    watched: Vec<PathBuf>,
    entries: Vec<(PathBuf, OsString)>,
}

impl Watcher {
    /// Watches the config file at `path`, and calls `changed`, from the
    /// watch's thread, when the file, its link's target or its directory
    /// changes: an entry of a watched directory, by its directory and its
    /// name, as watched or by its real path, or a watched directory that
    /// goes; the changes deeper in the directories that it watches count
    /// for nothing.
    pub fn new(path: &Path, changed: impl Fn() + Send + 'static) -> Result<Self, Error> {
        let error = |message: String| Error {
            path: path.to_owned(),
            position: None,
            message,
        };
        let targets = targets(path);
        let entries = targets.entries.clone();
        if targets.watched.is_empty() {
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
                let removed = matches!(event.kind, EventKind::Remove(_));
                let named = event.paths.iter().any(|path| {
                    entries.iter().any(|(directory, name)| {
                        (path.parent() == Some(directory.as_path())
                            && path.file_name() == Some(name.as_os_str()))
                            || (removed && path == directory)
                    })
                });
                if named && !matches!(event.kind, EventKind::Access(_)) {
                    changed();
                }
            })
            .map_err(|e| error(format!("it cannot be watched: {e}")))?;
        for directory in &targets.watched {
            watcher
                .watch(directory, RecursiveMode::NonRecursive)
                .map_err(|e| error(format!("it cannot be watched: {e}")))?;
        }
        Ok(Self {
            _watcher: watcher,
            targets,
        })
    }

    /// Whether the config at `path` needs another watch than this one: its
    /// directory came into being or went, or its link, or a link of its
    /// directory, points elsewhere, even to another file of the same
    /// directory.
    pub fn is_stale(&self, path: &Path) -> bool {
        targets(path) != self.targets
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

/// The directories to watch for the config at `path`, and the entries of
/// them that count: the file's directory and its name, with its link's
/// target's, or the parent of the target's directory and its name while
/// that does not exist; and the directory's parent and its name; or, while
/// that directory does not exist, its parent alone. The home directory is
/// never one of them.
fn targets(path: &Path) -> Targets {
    let (Some(directory), Some(name)) = (path.parent(), path.file_name()) else {
        return Targets {
            watched: Vec::new(),
            entries: Vec::new(),
        };
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
        if let Some(target) = link_target(path)
            && let (Some(directory), Some(name)) = (target.parent(), target.file_name())
        {
            if watchable(directory) {
                watched.push(directory.to_owned());
                names.push(name.to_owned());
            } else if let (Some(parent), Some(name)) = (directory.parent(), directory.file_name())
                && watchable(parent)
            {
                watched.push(parent.to_owned());
                names.push(name.to_owned());
            }
        }
    }
    if let Some((parent, name)) = parent {
        watched.push(parent.to_owned());
        names.push(name.to_owned());
    }
    // A directory that a link stands for, as GNU Stow folds one, by its
    // target too.
    let mut entries = Vec::new();
    for (directory, name) in watched.iter().zip(names) {
        let real = fs::canonicalize(directory).unwrap_or_else(|_| directory.clone());
        if real != *directory {
            entries.push((directory.clone(), name.clone()));
        }
        entries.push((real, name));
    }
    Targets { watched, entries }
}

/// The file that the link at `path` points to, if `path` is a link, whether
/// the file exists or not: by its real path, or by its directory's and its
/// name when it went.
fn link_target(path: &Path) -> Option<PathBuf> {
    if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_symlink()) {
        return None;
    }
    if let Ok(target) = fs::canonicalize(path) {
        return Some(target);
    }
    // Where the link points, from its own directory.
    let target = path.parent()?.join(fs::read_link(path).ok()?);
    let (directory, name) = (target.parent()?, target.file_name()?);
    Some(fs::canonicalize(directory).map_or_else(|_| target.clone(), |real| real.join(name)))
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
            std::env::temp_dir().join(format!("lokyr-watch-{test}-{}", std::process::id()));
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
        let path = parent.join("lokyr").join("config.toml");
        let (watcher, receiver) = watch(&path);
        fs::create_dir(parent.join("lokyr")).unwrap();
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
        let target = dotfiles.join("lokyr.toml");
        fs::write(&target, "").unwrap();
        let path = config.join("config.toml");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        let (_watcher, receiver) = watch(&path);
        fs::write(&target, "[cursor]\nshape = \"bar\"\n").unwrap();
        assert!(changed(&receiver));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_link_s_target_that_goes_and_comes_back_is_watched_still() {
        let home = scratch("checkout");
        let dotfiles = home.join("dotfiles");
        let config = home.join("config");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(&config).unwrap();
        let target = dotfiles.join("lokyr.toml");
        fs::write(&target, "").unwrap();
        let path = config.join("config.toml");
        // A link relative to its directory, as GNU Stow makes them.
        std::os::unix::fs::symlink("../dotfiles/lokyr.toml", &path).unwrap();
        let (watcher, receiver) = watch(&path);
        // As a checkout removes the file and writes it again.
        fs::remove_file(&target).unwrap();
        assert!(changed(&receiver));
        assert!(
            !watcher.is_stale(&path),
            "the target's directory stays watched"
        );
        fs::write(&target, "[cursor]\nshape = \"bar\"\n").unwrap();
        assert!(changed(&receiver));
        // Its directory too: its parent tells when it comes back.
        fs::remove_dir_all(&dotfiles).unwrap();
        assert!(changed(&receiver));
        assert!(watcher.is_stale(&path));
        drop(watcher);
        let (watcher, receiver) = watch(&path);
        fs::create_dir_all(&dotfiles).unwrap();
        assert!(changed(&receiver));
        assert!(watcher.is_stale(&path));
        drop(watcher);
        let (_watcher, receiver) = watch(&path);
        fs::write(&target, "").unwrap();
        assert!(changed(&receiver));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_link_pointed_elsewhere_is_watched_at_its_new_target() {
        let home = scratch("repointed");
        let dotfiles = home.join("dotfiles");
        let config = home.join("config");
        fs::create_dir_all(&dotfiles).unwrap();
        fs::create_dir_all(&config).unwrap();
        let (first, second) = (dotfiles.join("first.toml"), dotfiles.join("second.toml"));
        fs::write(&first, "").unwrap();
        fs::write(&second, "").unwrap();
        let path = config.join("config.toml");
        std::os::unix::fs::symlink(&first, &path).unwrap();
        let (watcher, receiver) = watch(&path);
        // As `ln -sf` points it to another file of the same directory.
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&second, &path).unwrap();
        assert!(changed(&receiver));
        assert!(watcher.is_stale(&path));
        drop(watcher);
        let (watcher, receiver) = watch(&path);
        assert!(!watcher.is_stale(&path));
        fs::write(&second, "[cursor]\nshape = \"bar\"\n").unwrap();
        assert!(changed(&receiver));
        drop(watcher);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_directory_link_pointed_elsewhere_is_watched_at_its_new_target() {
        // GNU Stow's packages, of which the config's directory is a link to
        // one.
        let home = scratch("folded");
        let packages = [
            home.join("one").join("lokyr"),
            home.join("two").join("lokyr"),
        ];
        for package in &packages {
            fs::create_dir_all(package).unwrap();
            fs::write(package.join("config.toml"), "").unwrap();
        }
        let config = home.join("config");
        fs::create_dir_all(&config).unwrap();
        let directory = config.join("lokyr");
        std::os::unix::fs::symlink(&packages[0], &directory).unwrap();
        let path = directory.join("config.toml");
        let (watcher, receiver) = watch(&path);
        fs::write(packages[0].join("config.toml"), "[font]\n").unwrap();
        assert!(changed(&receiver));
        assert!(!watcher.is_stale(&path));
        fs::remove_file(&directory).unwrap();
        std::os::unix::fs::symlink(&packages[1], &directory).unwrap();
        assert!(changed(&receiver));
        assert!(watcher.is_stale(&path));
        drop(watcher);
        let (watcher, receiver) = watch(&path);
        fs::write(packages[1].join("config.toml"), "[font]\n").unwrap();
        assert!(changed(&receiver));
        drop(watcher);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_directory_link_s_entries_count_by_its_path_and_its_target_s() {
        // inotify tells an event by the path watched, FSEvents by the real
        // one.
        let home = scratch("entries");
        let package = home.join("one").join("lokyr");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(home.join("config")).unwrap();
        let directory = home.join("config").join("lokyr");
        std::os::unix::fs::symlink(&package, &directory).unwrap();
        let entries = targets(&directory.join("config.toml")).entries;
        let name = OsString::from("config.toml");
        assert!(
            entries.contains(&(directory.clone(), name.clone())),
            "{entries:?}"
        );
        let real = fs::canonicalize(&package).unwrap();
        assert!(entries.contains(&(real, name)), "{entries:?}");
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn an_entry_of_the_same_name_elsewhere_counts_for_nothing() {
        // `config.toml` of another program, in the directory above the
        // config's, which the watch looks at for the config's directory.
        let parent = scratch("elsewhere");
        let path = parent.join("lokyr").join("config.toml");
        make_directory(&path).unwrap();
        fs::write(&path, "").unwrap();
        let (_watcher, receiver) = watch(&path);
        // FSEvents can tell the config's write of a moment before the watch
        // started: it goes first.
        while receiver.recv_timeout(Duration::from_millis(300)).is_ok() {}
        fs::write(parent.join("config.toml"), "").unwrap();
        assert!(receiver.recv_timeout(Duration::from_millis(500)).is_err());
        fs::write(&path, "[font]\n").unwrap();
        assert!(changed(&receiver));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn a_directory_that_goes_calls_back() {
        let parent = scratch("gone");
        let path = parent.join("lokyr").join("config.toml");
        make_directory(&path).unwrap();
        fs::write(&path, "").unwrap();
        let (watcher, receiver) = watch(&path);
        fs::remove_dir_all(parent.join("lokyr")).unwrap();
        assert!(changed(&receiver));
        // The new watch is the parent's alone.
        assert!(watcher.is_stale(&path));
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn the_directory_is_made_when_it_does_not_exist() {
        let parent = scratch("made");
        let path = parent.join("a").join("lokyr").join("config.toml");
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
