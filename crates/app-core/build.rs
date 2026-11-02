//! Checks the manifests of the built-in plugins, `plugins/*/plugin.toml`,
//! which the app reads as it adds them: one that does not read fails the
//! build, with its file, line and column.

use std::path::Path;
use std::process::ExitCode;

use lokyr_pkg_model::manifest::{Kind, Manifest};

fn main() -> ExitCode {
    let plugins = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins");
    println!("cargo::rerun-if-changed={}", plugins.display());
    let Ok(entries) = std::fs::read_dir(&plugins) else {
        eprintln!(
            "error: the built-in plugins are not at {}",
            plugins.display()
        );
        return ExitCode::FAILURE;
    };
    let mut failed = false;
    let mut directories: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    directories.sort();
    for directory in directories {
        let path = directory.join("plugin.toml");
        let name = directory
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let shown = format!("plugins/{}/plugin.toml", name.unwrap_or_default());
        println!("cargo::rerun-if-changed={}", path.display());
        let Ok(text) = std::fs::read_to_string(&path) else {
            eprintln!("error: {shown}: a built-in plugin has no manifest");
            failed = true;
            continue;
        };
        match Manifest::parse(&text) {
            Ok(manifest) if manifest.kind != Kind::Builtin => {
                eprintln!("error: {shown}: a built-in plugin's kind is `builtin`");
                failed = true;
            }
            Ok(_) => {}
            Err(error) => {
                eprintln!("error: {shown}:{error}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
