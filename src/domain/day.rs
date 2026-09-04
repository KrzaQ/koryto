//! The accounting day. An entry belongs to the calendar date of its instant
//! on the person's clock, shifted back by the day boundary, so a 01:00 snack
//! with the default 04:00 boundary lands on the evening before.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;

/// The zone in force at `instant`, given the location history sorted by
/// `valid_from` ascending. `None` when the history is empty or starts after
/// the instant, which the origin row is there to prevent.
pub fn zone_at(history: &[(DateTime<Utc>, Tz)], instant: DateTime<Utc>) -> Option<Tz> {
    history
        .iter()
        .rev()
        .find(|(from, _)| *from <= instant)
        .map(|(_, tz)| *tz)
}

pub fn day_of(instant: DateTime<Utc>, tz: Tz, boundary_minutes: i32) -> NaiveDate {
    (instant.with_timezone(&tz).naive_local() - Duration::minutes(i64::from(boundary_minutes)))
        .date()
}

/// The day "now" falls in for someone on `tz`.
pub fn today(tz: Tz, boundary_minutes: i32) -> NaiveDate {
    day_of(Utc::now(), tz, boundary_minutes)
}

/// Parse an IANA zone name; the error names the input.
pub fn parse_tz(name: &str) -> Result<Tz, String> {
    name.trim()
        .parse()
        .map_err(|_| format!("{name:?} is not an IANA time zone name"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::America::New_York;
    use chrono_tz::Europe::Warsaw;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }
    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    #[test]
    fn boundary_on_both_sides() {
        // 01:30 Warsaw on the 5th is still the 4th with a 04:00 boundary.
        assert_eq!(
            day_of(utc("2026-09-04T23:30:00Z"), Warsaw, 240),
            d("2026-09-04")
        );
        // 04:00 sharp is the new day; 03:59 is not.
        assert_eq!(
            day_of(utc("2026-09-05T02:00:00Z"), Warsaw, 240),
            d("2026-09-05")
        );
        assert_eq!(
            day_of(utc("2026-09-05T01:59:00Z"), Warsaw, 240),
            d("2026-09-04")
        );
        // A midnight boundary is the plain calendar date.
        assert_eq!(
            day_of(utc("2026-09-04T23:30:00Z"), Warsaw, 0),
            d("2026-09-05")
        );
    }

    #[test]
    fn zone_changes_the_day() {
        // 21:00 in New York is 03:00 the next day in Warsaw, but on the
        // traveller's clock it is still the same evening.
        let t = utc("2026-09-05T01:00:00Z");
        assert_eq!(day_of(t, New_York, 240), d("2026-09-04"));
        assert_eq!(day_of(t, Warsaw, 240), d("2026-09-04")); // 03:00 < 04:00
        let t = utc("2026-09-05T03:00:00Z");
        assert_eq!(day_of(t, New_York, 240), d("2026-09-04"));
        assert_eq!(day_of(t, Warsaw, 240), d("2026-09-05"));
    }

    #[test]
    fn dst_transition_is_just_a_clock() {
        // Warsaw leaves summer time on 2026-10-25 at 03:00 -> 02:00.
        assert_eq!(
            day_of(utc("2026-10-25T00:30:00Z"), Warsaw, 240),
            d("2026-10-24")
        ); // 02:30 CEST
        assert_eq!(
            day_of(utc("2026-10-25T01:30:00Z"), Warsaw, 240),
            d("2026-10-24")
        ); // 02:30 CET
        assert_eq!(
            day_of(utc("2026-10-25T03:00:00Z"), Warsaw, 240),
            d("2026-10-25")
        ); // 04:00 CET
    }

    #[test]
    fn history_lookup() {
        let history = vec![
            (utc("0001-01-01T00:00:00Z"), Warsaw),
            (utc("2026-09-10T12:00:00Z"), New_York),
        ];
        assert_eq!(zone_at(&history, utc("2026-09-01T00:00:00Z")), Some(Warsaw));
        assert_eq!(
            zone_at(&history, utc("2026-09-10T12:00:00Z")),
            Some(New_York)
        );
        assert_eq!(
            zone_at(&history, utc("2026-09-20T00:00:00Z")),
            Some(New_York)
        );
        assert_eq!(zone_at(&[], utc("2026-09-20T00:00:00Z")), None);
        assert!(parse_tz("Mars/Olympus").is_err());
        assert_eq!(parse_tz(" Europe/Warsaw ").unwrap(), Warsaw);
    }
}
