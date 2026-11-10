//! The plugins that Sleip installed, which the app adds as it starts: those
//! of `plugins.lock`, each of its package in the store, whose digest it
//! checks, or of the folder that `sleip dev` links, which it reads in
//! place; a plugin of Luau of its sources, on a thread of its own, which
//! activates at its manifest's trigger. A package changed since it was
//! stored, a manifest that does not read or that names another plugin, a
//! plugin of another plugin API than the app's or of an activation that the
//! app does not take yet, is refused, as a notice tells.
//!
//! The app watches the lockfile, the store and the folders that `sleip dev`
//! links, and, once a burst of changes settles, takes the lockfile again:
//! a plugin installed comes and activates at its trigger, one removed
//! goes, and one updated, or whose folder changed, is made anew, a new VM,
//! as `enabled` turns a plugin off and on again, without a restart.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use lokyr_kernel::commands::Code;
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::plugins::{API_VERSION, Plugins};
use lokyr_pkg_model::lock::{Lock, Locked, Source};
use lokyr_pkg_model::manifest::{Kind, Manifest};
use lokyr_pkg_model::store::Store;
use lokyr_platform_core::{Platform, TimerId};
use lokyr_plugin_host_luau::{Execution, LuauPlugin, Package};

use super::App;
use crate::drawing::Backend;
use crate::watch::{FolderWatch, Watcher};

/// How long the reload of the installed plugins waits after the last of a
/// burst of changes, as Sleip writes a package and the lockfile, or an
/// editor saves a plugin's files.
pub(super) const RELOAD_SETTLE: Duration = Duration::from_millis(250);

/// Where the installed plugins are: the lockfile, the store, and the
/// user's decisions on their capabilities, beside the lockfile.
#[derive(Clone, Debug)]
pub(super) struct Installed {
    pub(super) lock: PathBuf,
    pub(super) store: Store,
    pub(super) permissions: PathBuf,
}

/// A plugin of the lockfile that loaded: its manifest, its package, and
/// its lockfile's entry.
pub(super) struct Loaded {
    pub(super) manifest: Manifest,
    pub(super) package: Package,
    pub(super) locked: Locked,
}

/// Why a plugin of the lockfile did not load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Its plugin API is not the app's: the constraint of its manifest.
    Api(String),
    /// Something else, as its English text tells.
    Other(String),
}

/// The plugins of the lockfile: those that loaded, those refused, each by
/// its name, and the folders that `sleip dev` links, each of its plugin's
/// name.
#[derive(Default)]
pub(super) struct Found {
    pub(super) loaded: Vec<Loaded>,
    pub(super) refused: Vec<(String, Refusal)>,
    pub(super) dev: Vec<(String, PathBuf)>,
}

/// The plugins of `installed`; the error of a lockfile that does not read,
/// which loads none.
pub(super) fn load(installed: &Installed) -> Result<Found, String> {
    let lock = match Lock::read(&installed.lock) {
        Ok(Some(lock)) => lock,
        Ok(None) => return Ok(Found::default()),
        Err(error) => return Err(error.to_string()),
    };
    let mut found = Found::default();
    let Found {
        loaded,
        refused,
        dev,
    } = &mut found;
    for (name, locked) in &lock.packages {
        // A package of themes holds no plugin.
        if locked.kind != Kind::Luau {
            continue;
        }
        let folder = match (&locked.source, &locked.checksum) {
            (Source::Dev(folder), _) => {
                dev.push((name.clone(), PathBuf::from(folder)));
                PathBuf::from(folder)
            }
            (_, Some(digest)) => {
                if let Err(error) = installed.store.check(digest) {
                    refused.push((name.clone(), Refusal::Other(error.to_string())));
                    continue;
                }
                installed.store.folder(digest)
            }
            (_, None) => {
                refused.push((
                    name.clone(),
                    Refusal::Other("its package has no checksum".to_owned()),
                ));
                continue;
            }
        };
        match plugin(name, &folder) {
            Ok((manifest, package)) => loaded.push(Loaded {
                manifest,
                package,
                locked: locked.clone(),
            }),
            Err(refusal) => refused.push((name.clone(), refusal)),
        }
    }
    Ok(found)
}

