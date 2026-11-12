//! Lokyr's version, which `LOKYR_VERSION` gives the app: the crate's, with
//! the date of the commit that it is built from, in UTC, as
//! `0.2.0-20261101`, so that the same source gives the same version; or, as
//! `LOKYR_RELEASE` names it, a candidate's, `0.2.0-rc.1`, or a release's,
//! `0.2.0`, of the crate's numbers. `SOURCE_DATE_EPOCH`, when it is set,
//! gives the date, for builds that must come out the same, and for the
//! bundle, whose number is of the same moment; a source without git takes
//! the build's date.

#[path = "build/version.rs"]
mod version;

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rerun-if-env-changed=LOKYR_RELEASE");
    let seconds = source_date_epoch().or_else(commit_time).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    });
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo gives the crate's version");
    let release = std::env::var("LOKYR_RELEASE")
        .ok()
        .filter(|release| !release.is_empty());
    match version::version(&version, release.as_deref(), seconds) {
        Ok(version) => println!("cargo:rustc-env=LOKYR_VERSION={version}"),
        Err(error) => panic!("{error}"),
    }
}

/// The seconds since the epoch that `SOURCE_DATE_EPOCH` gives, when it is
/// set.
fn source_date_epoch() -> Option<u64> {
    std::env::var("SOURCE_DATE_EPOCH").ok()?.trim().parse().ok()
}

/// The time of HEAD's commit, in seconds since the epoch; the script runs
/// again as the files that a commit, a checkout or a branch's move changes
/// do.
fn commit_time() -> Option<u64> {
    let git = |args: &[&str]| -> Option<String> {
        let output = Command::new("git").args(args).output().ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let seconds = git(&["log", "-1", "--format=%ct"])?.parse().ok()?;
    // The reflog of `HEAD` takes a line at each commit, as a branch's ref
    // in `packed-refs` alone, after `git gc`, does not change its file.
    let mut moved = vec![
        "HEAD".to_owned(),
        "logs/HEAD".to_owned(),
        "packed-refs".to_owned(),
    ];
    moved.extend(git(&["symbolic-ref", "-q", "HEAD"]));
    for name in moved {
        // A file that does not exist would run the script at every build.
        if let Some(path) = git(&["rev-parse", "--git-path", &name])
            && Path::new(&path).exists()
        {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    Some(seconds)
}
