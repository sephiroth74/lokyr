//! The system's clock and the local time zone's offset from UTC, which the
//! plugins read for a time of day.

use std::time::{SystemTime, UNIX_EPOCH};

use lokyr_kernel::plugins::host::LocalTime;

/// The system's time and the local time zone's offset from UTC now.
pub(crate) fn local_time() -> LocalTime {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    LocalTime {
        time: u64::try_from(since.as_millis()).unwrap_or(u64::MAX),
        offset: utc_offset(since.as_secs()),
    }
}

/// The local time zone's offset from UTC, in seconds east of it, at
/// `seconds` since 1970-01-01, as the C library's `localtime_r` reads the
/// zone that the system sets.
#[cfg(unix)]
#[allow(unsafe_code)]
fn utc_offset(seconds: u64) -> i32 {
    let Ok(time) = libc::time_t::try_from(seconds) else {
        return 0;
    };
    // SAFETY: `tm` is a plain C struct, of no invariant, which zeroes make.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: both pointers are to values that live through the call, which
    // fills `tm` alone, as `localtime_r` keeps no pointer to them.
    let filled = unsafe { libc::localtime_r(&time, &mut tm) };
    if filled.is_null() {
        return 0;
    }
    i32::try_from(tm.tm_gmtoff).unwrap_or(0)
}

/// The local time zone's offset from UTC: none known here.
#[cfg(not(unix))]
fn utc_offset(_seconds: u64) -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_local_time_is_the_clock_s_of_an_offset_within_a_day() {
        let before = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let now = local_time();
        assert!(now.time >= before && now.time < before + 60_000, "{now:?}");
        assert!(now.offset.abs() <= 14 * 3600, "{now:?}");
        assert_eq!(now.offset % 60, 0, "{now:?}");
    }
}
