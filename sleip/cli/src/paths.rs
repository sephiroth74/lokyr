//! Where Sleip finds what it handles, the app's own places: the config's
//! folder, of `plugins.toml`, `plugins.lock` and `permissions.toml`; the
//! store of the packages; Sleip's cache; and the app, whose plugin API the
//! plugins are resolved against.

use std::path::{Path, PathBuf};

use lokyr_pkg_model::store::Store;

/// The places of an installation.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The config's folder, the app's.
    pub config: PathBuf,
    /// The store of the packages.
    pub store: Store,
    /// Sleip's cache, of the registry's index.
    pub cache: PathBuf,
}

impl Paths {
    /// The places of the user of the home folder that the environment
    /// tells, of the base directories of XDG where they apply; the error
    /// of no home folder.
    pub fn of_environment() -> Result<Self, String> {
        let home = std::env::home_dir()
            .filter(|home| home.is_absolute())
            .ok_or_else(|| "no home folder, as HOME tells none".to_owned())?;
        let absolute = |name: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let config = absolute("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home.join(".config"))
            .join("lokyr");
        let store = Store::of_user(&home, absolute("XDG_DATA_HOME").as_deref());
        let cache = if cfg!(target_os = "macos") {
            home.join("Library/Caches")
        } else {
            absolute("XDG_CACHE_HOME").unwrap_or_else(|| home.join(".cache"))
        }
        .join("lokyr/sleip");
        Ok(Self {
            config,
            store,
            cache,
        })
    }

    /// The user's list of plugins.
    pub fn list(&self) -> PathBuf {
        self.config.join(lokyr_pkg_model::list::FILE)
    }

    /// The lockfile.
    pub fn lock(&self) -> PathBuf {
        self.config.join(lokyr_pkg_model::lock::FILE)
    }

    /// The decisions on the plugins' capabilities.
    pub fn permissions(&self) -> PathBuf {
        self.config.join(lokyr_pkg_model::permissions::FILE)
    }

    /// The file of the store's lock, beside the store rather than in it, so
    /// that the app's watch of the store does not take a lock for a change.
    pub fn store_lock(&self) -> PathBuf {
        let root = self.store.root();
        root.with_file_name(format!(
            "{}.lock",
            root.file_name()
                .map_or("store".into(), |name| name.to_string_lossy())
        ))
    }
}

/// The app's plugin API's version, and where it was read: the app that
/// `SLEIP_LOKYR` names, a bundle, its resources' `api-version` file or its
/// binary; the bundle that Sleip's binary is in, through the links to it,
/// as one in a folder of the `PATH`; the app in the Applications folders;
/// or `lokyr --api-version` of the `PATH`.
pub fn app_api() -> Option<(String, String)> {
    let mut places: Vec<PathBuf> = Vec::new();
    if let Some(named) = std::env::var_os("SLEIP_LOKYR") {
        places.push(PathBuf::from(named));
    }
    // The system tells the path that the binary was started of, a link's.
    if let Ok(executable) = std::env::current_exe().and_then(std::fs::canonicalize)
        && let Some(contents) = executable.parent().and_then(Path::parent)
    {
        places.push(contents.join("Resources/api-version"));
    }
    if let Some(home) = std::env::home_dir() {
        places.push(home.join("Applications/Lokyr.app"));
    }
    places.push(PathBuf::from("/Applications/Lokyr.app"));
    for place in places {
        if let Some(found) = api_of(&place) {
            return Some(found);
        }
    }
    let told = std::process::Command::new("lokyr")
        .arg("--api-version")
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let version = String::from_utf8(told.stdout).ok()?.trim().to_owned();
    valid(&version).then(|| (version, "`lokyr --api-version`".to_owned()))
}

/// The plugin API's version that `place` tells: an `api-version` file, an
/// app's bundle of one in its resources, or a binary of `--api-version`.
fn api_of(place: &Path) -> Option<(String, String)> {
    let file = if place.is_dir() {
        place.join("Contents/Resources/api-version")
    } else {
        place.to_path_buf()
    };
    if file.file_name().is_some_and(|name| name == "api-version") {
        let version = std::fs::read_to_string(&file).ok()?.trim().to_owned();
        return valid(&version).then(|| (version, file.display().to_string()));
    }
    let told = std::process::Command::new(&file)
        .arg("--api-version")
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let version = String::from_utf8(told.stdout).ok()?.trim().to_owned();
    valid(&version).then(|| (version, file.display().to_string()))
}

/// Whether `version` is a version, as `0.1.0`.
fn valid(version: &str) -> bool {
    let parts: Vec<&str> = version.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}
