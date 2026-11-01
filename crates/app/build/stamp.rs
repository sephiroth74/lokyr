//! The moment of a build, in UTC, to the minute: the date of the app's
//! version, its first eight digits, and the number of its bundle.

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
