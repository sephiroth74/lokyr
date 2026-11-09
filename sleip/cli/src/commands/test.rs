//! `sleip test`: the tests of the plugin of the current folder run, each
//! function of the table that each file `tests/*.luau` returns, of a
//! `TestHost` of its own, the plugin made anew on the SDK's fake host, of
//! its manifest's capabilities and its English messages of no argument;
//! each test told, a failure of its file, line and why; a failure of a
//! test that failed.

use std::path::Path;

use lokyr_pkg_model::manifest::{Kind, Manifest};
use lokyr_plugin_host_luau::Package;
use lokyr_plugin_host_luau::test_host::{self, Plugin};
use serde_json::json;

use crate::output::Out;

/// Runs the tests of the plugin of the current folder, as `out` tells.
pub fn test(out: Out) -> Result<(), String> {
    let folder = std::env::current_dir()
        .map_err(|error| format!("the current folder cannot be read: {error}"))?;
    let manifest = std::fs::read_to_string(folder.join("plugin.toml"))
        .map_err(|error| format!("plugin.toml cannot be read, as no plugin is in this folder or the file is not there: {error}"))?;
    let parsed = Manifest::parse(&manifest).map_err(|error| format!("plugin.toml:{error}"))?;
    if parsed.kind != Kind::Luau {
        return Err(format!(
            "`{}` is not a plugin of Luau, whose tests `sleip test` runs",
            parsed.name
        ));
    }
    let (sources, unread) = super::luau_sources(&folder);
    if let Some((name, error)) = unread.first() {
        return Err(format!("{name} cannot be read: {error}"));
    }
    let package = Package::new(sources);
    let messages = messages(&folder);
    let outcomes = test_host::run(&Plugin {
        name: &parsed.name,
        manifest: &manifest,
        package: &package,
        messages: &messages,
    });
    let failed = outcomes
        .iter()
        .filter(|outcome| outcome.failure.is_some())
        .count();
    for outcome in &outcomes {
        let (file, name) = (&outcome.file, &outcome.name);
        match &outcome.failure {
            None => out.tell(
                "test",
                json!({ "file": file, "name": name, "ok": true }),
                &format!("ok      {file} {name}"),
            ),
            Some(failure) => {
                let place = failure
                    .line
                    .map_or_else(|| file.clone(), |line| format!("{file}:{line}"));
                out.tell(
                    "test",
                    json!({
                        "file": file,
                        "name": name,
                        "ok": false,
                        "line": failure.line,
                        "message": failure.message,
                    }),
                    &format!("FAILED  {place} {name}: {}", failure.message),
                );
            }
        }
    }
    let passed = outcomes.len() - failed;
    let summary = match (outcomes.len(), failed) {
        (0, _) => "No test found, as no file tests/*.luau returns one.".to_owned(),
        (_, 0) => format!("{passed} passed."),
        _ => format!("{passed} passed, {failed} failed."),
    };
    out.tell(
        "done",
        json!({ "passed": passed, "failed": failed }),
        &summary,
    );
    // The summary told the failures already.
    if failed == 0 {
        Ok(())
    } else {
        Err(String::new())
    }
}

/// The plugin's messages in English of no argument, of its catalogs
/// `locales/en/*.ftl`, each its id and its text, which its tests' calls
/// read; those of arguments a test gives with `set_message`.
fn messages(folder: &Path) -> Vec<(String, String)> {
    use fluent_syntax::ast::{Entry, PatternElement};

    let mut messages = Vec::new();
    let Ok(entries) = std::fs::read_dir(folder.join("locales/en")) else {
        return messages;
    };
    let mut catalogs: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    catalogs.sort();
    for catalog in catalogs
        .iter()
        .filter(|path| path.extension().is_some_and(|extension| extension == "ftl"))
    {
        let Ok(text) = std::fs::read_to_string(catalog) else {
            continue;
        };
        let resource = match fluent_syntax::parser::parse(text.as_str()) {
            Ok(resource) => resource,
            Err((resource, _)) => resource,
        };
        for entry in resource.body {
            let Entry::Message(message) = entry else {
                continue;
            };
            let Some(pattern) = message.value else {
                continue;
            };
            let mut plain = String::new();
            let whole = pattern.elements.iter().all(|element| match element {
                PatternElement::TextElement { value } => {
                    plain.push_str(value);
                    true
                }
                PatternElement::Placeable { .. } => false,
            });
            if whole {
                messages.push((message.id.name.to_owned(), plain));
            }
        }
    }
    messages
}
