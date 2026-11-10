//! `sleip search` and `sleip info`: the registry's plugins, of its index,
//! fetched, checked against the root keys and cached, or of the cache
//! alone with `--offline`. `search` reads the list of the plugins, a
//! plugin that every word finds in its name or its description; `info`
//! reads a plugin's index file, its versions, capabilities and checksums.
//! The registry is `plugins.toml`'s `default`, or Lokyr's.

use lokyr_pkg_model::list::List;
use serde_json::json;
use sleip_index::client::Client;
use sleip_index::remote::Remote;
use sleip_index::signing::{self, PublicKey};

use crate::args::Options;
use crate::output::Out;
use crate::paths::Paths;

/// The site of Lokyr's registry, which searches its plugins too.
const SITE: &str = "https://pkg.lokyr.dev";

/// The root keys that Sleip trusts: its own; in a build for debugging
/// alone, those of `SLEIP_TEST_ROOT_KEYS`, a key a line or a comma, which
/// the tests take, as a release reads none of it.
fn roots() -> Vec<PublicKey> {
    #[cfg(debug_assertions)]
    if let Ok(text) = std::env::var("SLEIP_TEST_ROOT_KEYS") {
        return text
            .split([',', '\n'])
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .filter_map(|key| PublicKey::parse(key).ok())
            .collect();
    }
    signing::roots()
}

/// The registry of `plugins.toml` of `paths`, its `default`.
fn registry_url(paths: &Paths) -> Result<String, String> {
    let list = List::read(&paths.list()).map_err(|error| error.to_string())?;
    list.registry("default")
        .map(str::to_owned)
        .ok_or_else(|| "plugins.toml names no default registry".to_owned())
}

/// Searches the registry's plugins of `words`, as `options` and `out`
/// say.
pub fn search(paths: &Paths, options: Options, out: Out, words: &[String]) -> Result<(), String> {
    let url = registry_url(paths)?;
    let client = (!options.offline).then(Client::new);
    let remote = Remote::new(&url, &paths.cache, client.as_ref(), roots());
    let keys = remote.keys()?;
    let list = remote.list(&keys)?;
    let words: Vec<String> = words.iter().map(|word| word.to_lowercase()).collect();
    let mut found = 0;
    for plugin in &list.plugins {
        let text = format!(
            "{} {}",
            plugin.name,
            plugin.description.as_deref().unwrap_or_default()
        )
        .to_lowercase();
        if !words.iter().all(|word| text.contains(word.as_str())) {
            continue;
        }
        found += 1;
        let yanked = if plugin.yanked { " (yanked)" } else { "" };
        let description = plugin.description.as_deref().unwrap_or_default();
        out.tell(
            "found",
            json!({
                "name": plugin.name,
                "version": plugin.vers,
                "description": description,
                "capabilities": plugin.caps,
                "yanked": plugin.yanked,
            }),
            &format!("{} {}{yanked}  {description}", plugin.name, plugin.vers),
        );
    }
    let more = if url.trim_end_matches('/') == SITE {
        format!("; {SITE} searches their pages too")
    } else {
        String::new()
    };
    out.tell(
        "done",
        json!({ "found": found }),
        &format!("{found} plugins found{more}."),
    );
    Ok(())
}

/// Tells the registry's plugin `plugin`, `owner/name`: its versions, newest
/// first, as `options` and `out` say.
pub fn info(paths: &Paths, options: Options, out: Out, plugin: &str) -> Result<(), String> {
    let url = registry_url(paths)?;
    let client = (!options.offline).then(Client::new);
    let remote = Remote::new(&url, &paths.cache, client.as_ref(), roots());
    let keys = remote.keys()?;
    let index = remote
        .index(plugin, &keys)?
        .ok_or_else(|| format!("{url} publishes no plugin `{plugin}`"))?;
    let Some(shown) = index
        .entries
        .iter()
        .rev()
        .find(|entry| !entry.yanked)
        .or_else(|| index.entries.last())
    else {
        return Err(format!("{url} publishes no version of `{plugin}` yet"));
    };
    let versions: Vec<serde_json::Value> = index
        .entries
        .iter()
        .rev()
        .map(|entry| {
            json!({
                "version": entry.vers,
                "yanked": entry.yanked,
                "capabilities": entry.caps,
                "cksum": entry.cksum,
            })
        })
        .collect();
    let mut text = format!("{plugin} {}", shown.vers);
    if let Some(description) = &shown.description {
        text.push_str(&format!("\n{description}"));
    }
    let list = |items: &[String]| {
        if items.is_empty() {
            "none".to_owned()
        } else {
            items.join(", ")
        }
    };
    text.push_str(&format!("\nCapabilities: {}", list(&shown.caps)));
    if !shown.uses.is_empty() {
        text.push_str(&format!("\nCommands it runs: {}", list(&shown.uses)));
    }
    if !shown.deps.is_empty() {
        let deps: Vec<String> = shown
            .deps
            .iter()
            .map(|dependency| format!("{} {}", dependency.name, dependency.req))
            .collect();
        text.push_str(&format!("\nDepends on: {}", deps.join(", ")));
    }
    text.push_str("\nVersions:");
    for entry in index.entries.iter().rev() {
        let yanked = if entry.yanked { " (yanked)" } else { "" };
        text.push_str(&format!("\n  {}{yanked}  {}", entry.vers, entry.cksum));
    }
    if url.trim_end_matches('/') == SITE {
        text.push_str(&format!("\n{SITE}/plugins/{plugin}/"));
    }
    out.tell(
        "info",
        json!({
            "name": plugin,
            "version": shown.vers,
            "description": shown.description,
            "kind": shown.kind,
            "api": shown.api,
            "capabilities": shown.caps,
            "uses": shown.uses,
            "dependencies": shown.deps.iter().map(|dependency| json!({ "name": dependency.name, "req": dependency.req })).collect::<Vec<_>>(),
            "versions": versions,
        }),
        &text,
    );
    Ok(())
}
