//! `cargo xtask luau-types`: writes `lokyr.d.luau`, the plugin API's types
//! for Luau's type checker, which `lokyr-plugin-host-luau` makes of the
//! SDK's types, into `crates/plugin-host-luau/luau/`; with `--check`, tells
//! whether the file there is the SDK's, without writing it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

/// The file, relative to the workspace root.
pub(crate) const FILE: &str = "crates/plugin-host-luau/luau/lokyr.d.luau";

/// Runs the task with the remaining command-line arguments.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => bail!("unknown argument `{other}`"),
        }
    }
    let path = workspace_root().join(FILE);
    let made = lokyr_plugin_host_luau::types::definitions();
    if check {
        let held = fs::read_to_string(&path).with_context(|| format!("reading {FILE}"))?;
        if held != made {
            eprintln!("{FILE} is not the SDK's: run `cargo xtask luau-types`");
            return Ok(ExitCode::FAILURE);
        }
        println!("{FILE} is the SDK's");
        return Ok(ExitCode::SUCCESS);
    }
    fs::write(&path, made).with_context(|| format!("writing {FILE}"))?;
    println!("wrote {FILE}");
    Ok(ExitCode::SUCCESS)
}

/// The workspace's root, two levels above xtask's manifest.
pub(crate) fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("xtask lives two levels below the workspace root")
        .to_path_buf()
}
