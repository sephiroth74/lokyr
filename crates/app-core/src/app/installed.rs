//! The plugins that Sleip installed, which the app adds as it starts: those
//! of `plugins.lock`, each of its package in the store, whose digest it
//! checks, or of the folder that `sleip dev` links, which it reads in
//! place; a plugin of Luau of its sources, on a thread of its own, which
//! activates at its manifest's trigger. A package changed since it was
//! stored, a manifest that does not read or that names another plugin, a
//! plugin of another plugin API than the app's or of an activation that the
//! app does not take yet, is refused, as a notice tells.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lokyr_kernel::commands::Code;
use lokyr_kernel::diagnostics::{About, Diagnostic};
use lokyr_kernel::l10n::Arg;
use lokyr_kernel::plugins::API_VERSION;
use lokyr_pkg_model::lock::{Lock, Source};
use lokyr_pkg_model::manifest::{Kind, Manifest};
use lokyr_pkg_model::store::Store;
use lokyr_platform_core::Platform;
use lokyr_plugin_host_luau::{Execution, LuauPlugin, Package};

use super::App;
use crate::drawing::Backend;

/// Where the installed plugins are: the lockfile, the store, and the
/// user's decisions on their capabilities, beside the lockfile.
#[derive(Clone, Debug)]
pub(super) struct Installed {
    pub(super) lock: PathBuf,
    pub(super) store: Store,
    pub(super) permissions: PathBuf,
}

/// A plugin of the lockfile that loaded: its manifest, and the plugin.
pub(super) struct Loaded {
    pub(super) manifest: Manifest,
    pub(super) plugin: LuauPlugin,
}

/// Why a plugin of the lockfile did not load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Refusal {
    /// Its plugin API is not the app's: the constraint of its manifest.
    Api(String),
    /// Something else, as its English text tells.
    Other(String),
}

/// The plugins of the lockfile that loaded, and those refused, each by its
/// name.
pub(super) type Found = (Vec<Loaded>, Vec<(String, Refusal)>);

/// The plugins of `installed`, each that loaded, and each refused by its
/// name; the error of a lockfile that does not read, which loads none.
pub(super) fn load(installed: &Installed) -> Result<Found, String> {
    let lock = match Lock::read(&installed.lock) {
        Ok(Some(lock)) => lock,
        Ok(None) => return Ok((Vec::new(), Vec::new())),
        Err(error) => return Err(error.to_string()),
    };
    let (mut loaded, mut refused) = (Vec::new(), Vec::new());
    for (name, locked) in &lock.packages {
        // A package of themes holds no plugin.
        if locked.kind != Kind::Luau {
            continue;
        }
        let folder = match (&locked.source, &locked.checksum) {
            (Source::Dev(folder), _) => PathBuf::from(folder),
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
            Ok(plugin) => loaded.push(plugin),
            Err(refusal) => refused.push((name.clone(), refusal)),
        }
    }
    Ok((loaded, refused))
}

/// The plugin `name` of the package in `folder`: its manifest, which names
/// it, of the app's plugin API, and its sources; or why not.
fn plugin(name: &str, folder: &Path) -> Result<Loaded, Refusal> {
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
    let plugin = LuauPlugin::new(name, Package::new(sources), Execution::Native);
    Ok(Loaded { manifest, plugin })
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
    /// of its own, to activate at its trigger; tells of each refused, and
    /// of a lockfile that does not read.
    pub(super) fn add_installed(&mut self, platform: &mut dyn Platform) {
        let Some(installed) = self.installed.clone() else {
            return;
        };
        let (loaded, refused) = match load(&installed) {
            Ok(found) => found,
            Err(error) => {
                let text = self
                    .texts
                    .text_with("plugins-lock-unread", &[("error", Arg::Text(&error))]);
                self.report(
                    platform,
                    Diagnostic::new(Code::Unavailable, text, About::Other),
                );
                return;
            }
        };
        // The user's decisions on their capabilities, whose changes the app
        // watches.
        self.decisions = self.permissions(platform);
        let path = installed.permissions.clone();
        let mut file = super::ConfigFile {
            path,
            watcher: None,
            changed: std::sync::Arc::default(),
            reading: None,
        };
        file.watch(platform);
        self.permissions_file = Some(file);
        let mut refused = refused;
        for Loaded { manifest, plugin } in loaded {
            let name = manifest.name.clone();
            let decisions = self
                .decisions
                .plugins
                .get(&name)
                .cloned()
                .unwrap_or_default();
            let added = self.plugins.add_installed(
                manifest,
                Box::new(plugin),
                (&decisions.granted, &decisions.refused),
            );
            match added {
                Ok(()) => self.installed_names.push(name),
                Err(error) => refused.push((name, Refusal::Other(error.message))),
            }
        }
        for (name, refusal) in refused {
            let text = match &refusal {
                Refusal::Api(api) => self.texts.text_with(
                    "plugin-other-api",
                    &[
                        ("name", Arg::Text(&name)),
                        ("api", Arg::Text(api)),
                        ("version", Arg::Text(API_VERSION)),
                    ],
                ),
                Refusal::Other(why) => self.texts.text_with(
                    "plugin-not-loaded",
                    &[("name", Arg::Text(&name)), ("why", Arg::Text(why))],
                ),
            };
            self.report(
                platform,
                Diagnostic::new(Code::Unavailable, text, About::Other),
            );
        }
    }
}
