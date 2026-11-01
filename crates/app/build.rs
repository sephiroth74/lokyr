//! Lokyr's version, which `LOKYR_VERSION` gives the app: the crate's, with
//! the date of the commit that it is built from, in UTC, as
//! `0.1.0-20261101`, so that the same source gives the same version.
//! `SOURCE_DATE_EPOCH`, when it is set, gives the date, for builds that must
//! come out the same; a source without git takes the build's date.

use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    let seconds = source_date_epoch().or_else(commit_time).unwrap_or_else(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    });
    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo gives the crate's version");
    println!("cargo:rustc-env=LOKYR_VERSION={version}-{}", date(seconds));
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

/// The date in UTC of `seconds` since the epoch, as `20261101`, by the
/// proleptic Gregorian calendar.
fn date(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    // Howard Hinnant's `civil_from_days`, from days since 1970-01-01.
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_from_march = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_from_march + 2) / 5 + 1;
    let month = if month_from_march < 10 {
        month_from_march + 3
    } else {
        month_from_march - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}")
}
