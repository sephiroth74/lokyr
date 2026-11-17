//! `cargo xtask docs`: writes the commands' reference, `docs/commands.md`,
//! and the keys of the commands' tables of the bundled plugins' pages, of
//! the app's registry and its default keymap, as `lokyr-app-core`'s test of
//! them makes them; with `--check`, tells whether they are the app's.

use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};

use crate::luau_types::workspace_root;

/// Runs the task with the remaining command-line arguments.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => bail!("unknown argument `{other}`"),
        }
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut test = Command::new(cargo);
    test.args([
        "test",
        "-p",
        "lokyr-app-core",
        "--lib",
        "--",
        "the_commands_reference_and_the_plugins_tables_are_the_app_s",
    ])
    .current_dir(workspace_root());
    if check {
        test.env_remove("LOKYR_WRITE_DOCS");
    } else {
        test.env("LOKYR_WRITE_DOCS", "1");
    }
    let status = test
        .status()
        .context("running the app's test of the commands")?;
    if !status.success() {
        if check {
            eprintln!(
                "docs/commands.md or a plugin's table of commands is not the app's: run `cargo xtask docs`"
            );
        }
        return Ok(ExitCode::FAILURE);
    }
    println!(
        "{}",
        if check {
            "docs/commands.md and the plugins' tables of commands are the app's"
        } else {
            "wrote docs/commands.md and the plugins' tables of commands"
        }
    );
    Ok(ExitCode::SUCCESS)
}
