//! `sleip list`: the plugins of the lockfile, each one's version, kind,
//! source and capabilities.

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
        println!("No plugin is installed.");
    }
    for (name, locked) in &packages {
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
            Kind::Themes => "themes",
            Kind::Builtin => "builtin",
        };
        let asks = if capabilities.is_empty() {
            String::new()
        } else {
            format!(": {}", capabilities.join(", "))
        };
        out.tell(
            "plugin",
            json!({
                "name": name,
                "version": locked.version,
                "kind": kind,
                "source": locked.source.to_string(),
                "capabilities": capabilities,
            }),
            &format!("{name} {} ({source}){asks}", locked.version),
        );
    }
    Ok(())
}
