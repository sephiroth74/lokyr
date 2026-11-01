//! The app's version, of the crate's, as a nightly's, a candidate's or a
//! release's, and the moment of a build, in UTC, to the minute: the date
//! of a nightly's version, its first eight digits, and the number of its
//! bundle.

/// The app's version, of the crate's `crate_version`, `X.Y.Z`, built at
/// `seconds` since the epoch: `release`'s, a candidate's `X.Y.Z-rc.N` or a
/// release's `X.Y.Z`, of the crate's numbers; or, without one, a nightly's,
/// with the date, `X.Y.Z-YYYYMMDD`. An error for a `release` of other
/// numbers or of another form.
pub fn version(crate_version: &str, release: Option<&str>, seconds: u64) -> Result<String, String> {
    let Some(release) = release else {
        return Ok(format!("{crate_version}-{}", &stamp(seconds)[..8]));
    };
    let candidate = release
        .strip_prefix(crate_version)
        .and_then(|rest| rest.strip_prefix("-rc."))
        .is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        });
    if release == crate_version || candidate {
        Ok(release.to_owned())
    } else {
        Err(format!(
            "LOKYR_RELEASE is `{release}`, neither the crate's version, `{crate_version}`, nor a candidate of it, `{crate_version}-rc.N`"
        ))
    }
}

/// The time in UTC of `seconds` since the epoch, as `202611011530`, by the
/// proleptic Gregorian calendar.
pub fn stamp(seconds: u64) -> String {
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let minutes = seconds % 86_400 / 60;
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
    format!(
        "{year:04}{month:02}{day:02}{:02}{:02}",
        minutes / 60,
        minutes % 60
    )
}
