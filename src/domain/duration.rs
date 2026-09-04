//! Sport durations are stored as integer minutes. This module is the only
//! place that converts between the input grammar and minutes.

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("cannot read {0:?} as a duration (try 45, 45m, 1h, 1h30, 1:30 or 1.5h)")]
pub struct DurationError(String);

/// Accepts `45` (minutes), `45m`, `1h`, `1h30`, `1h30m`, `1:30`, `1.5h`.
/// Returns whole minutes, always positive.
pub fn parse_minutes(input: &str) -> Result<i32, DurationError> {
    let s = input.trim().to_ascii_lowercase().replace(' ', "");
    let err = || DurationError(input.to_string());
    if s.is_empty() {
        return Err(err());
    }
    let minutes: i64 = if let Some((h, m)) = s.split_once(':') {
        let h: i64 = h.parse().map_err(|_| err())?;
        let m: i64 = m.parse().map_err(|_| err())?;
        if m >= 60 {
            return Err(err());
        }
        h * 60 + m
    } else if let Some(rest) = s.strip_suffix('m') {
        if let Some((h, m)) = rest.split_once('h') {
            let h: i64 = h.parse().map_err(|_| err())?;
            let m: i64 = if m.is_empty() {
                0
            } else {
                m.parse().map_err(|_| err())?
            };
            if m >= 60 {
                return Err(err());
            }
            h * 60 + m
        } else {
            rest.parse().map_err(|_| err())?
        }
    } else if let Some((h, m)) = s.split_once('h') {
        if m.is_empty() {
            let dec: Decimal = h.replace(',', ".").parse().map_err(|_| err())?;
            (dec * Decimal::from(60)).round().to_i64().ok_or_else(err)?
        } else {
            let h: i64 = h.parse().map_err(|_| err())?;
            let m: i64 = m.parse().map_err(|_| err())?;
            if m >= 60 {
                return Err(err());
            }
            h * 60 + m
        }
    } else {
        s.parse().map_err(|_| err())?
    };
    if minutes <= 0 {
        return Err(err());
    }
    i32::try_from(minutes).map_err(|_| err())
}

/// `45m`, `1h`, `1h30`.
pub fn format_minutes(minutes: i32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h{m:02}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_forms() {
        for (input, minutes) in [
            ("45", 45),
            ("45m", 45),
            ("1h", 60),
            ("1h30", 90),
            ("1h30m", 90),
            ("1h 30m", 90),
            ("1:30", 90),
            ("1.5h", 90),
            ("1,5h", 90),
            (" 2H ", 120),
            ("0:45", 45),
        ] {
            assert_eq!(parse_minutes(input), Ok(minutes), "{input}");
        }
    }

    #[test]
    fn rejected_forms() {
        for input in ["", "abc", "1:60", "1h60", "-1", "0", "0m", "1.5"] {
            assert!(parse_minutes(input).is_err(), "{input}");
        }
    }

    #[test]
    fn formatting_round_trips() {
        assert_eq!(format_minutes(45), "45m");
        assert_eq!(format_minutes(60), "1h");
        assert_eq!(format_minutes(90), "1h30");
        assert_eq!(format_minutes(605), "10h05");
        for m in [1, 45, 60, 90, 125, 600] {
            assert_eq!(parse_minutes(&format_minutes(m)), Ok(m), "{m}");
        }
    }
}
