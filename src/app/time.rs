//! Resolving when an entry happened into the instant, zone and day the
//! tables store.

use anyhow::Result;
use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;

use crate::db::{Db, DbResult, User};
use crate::domain::day;

/// The stored time fields of an entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub instant: DateTime<Utc>,
    pub timezone: String,
    pub day: NaiveDate,
    pub day_override: bool,
}

/// An explicit zone wins over the location history; an explicit day wins
/// over the computed one and marks the row as overridden. `instant` defaults
/// to now.
pub async fn resolve(
    db: &Db,
    user: &User,
    instant: Option<DateTime<Utc>>,
    zone: Option<Tz>,
    day_override: Option<NaiveDate>,
) -> DbResult<Resolved> {
    let instant = instant.unwrap_or_else(Utc::now);
    let tz = match zone {
        Some(tz) => tz,
        None => db
            .zone_at(user.id, instant)
            .await?
            .parse()
            .unwrap_or(chrono_tz::UTC),
    };
    let computed = day::day_of(instant, tz, user.day_boundary_minutes);
    Ok(Resolved {
        instant,
        timezone: tz.name().to_string(),
        day: day_override.unwrap_or(computed),
        day_override: day_override.is_some_and(|d| d != computed),
    })
}

/// The zone a user is in right now.
pub async fn current_zone(db: &Db, user: &User) -> DbResult<Tz> {
    Ok(db
        .zone_at(user.id, Utc::now())
        .await?
        .parse()
        .unwrap_or(chrono_tz::UTC))
}

/// Recompute zone and day for every non-overridden entry of a user from the
/// location history and the day boundary. Returns how many rows changed.
pub async fn recompute_days(db: &Db, user: &User) -> Result<usize> {
    let history: Vec<(DateTime<Utc>, Tz)> = db
        .list_locations(user.id)
        .await?
        .into_iter()
        .filter_map(|l| l.timezone.parse().ok().map(|tz| (l.valid_from, tz)))
        .collect();
    let boundary = user.day_boundary_minutes;
    Ok(db
        .recompute_days(user.id, |instant| {
            let tz = day::zone_at(&history, instant).unwrap_or(chrono_tz::UTC);
            (tz.name().to_string(), day::day_of(instant, tz, boundary))
        })
        .await?)
}
