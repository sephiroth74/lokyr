//! `lokyr-e2e`: Lokyr.app checked end to end, as a hand would use it, on
//! the Mac that runs it: its windows, tabs, splits, keys and find bar,
//! through the keys that the system posts to its process and its
//! accessibility tree, which tells what it shows. Each step waits for what
//! it checks, and a failure tells the step and what the app showed.
//!
//! The app runs of a home of its own, in a temporary directory, so that its
//! config, its state and its log are not the user's; its shells are the
//! user's, of the user's account. The keys go to its process alone. It
//! takes the Accessibility permission of the app that runs the checks, as
//! the terminal or the editor that starts them, and the screen while they
//! run: nothing else should take the keyboard's focus meanwhile.
//!
//! `lokyr-e2e [--app <Lokyr.app>] [--dump] [--verbose]`: the app of
//! `target/Lokyr.app` by default; `--dump` prints its tree once its window
//! opens, and quits; `--verbose` prints what each step saw.

// `deny` rather than `forbid`: the accessibility of another app, its keys
// and its process go through C APIs, which are `unsafe`, and each of those
// call sites is allowed explicitly.
#![deny(unsafe_code)]

#[cfg(target_os = "macos")]
mod ax;
#[cfg(target_os = "macos")]
mod keys;
#[cfg(target_os = "macos")]
mod run;

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(target_os = "macos")]
    {
        run::main()
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("lokyr-e2e: the checks run on macOS alone");
        ExitCode::FAILURE
    }
}