/// The plugin `name` of the package in `folder`: its manifest, which names
/// it, of the app's plugin API, and its sources; or why not.
fn plugin(name: &str, folder: &Path) -> Result<(Manifest, Package), Refusal> {
    let other = Refusal::Other;
    let text = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| other(format!("its plugin.toml cannot be read: {error}")))?;
    let manifest = Manifest::parse(&text)
        .map_err(|error| other(format!("its plugin.toml does not read: {error}")))?;
    if manifest.name != name {
        return Err(other(format!(
            "its package holds the plugin `{}`",
            manifest.name
        )));
    }
    if !manifest.takes_api(API_VERSION) {
        return Err(Refusal::Api(manifest.api.clone().unwrap_or_default()));
    }
    let sources =
        sources(folder).map_err(|error| other(format!("its sources cannot be read: {error}")))?;
    Ok((manifest, Package::new(sources)))
}

/// What the app keeps of the installed plugins as they change: the watches
/// of the lockfile, the store and the folders that `sleip dev` links; the
/// plugins loaded and those refused; and the timer of the reload.
#[derive(Debug, Default)]
pub(super) struct Reloads {
    /// Whether something may have changed, which the watches' threads tell.
    pub(super) changed: Arc<AtomicBool>,
    lock: Option<Watcher>,
    store: Option<FolderWatch>,
    /// The watches of the folders that `sleip dev` links.
    pub(super) dev: Vec<DevWatch>,
    /// The plugins loaded, by their names, each of its lockfile's entry.
    pub(super) loaded: BTreeMap<String, Locked>,
    /// The plugins loaded of the folders that `sleip dev` links, by their
    /// names, each of its manifest and its sources, which a reload compares
    /// with the folder's, so that a change of no file of the package, as
    /// an editor's swap file, makes no plugin anew.
    dev_loaded: BTreeMap<String, (Manifest, Package)>,
    /// The plugins refused, by their names, each of why, told once.
    refused: BTreeMap<String, Refusal>,
    /// The timer of the reload, which waits for a burst of changes to
    /// settle.
    pub(super) timer: Option<TimerId>,
}

/// The watch of a folder that `sleip dev` links: its plugin and the
/// folder.
#[derive(Debug)]
pub(super) struct DevWatch {
    name: String,
    folder: PathBuf,
    _watch: FolderWatch,
}

/// What a watch calls: `flags`, set, and `platform`'s main thread woken.
fn on_change(platform: &dyn Platform, flags: Vec<Arc<AtomicBool>>) -> impl Fn() + Send + 'static {
    let waker = platform.waker();
    move || {
        for flag in &flags {
            flag.store(true, Ordering::Release);
        }
        waker.wake();
    }
}

/// The sources of the package in `folder`, `.luau` of its `src`, by their
/// paths in the package, of `/` between their folders.
fn sources(folder: &Path) -> std::io::Result<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    let mut left = vec![folder.join("src")];
    while let Some(at) = left.pop() {
        for entry in std::fs::read_dir(&at)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_dir() {
                left.push(path);
            } else if kind.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension == "luau")
            {
                let name = path
                    .strip_prefix(folder)
                    .ok()
                    .and_then(|inner| {
                        inner
                            .iter()
                            .map(|part| part.to_str())
                            .collect::<Option<Vec<_>>>()
                    })
                    .map(|parts| parts.join("/"))
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("{} is not of a UTF-8 name", path.display()),
                        )
                    })?;
                sources.insert(name, std::fs::read_to_string(&path)?);
            }
        }
    }
    Ok(sources)
}

