//! RFC 3339 timestamps in UTC, with microsecond precision, without a dependency.
//!
//! SOLAR needs one thing from a calendar: turning a [`SystemTime`] into the string that
//! goes into `meta.started_at`. That is a dozen lines of arithmetic, so it is written here
//! rather than pulled in, and it is checked against known instants by the tests below.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds in a day.
const SECONDS_PER_DAY: i64 = 86_400;

/// The current instant, formatted for `meta.started_at`.
#[must_use]
pub fn now_rfc3339_micros() -> String {
    format_rfc3339_micros(SystemTime::now())
}

/// Formats an instant as `YYYY-MM-DDThh:mm:ss.ffffffZ`, always in UTC.
///
/// An instant before the Unix epoch, which only a badly set clock can produce, is reported
/// as the epoch itself rather than as a negative year.
#[must_use]
pub fn format_rfc3339_micros(time: SystemTime) -> String {
    let (secs, micros) = match time.duration_since(UNIX_EPOCH) {
        // Wrapping would need a clock 292 billion years past the epoch.
        Ok(delta) => (delta.as_secs().cast_signed(), delta.subsec_micros()),
        Err(_) => (0, 0),
    };

    let days = secs.div_euclid(SECONDS_PER_DAY);
    let seconds_of_day = secs.rem_euclid(SECONDS_PER_DAY);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{micros:06}Z")
}

/// Turns a count of days since 1970-01-01 into a civil date.
///
/// This is Howard Hinnant's `civil_from_days`, the algorithm behind `std::chrono`. It is
/// exact for every year the proleptic Gregorian calendar covers.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the algorithm bounds day to 1..=31 and month to 1..=12 before the casts"
)]
const fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = (if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    }) as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(secs: u64, micros: u32) -> String {
        format_rfc3339_micros(UNIX_EPOCH + Duration::new(secs, micros * 1000))
    }

    #[test]
    fn the_epoch_is_the_first_of_january_nineteen_seventy() {
        assert_eq!(at(0, 0), "1970-01-01T00:00:00.000000Z");
    }

    #[test]
    fn known_instants_match_to_the_second() {
        assert_eq!(at(1_700_000_000, 0), "2023-11-14T22:13:20.000000Z");
        assert_eq!(at(1_735_689_600, 0), "2025-01-01T00:00:00.000000Z");
        assert_eq!(at(946_684_800, 0), "2000-01-01T00:00:00.000000Z");
    }

    #[test]
    fn the_twenty_ninth_of_february_two_thousand_exists() {
        assert_eq!(at(951_782_400, 0), "2000-02-29T00:00:00.000000Z");
        assert_eq!(at(951_868_800, 0), "2000-03-01T00:00:00.000000Z");
    }

    #[test]
    fn nineteen_hundred_was_not_a_leap_year_under_the_same_arithmetic() {
        // 1900-02-28 is 25509 days before the epoch, and the next day must be March.
        assert_eq!(civil_from_days(-25_509), (1900, 2, 28));
        assert_eq!(civil_from_days(-25_508), (1900, 3, 1));
    }

    #[test]
    fn microseconds_are_padded_to_six_digits() {
        assert_eq!(at(0, 7), "1970-01-01T00:00:00.000007Z");
        assert_eq!(at(0, 123_456), "1970-01-01T00:00:00.123456Z");
    }

    #[test]
    fn a_clock_before_the_epoch_reports_the_epoch() {
        let before = UNIX_EPOCH - Duration::from_secs(10);
        assert_eq!(format_rfc3339_micros(before), "1970-01-01T00:00:00.000000Z");
    }

    #[test]
    fn now_has_the_shape_the_contract_requires() {
        let now = now_rfc3339_micros();
        assert_eq!(now.len(), 27, "{now} is not YYYY-MM-DDThh:mm:ss.ffffffZ");
        assert!(now.ends_with('Z'));
        assert_eq!(now.as_bytes()[10], b'T');
        assert!(
            now.starts_with("20"),
            "{now} does not look like this century"
        );
    }
}
