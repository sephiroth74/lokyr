//! `sleip list`: the plugins of the lockfile, each one's version, kind,
//! source and capabilities, and the installed plugin that replaces it, if
//! one does, which Lokyr turns it off for while that one runs.

use lokyr_pkg_model::lock::{Lock, Source};
use lokyr_pkg_model::manifest::Kind;
use serde_json::json;

use crate::output::Out;
use crate::paths::Paths;

/// Lists the installed plugins of `paths`, as `out` tells them.
pub fn list(paths: &Paths, out: Out) -> Result<(), String> {
    let lock = Lock::read(&paths.lock()).map_err(|error| error.to_string())?;
    let packages = lock.map(|lock| lock.packages).unwrap_or_default();
    if packages.is_empty() && !out.json() {
        crate::output::print("No plugin is installed.");
    }
    for (name, locked) in &packages {
        let replaced_by = packages
            .iter()
            .find(|(_, other)| {
                other
                    .capabilities
                    .contains(&lokyr_plugin_sdk::command::Capability::Replaces(
                        name.clone(),
                    ))
            })
            .map(|(other, _)| other.clone());
        let capabilities: Vec<String> = locked
            .capabilities
            .iter()
            .map(ToString::to_string)
            .collect();
        let source = match &locked.source {
            Source::Registry(_) => "registry",
            Source::Path(_) => "path",
            Source::Git { .. } => "git",
            Source::Dev(_) => "dev",
        };
        let kind = match locked.kind {
            Kind::Luau => "luau",
            Kind::Builtin => "builtin",
        };
        let asks = if capabilities.is_empty() {
            String::new()
        } else {
            format!(": {}", capabilities.join(", "))
        };
        let replaced = replaced_by
            .as_ref()
            .map(|other| format!(", replaced by {other}"))
            .unwrap_or_default();
        out.tell(
            "plugin",
            json!({
                "name": name,
                "version": locked.version,
                "kind": kind,
                "source": locked.source.to_string(),
                "capabilities": capabilities,
                "replaced_by": replaced_by,
            }),
            &format!("{name} {} ({source}{replaced}){asks}", locked.version),
        );
    }
    Ok(())
}
