//! `sleip doctor`: the installation diagnosed, each check told: the config's
//! folder; `plugins.toml`, `plugins.lock` and `permissions.toml`, that they
//! read; the store, and each package that the lockfile names against its
//! digest, or each folder that `sleip dev` links; the packages that it does
//! not name; the cache; and the app's plugin API, which each plugin's
//! manifest takes. A failure of the command of a problem found.

use lokyr_pkg_model::list::List;
use lokyr_pkg_model::lock::{Lock, Source};
use lokyr_pkg_model::manifest::Manifest;
use lokyr_pkg_model::permissions::Permissions;
use serde_json::json;

use super::{bytes_text, size_of};
use crate::output::Out;
use crate::paths::{self, Paths};

/// How a check went.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Ok,
    /// Nothing wrong, of something to know.
    Note,
    Error,
}

/// The checks told so far.
struct Report {
    out: Out,
    problems: usize,
}

impl Report {
    /// Tells the check `name` of `level`, as `detail` says.
    fn check(&mut self, name: &str, level: Level, detail: &str) {
        let level = match level {
            Level::Ok => "ok",
            Level::Note => "note",
            Level::Error => {
                self.problems += 1;
                "error"
            }
        };
        self.out.tell(
            "check",
            json!({ "name": name, "level": level, "detail": detail }),
            &format!("{level:<6} {name}: {detail}"),
        );
    }
}

/// Diagnoses the installation of `paths`, as `out` tells.
pub fn doctor(paths: &Paths, out: Out) -> Result<(), String> {
    let mut report = Report { out, problems: 0 };
    let config = paths.config.display().to_string();
    if paths.config.is_dir() {
        report.check("config", Level::Ok, &config);
    } else {
        report.check("config", Level::Note, &format!("{config} is not there yet"));
    }
    match List::read(&paths.list()) {
        Ok(list) => report.check(
            "plugins.toml",
            Level::Ok,
            &format!("{} plugins asked for", list.plugins.len()),
        ),
        Err(error) => report.check("plugins.toml", Level::Error, &error.to_string()),
    }
    match Permissions::read(&paths.permissions()) {
        Ok(permissions) => report.check(
            "permissions.toml",
            Level::Ok,
            &format!("the decisions of {} plugins", permissions.plugins.len()),
        ),
        Err(error) => report.check("permissions.toml", Level::Error, &error.to_string()),
    }
    let lock = match Lock::read(&paths.lock()) {
        Ok(lock) => {
            let count = lock.as_ref().map_or(0, |lock| lock.packages.len());
            report.check(
                "plugins.lock",
                Level::Ok,
                &format!("{count} plugins installed"),
            );
            lock
        }
        Err(error) => {
            report.check("plugins.lock", Level::Error, &error.to_string());
            None
        }
    };
    let store = &paths.store;
    let held = store.digests().unwrap_or_default();
    if store.root().is_dir() {
        report.check(
            "store",
            Level::Ok,
            &format!("{}, {} packages", store.root().display(), held.len()),
        );
    } else {
        report.check(
            "store",
            Level::Note,
            &format!("{} is not there yet", store.root().display()),
        );
    }
    let app = paths::app_api();
    match &app {
        Some((version, place)) => {
            report.check("app", Level::Ok, &format!("the plugin API {version}, of {place}"));
        }
        None => report.check(
            "app",
            Level::Error,
            "Lokyr is not found, of SLEIP_LOKYR, the Applications folders or the PATH, and so neither its plugin API",
        ),
    }
    let packages = lock.map(|lock| lock.packages).unwrap_or_default();
    for (name, locked) in &packages {
        let check = format!("plugin {name}");
        let folder = match (&locked.source, &locked.checksum) {
            (Source::Dev(folder), _) => std::path::PathBuf::from(folder),
            (_, Some(digest)) => {
                if let Err(error) = store.check(digest) {
                    report.check(&check, Level::Error, &error.to_string());
                    continue;
                }
                store.folder(digest)
            }
            (_, None) => {
                report.check(&check, Level::Error, "its package has no checksum");
                continue;
            }
        };
        let manifest = std::fs::read_to_string(folder.join("plugin.toml"))
            .map_err(|error| {
                format!(
                    "{} cannot be read: {error}",
                    folder.join("plugin.toml").display()
                )
            })
            .and_then(|text| {
                Manifest::parse(&text).map_err(|error| format!("its plugin.toml: {error}"))
            });
        match (manifest, &app) {
            (Err(error), _) => report.check(&check, Level::Error, &error),
            (Ok(manifest), Some((version, _))) if !manifest.takes_api(version) => report.check(
                &check,
                Level::Error,
                &format!(
                    "it takes the plugin API {}, which the app's, {version}, is not",
                    manifest.api.unwrap_or_default()
                ),
            ),
            (Ok(_), _) => report.check(
                &check,
                Level::Ok,
                &format!("{} of {}", locked.version, folder.display()),
            ),
        }
    }
    let named: Vec<_> = packages
        .values()
        .filter_map(|locked| locked.checksum)
        .collect();
    let unnamed = held.iter().filter(|digest| !named.contains(digest)).count();
    if unnamed > 0 {
        report.check(
            "unused",
            Level::Note,
            &format!(
                "{unnamed} packages of the store that no lockfile names, which `sleip gc` removes"
            ),
        );
    }
    let cache = paths.cache.display().to_string();
    if paths.cache.is_dir() {
        report.check(
            "cache",
            Level::Ok,
            &format!("{cache}, {}", bytes_text(size_of(&paths.cache))),
        );
    } else {
        report.check("cache", Level::Note, &format!("{cache} is not there yet"));
    }
    let problems = report.problems;
    let summary = match problems {
        0 => "No problem found.".to_owned(),
        1 => "1 problem found.".to_owned(),
        count => format!("{count} problems found."),
    };
    out.tell("done", json!({ "problems": problems }), &summary);
    // The summary told the problems already.
    if problems == 0 {
        Ok(())
    } else {
        Err(String::new())
    }
}
