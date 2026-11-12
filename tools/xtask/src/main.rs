//! Build automation for the Lokyr workspace, run as `cargo xtask <task>`.

mod bundle;
mod cask;
mod changelog;
mod check_comments;
mod check_deps;
mod check_links;
mod dmg;
mod feed;
mod gen_dist;
mod languages;
mod licenses;
mod luau_types;
mod notarize;
mod private;
mod release;
mod sbom;
mod site;
mod sparkle;
mod symbolicate;
mod terminfo;
mod themes;
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
        Some("cask") => cask::run(args),
        Some("check-changelog") => changelog::run(args),
        Some("check-comments") => check_comments::run(args),
        Some("check-deps") => check_deps::run(args),
        Some("check-language") => languages::run_check(args),
        Some("check-links") => check_links::run(args),
        Some("dmg") => dmg::run(args),
        Some("feed") => feed::run(args),
        Some("gen-dist") => gen_dist::run(args),
        Some("luau-types") => luau_types::run(args),
        Some("new-language") => languages::run_new(args),
        Some("notarize") => notarize::run(args),
        Some("release") => release::run(args),
        Some("site") => site::run(args),
        Some("symbolicate") => symbolicate::run(args),
        Some("terminfo") => terminfo::run(args),
        Some("themes") => themes::run(args),
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
         cask <version> [--tap <dir>] write the Homebrew cask of a release, of its DMG's SHA-256\n  \
         check-changelog             check CHANGELOG.md against Keep a Changelog's format\n  \
         check-comments              find the plan, tasks, phases and ADRs, in the code's comments\n  \
         check-deps [--print-graph]  check the dependency rules of the workspace\n  \
         check-language <code>       check a language's catalogs against English's: missing, extra and untranslated messages\n  \
         check-links                 find the internal links of the Markdown files that do not resolve\n  \
         dmg [--no-notarize]         build target/Lokyr-<version>.dmg of the bundle, notarized first, signed, notarized and stapled\n  \
         dmg layout                  have the Finder lay out the image's window, and keep its .DS_Store\n  \
         feed add <dmg> [--notes <file>] [--channel <name>]  add a DMG and its notes to the feed of tests, its appcast signed\n  \
         feed serve                  have Lokyr on this Mac read the feed of tests, and serve it on 127.0.0.1:8123\n  \
         gen-dist [--check]          the app's distributions' features and plugins, of dist/*.toml, or check them\n  \
         luau-types [--check]        write the plugin API's types for Luau, lokyr.d.luau, of the SDK's, or check them\n  \
         new-language <code>         make a language ready to translate: English's catalogs copied, and every list and guide of it\n  \
         notarize                    have Apple notarize target/Lokyr.app, and staple the ticket\n  \
         release <version> [--without-feed]  a candidate's or a release's bundle, DMG, dSYM and SBOM, to a draft on GitHub and the feed\n  \
         site [--out <dir>]          build the documentation's site of the public Markdown into target/site\n  \
         symbolicate <report.ips>    the crashed thread of a crash report, Lokyr's frames told by the release's dSYM\n  \
         terminfo [--out DIR]        compile the terminfo entry xterm-lokyr, into target/terminfo by default\n  \
         themes [--out DIR]          check the themes' collection and write the themes that the bundle holds, into target/themes by default\n  \
         ucd [--check]               generate the Unicode tables of vt-core, or check them"
    );
}
