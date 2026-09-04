//! A point in time as people type it: either an instant with an offset
//! (RFC 3339) or a wall-clock time without one, which only becomes an instant
//! once we know whose clock it is.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Instant(DateTime<Utc>),
    /// Wall-clock time; resolved against a zone by [`When::resolve`].
    Wall(NaiveDateTime),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("cannot read {0:?} as a time; use YYYY-MM-DD, YYYY-MM-DD HH:MM or RFC 3339")]
pub struct WhenError(String);

impl When {
    /// Instants stay as they are; wall-clock times are read in `tz`. A time
    /// in a DST gap moves forward by the gap; a repeated hour takes its first
    /// occurrence. The app resolves through [`resolve_wall`] directly.
    #[cfg(test)]
    pub fn resolve(self, tz: Tz) -> DateTime<Utc> {
        match self {
            Self::Instant(i) => i,
            Self::Wall(w) => resolve_wall(w, tz),
        }
    }
}

pub fn resolve_wall(w: NaiveDateTime, tz: Tz) -> DateTime<Utc> {
    match tz.from_local_datetime(&w) {
        LocalResult::Single(d) | LocalResult::Ambiguous(d, _) => d.with_timezone(&Utc),
        LocalResult::None => {
            let later = w + chrono::Duration::hours(1);
            tz.from_local_datetime(&later)
                .earliest()
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|| Utc.from_utc_datetime(&w))
        }
    }
}

/// Wall-clock parsing shared by the CLI, the API and MCP: `YYYY-MM-DD`,
/// `YYYY-MM-DD HH:MM`, with an optional `T` and optional seconds.
pub fn parse_wall(s: &str) -> Option<NaiveDateTime> {
    for fmt in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .ok()?
        .and_hms_opt(9, 0, 0)
}

impl FromStr for When {
    type Err = WhenError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if let Ok(d) = DateTime::parse_from_rfc3339(s) {
            return Ok(Self::Instant(d.with_timezone(&Utc)));
        }
        parse_wall(s)
            .map(Self::Wall)
            .ok_or_else(|| WhenError(s.to_string()))
    }
}

impl fmt::Display for When {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Instant(i) => write!(f, "{}", i.to_rfc3339()),
            Self::Wall(w) => write!(f, "{}", w.format("%Y-%m-%d %H:%M")),
        }
    }
}

impl<'de> Deserialize<'de> for When {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Warsaw;

    #[test]
    fn instants_pass_through_and_walls_resolve() {
        let i: When = "2026-08-04T08:45:00Z".parse().unwrap();
        assert_eq!(i.resolve(Warsaw).to_rfc3339(), "2026-08-04T08:45:00+00:00");
        let w: When = "2026-08-04 10:45".parse().unwrap();
        assert_eq!(w.resolve(Warsaw), i.resolve(Warsaw));
        let t: When = "2026-08-04T10:45:00".parse().unwrap();
        assert_eq!(t, w);
        let d: When = "2026-08-04".parse().unwrap();
        assert_eq!(d, "2026-08-04 09:00".parse().unwrap());
        let off: When = "2026-08-04T10:45:00+02:00".parse().unwrap();
        assert_eq!(off, i);
        assert!("yesterday".parse::<When>().is_err());
    }

    #[test]
    fn dst_edges_are_deterministic() {
        // 2026-03-29 02:30 does not exist in Warsaw; it becomes 03:30.
        let gap = resolve_wall("2026-03-29 02:30".parse::<When>().unwrap().wall(), Warsaw);
        assert_eq!(gap.to_rfc3339(), "2026-03-29T01:30:00+00:00");
        // 2026-10-25 02:30 happens twice; the first (summer time) wins.
        let twice = resolve_wall("2026-10-25 02:30".parse::<When>().unwrap().wall(), Warsaw);
        assert_eq!(twice.to_rfc3339(), "2026-10-25T00:30:00+00:00");
    }

    impl When {
        fn wall(self) -> NaiveDateTime {
            match self {
                Self::Wall(w) => w,
                Self::Instant(_) => panic!("expected wall-clock"),
            }
        }
    }
}
