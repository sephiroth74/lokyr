//! Build automation for the Lokyr workspace, run as `cargo xtask <task>`.

mod bundle;
mod check_comments;
mod check_deps;
mod check_links;
mod dmg;
mod feed;
mod notarize;
mod private;
mod release;
mod sbom;
mod sparkle;
mod terminfo;
mod ucd;

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let result = match args.next().as_deref() {
        None | Some("help" | "-h" | "--help") => {
            print_help();
            return ExitCode::SUCCESS;
        }
        Some("bundle") => bundle::run(args),
        Some("check-comments") => check_comments::run(args),
        Some("check-deps") => check_deps::run(args),
        Some("check-links") => check_links::run(args),
        Some("dmg") => dmg::run(args),
        Some("feed") => feed::run(args),
        Some("notarize") => notarize::run(args),
        Some("release") => release::run(args),
        Some("terminfo") => terminfo::run(args),
        Some("ucd") => ucd::run(args),
        Some(task) => {
            eprintln!("error: unknown task `{task}`\n");
            print_help();
            return ExitCode::FAILURE;
        }
    };
    result.unwrap_or_else(|error| {
        eprintln!("error: {error:#}");
        ExitCode::FAILURE
    })
}

fn print_help() {
    println!(
        "Usage: cargo xtask <task>\n\n\
         Tasks:\n  \
         bundle                      build target/Lokyr.app from the release binary, signed\n  \
         check-comments              find the plan, tasks, phases and ADRs, in the code's comments\n  \
         check-deps [--print-graph]  check the dependency rules of the workspace\n  \
         check-links                 find the internal links of the Markdown files that do not resolve\n  \
         dmg [--no-notarize]         build target/Lokyr-<version>.dmg of the bundle, notarized first, signed, notarized and stapled\n  \
         dmg layout                  have the Finder lay out the image's window, and keep its .DS_Store\n  \
         feed add <dmg>              add a DMG and its notes to the feed of tests in private/feed, its appcast signed\n  \
         feed serve                  have Lokyr on this Mac read the feed of tests, and serve it on 127.0.0.1:8123\n  \
         notarize                    have Apple notarize target/Lokyr.app, and staple the ticket\n  \
         release <version>           a candidate's or a release's bundle, DMG, dSYM and SBOM, to a draft on GitHub and the feed\n  \
         terminfo [--out DIR]        compile the terminfo entry xterm-lokyr, into target/terminfo by default\n  \
         ucd [--check]               generate the Unicode tables of vt-core, or check them"
    );
}
