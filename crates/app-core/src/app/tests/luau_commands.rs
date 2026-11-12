//! The commands that the host of a Luau plugin's tests holds, beside the
//! plugin's own: the app's and its first-party plugins', as the registry
//! of an app of the default distribution holds them, which
//! `lokyr-plugin-host-luau` keeps in `luau/commands.json`.

use std::path::Path;

use super::*;

/// The file, of `lokyr-plugin-host-luau`'s.
fn file() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin-host-luau/luau/commands.json")
}

#[test]
fn the_luau_test_host_holds_the_app_s_commands() {
    let (_fake, app) = started();
    let mut names: Vec<&str> = app
        .commands
        .in_order()
        .map(|command| command.name.as_str())
        .collect();
    names.sort_unstable();
    let entries: Vec<serde_json::Value> = names
        .into_iter()
        .filter_map(|name| {
            let command = app.commands.get(name)?;
            let mut entry = serde_json::to_value(command).ok()?;
            entry.as_object_mut()?.insert(
                "plugin".to_owned(),
                app.commands
                    .owner(name)
                    .map_or(serde_json::Value::Null, Into::into),
            );
            Some(entry)
        })
        .collect();
    let made = serde_json::to_string_pretty(&entries).unwrap() + "\n";
    if std::env::var_os("LOKYR_WRITE_COMMANDS").is_some() {
        std::fs::write(file(), &made).unwrap();
        return;
    }
    let held = std::fs::read_to_string(file()).unwrap_or_default();
    assert!(
        held == made,
        "luau/commands.json is not the app's registry: `just luau-commands` writes it"
    );
}