impl<B: Backend> App<B> {
    /// Adds the installed plugins, as [`load`] finds them, each on a thread
    /// of its own, to activate at its trigger, of the user's decisions on
    /// their capabilities; watches the lockfile, the store, the folders
    /// that `sleip dev` links and the decisions; tells of each refused, and
    /// of a lockfile that does not read.
    pub(super) fn add_installed(&mut self, platform: &mut dyn Platform) {
        let Some(installed) = self.installed.clone() else {
            return;
        };
        // The config's folder, of the lockfile and the decisions, made
        // first, so that their watches start, as on a new account of none.
        if let Err(error) = crate::watch::make_directory(&installed.lock) {
            eprintln!(
                "lokyr: the folder of the installed plugins' lockfile cannot be made, for {}: {error}",
                installed.lock.display()
            );
        }
        // Of a file that does not read, none, which nothing writes over.
        self.decisions = self.permissions(platform).unwrap_or_default();
        let mut file = super::ConfigFile {
            path: installed.permissions.clone(),
            watcher: None,
            changed: Arc::default(),
            reading: None,
        };
        file.watch(platform);
        self.permissions_file = Some(file);
        self.watch_installed(platform, &installed);
        self.apply_installed(platform, &installed);
    }

    /// Takes the lockfile again, as a burst of changes of the installed
    /// plugins settled: each plugin installed comes, and activates at its
    /// trigger, the app's start's at once; each removed goes; each updated,
    /// or whose folder changed, is made anew, of a new VM.
    pub(super) fn reload_installed(&mut self, platform: &mut dyn Platform) {
        let Some(installed) = self.installed.clone() else {
            return;
        };
        // The decisions' own changes that their watch has not told yet,
        // applied to the plugins that stay, so that none is lost.
        self.watch_permissions(platform);
        self.permissions_changed(platform);
        self.watch_installed(platform, &installed);
        self.apply_installed(platform, &installed);
        self.configure_plugins(platform);
        for error in self.with_plugins(platform, Plugins::activate) {
            eprintln!("lokyr: {error}");
        }
        self.after_plugins(platform);
    }

    /// Watches `permissions.toml` anew, as its watch is none or stale: its
    /// folder came into being or went, or its link points elsewhere, as
    /// GNU Stow makes it.
    pub(super) fn watch_permissions(&mut self, platform: &dyn Platform) {
        if let Some(file) = &mut self.permissions_file
            && file
                .watcher
                .as_ref()
                .is_none_or(|watcher| watcher.is_stale(&file.path))
        {
            file.watch(platform);
        }
    }

    /// Watches the lockfile and the store, unless it watches them already:
    /// the store once it is there, as Sleip makes it.
    fn watch_installed(&mut self, platform: &dyn Platform, installed: &Installed) {
        let reloads = &mut self.reloads;
        let stale = reloads
            .lock
            .as_ref()
            .is_none_or(|watcher| watcher.is_stale(&installed.lock));
        if stale {
            let changed = on_change(platform, vec![Arc::clone(&reloads.changed)]);
            reloads.lock = Watcher::new(&installed.lock, changed)
                .inspect_err(|error| eprintln!("lokyr: {error}"))
                .ok();
        }
        if reloads.store.is_none() && installed.store.root().is_dir() {
            let changed = on_change(platform, vec![Arc::clone(&reloads.changed)]);
            reloads.store = FolderWatch::new(installed.store.root(), changed)
                .inspect_err(|error| eprintln!("lokyr: {error}"))
                .ok();
        }
    }

