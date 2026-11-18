//! `cargo xtask docs`: writes the commands' reference, `docs/commands.md`,
//! and the keys of the commands' tables of the bundled plugins' pages, of
//! the app's registry and its default keymap, as `lokyr-app-core`'s test of
//! them makes them; with `--check`, tells whether they are the app's. The
//! keys are macOS's, so it runs on a Mac alone.

use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};

use crate::luau_types::workspace_root;

/// The app's test that makes the reference and the tables, by its path.
const TEST: &str = "app::tests::docs::the_commands_reference_and_the_plugins_tables_are_the_app_s";

/// Runs the task with the remaining command-line arguments.
pub fn run(args: impl Iterator<Item = String>) -> Result<ExitCode> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            other => bail!("unknown argument `{other}`"),
        }
    }
    let ran = test(if check { &[] } else { &["LOKYR_WRITE_DOCS"] })?;
    if !ran {
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

/// A release's: writes the released commands' file of the app's commands
/// and keys, then the reference and the tables, of none marked as not
/// released yet.
pub(crate) fn release() -> Result<()> {
    ensure!(
        test(&["LOKYR_WRITE_DOCS", "LOKYR_RELEASED_COMMANDS"])?,
        "the commands' reference could not be written of the released commands"
    );
    Ok(())
}

/// Runs the app's test of the commands, of the variables `set` and of no
/// other of its own, which must have run, whose output shows; whether it
/// passed.
fn test(set: &[&str]) -> Result<bool> {
    if !cfg!(target_os = "macos") {
        bail!("the commands' reference is of macOS's keys: run it on a Mac");
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut test = Command::new(cargo);
    test.args([
        "test",
        "-p",
        "lokyr-app-core",
        "--lib",
        "--",
        TEST,
        "--exact",
    ])
    .current_dir(workspace_root());
    for name in ["LOKYR_WRITE_DOCS", "LOKYR_RELEASED_COMMANDS"] {
        if set.contains(&name) {
            test.env(name, "1");
        } else {
            test.env_remove(name);
        }
    }
    let output = test
        .output()
        .context("running the app's test of the commands")?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    // A test renamed, or of another system, runs nothing, and passes.
    ensure!(
        !output.status.success() || stdout.contains("test result: ok. 1 passed"),
        "the app's test of the commands, `{TEST}`, did not run"
    );
    Ok(output.status.success())
}