    /// Makes the plugins of the app those of the lockfile, as [`load`]
    /// finds them: the plugins gone or changed, of their entries or of
    /// their folders' changes, taken away; the new and the changed added;
    /// each refused told, once while it stays so; the folders that
    /// `sleip dev` links watched.
    fn apply_installed(&mut self, platform: &mut dyn Platform, installed: &Installed) {
        let found = match load(installed) {
            Ok(found) => found,
            Err(error) => {
                let text = self
                    .texts
                    .text_with("plugins-lock-unread", &[("error", Arg::Text(&error))]);
                // One notice of it, which the next one replaces, however
                // many reloads tell it.
                self.report(
                    platform,
                    Diagnostic::new(Code::Unavailable, text, About::InstalledPlugins),
                );
                return;
            }
        };
        self.resolve(platform, &About::InstalledPlugins);
        // A plugin of the lockfile's same entry stays; one of a folder
        // that `sleip dev` links, of the same manifest and sources too.
        let (kept, fresh): (Vec<Loaded>, Vec<Loaded>) =
            found.loaded.into_iter().partition(|loaded| {
                let name = &loaded.manifest.name;
                self.reloads.loaded.get(name) == Some(&loaded.locked)
                    && (!matches!(loaded.locked.source, Source::Dev(_))
                        || self
                            .reloads
                            .dev_loaded
                            .get(name)
                            .is_some_and(|(manifest, package)| {
                                *manifest == loaded.manifest && *package == loaded.package
                            }))
            });
        let kept: BTreeSet<String> = kept
            .into_iter()
            .map(|loaded| loaded.manifest.name)
            .collect();
        let gone: Vec<String> = self
            .reloads
            .loaded
            .keys()
            .filter(|name| !kept.contains(*name))
            .cloned()
            .collect();
        for name in gone {
            self.with_plugins(platform, |plugins, context| plugins.remove(&name, context));
            self.forget_consent(platform, &name);
            self.reloads.loaded.remove(&name);
            self.reloads.dev_loaded.remove(&name);
        }
        let mut refused = found.refused;
        for Loaded {
            manifest,
            package,
            locked,
        } in fresh
        {
            let name = manifest.name.clone();
            let decisions = self
                .decisions
                .plugins
                .get(&name)
                .cloned()
                .unwrap_or_default();
            let dev = matches!(locked.source, Source::Dev(_))
                .then(|| (manifest.clone(), package.clone()));
            // A fresh VM of the package each time, as a fault's while
            // passes or the plugin is turned on again.
            let made = name.clone();
            let added = self.plugins.add_installed(
                manifest,
                Box::new(move || {
                    Box::new(LuauPlugin::new(
                        made.clone(),
                        package.clone(),
                        Execution::Native,
                    ))
                }),
                (&decisions.granted, &decisions.refused),
            );
            match added {
                Ok(()) => {
                    if let Some(dev) = dev {
                        self.reloads.dev_loaded.insert(name.clone(), dev);
                    }
                    self.reloads.loaded.insert(name, locked);
                }
                Err(error) => refused.push((name, Refusal::Other(error.message))),
            }
        }
        let now: BTreeMap<String, Refusal> = refused.into_iter().collect();
        for (name, refusal) in &now {
            if self.reloads.refused.get(name) == Some(refusal) {
                continue;
            }
            let text = match refusal {
                Refusal::Api(api) => self.texts.text_with(
                    "plugin-other-api",
                    &[
                        ("name", Arg::Text(name)),
                        ("api", Arg::Text(api)),
                        ("version", Arg::Text(API_VERSION)),
                    ],
                ),
                Refusal::Other(why) => self.texts.text_with(
                    "plugin-not-loaded",
                    &[("name", Arg::Text(name)), ("why", Arg::Text(why))],
                ),
            };
            self.report(
                platform,
                Diagnostic::new(Code::Unavailable, text, About::Other),
            );
        }
        self.reloads.refused = now;
        self.watch_folders(platform, found.dev);
    }

    /// Watches the folders of `dev`, each of its plugin, that `sleip dev`
    /// links, and those alone, keeping the watches that it has.
    fn watch_folders(&mut self, platform: &dyn Platform, dev: Vec<(String, PathBuf)>) {
        let reloads = &mut self.reloads;
        reloads.dev.retain(|watch| {
            dev.iter()
                .any(|(name, folder)| *name == watch.name && *folder == watch.folder)
        });
        for (name, folder) in dev {
            if reloads.dev.iter().any(|watch| watch.name == name) {
                continue;
            }
            let flags = vec![Arc::clone(&reloads.changed)];
            match FolderWatch::new(&folder, on_change(platform, flags)) {
                Ok(watch) => reloads.dev.push(DevWatch {
                    name,
                    folder,
                    _watch: watch,
                }),
                Err(error) => eprintln!("lokyr: {error}"),
            }
        }
    }
}
