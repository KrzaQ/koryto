//! What a day and a range of days look like for one person.

use std::collections::{BTreeMap, HashMap};

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use utoipa::ToSchema;

use super::{AppResult, bad};
use crate::db::{Activity, Db, Meal, Target, User, Weight};
use crate::domain::expenditure::Estimate;
use crate::domain::trend;

/// The longest range a single request may ask for.
pub const MAX_RANGE_DAYS: i64 = 1000;

#[derive(Debug, Clone, Default, Serialize, ToSchema, PartialEq)]
pub struct Totals {
    pub kcal: i32,
    /// Sum over the meals that have protein; null when none do
    pub protein_g: Option<i32>,
    pub meals: i32,
    pub meals_without_protein: i32,
    pub sport_minutes: i32,
    /// Sum over the sport entries that carry kcal; null when none do
    pub sport_kcal: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct DayView {
    pub day: NaiveDate,
    pub user_id: i32,
    pub meals: Vec<Meal>,
    pub weights: Vec<Weight>,
    pub activities: Vec<Activity>,
    pub totals: Totals,
    pub target: Option<Target>,
    /// kcal minus the target, on a logged day with a target
    pub balance: Option<i32>,
    pub logged: bool,
    /// The expenditure estimate as of this day (base plus the day's sport),
    /// and where it comes from
    pub expenditure: Estimate,
    /// kcal minus the estimate, on a logged day with an estimate
    pub balance_vs_expenditure: Option<i32>,
}

pub async fn day_view(
    db: &Db,
    user: &User,
    day: NaiveDate,
    include_voided: bool,
) -> AppResult<DayView> {
    let meals = db.list_meals(user.id, day, day, include_voided).await?;
    let weights = db.list_weights(user.id, day, day, include_voided).await?;
    let activities = db
        .list_activities(user.id, day, day, include_voided)
        .await?;
    let live: Vec<&Meal> = meals.iter().filter(|m| m.voided_at.is_none()).collect();
    let with_protein: Vec<i32> = live.iter().filter_map(|m| m.protein_g).collect();
    let sport: Vec<&Activity> = activities
        .iter()
        .filter(|a| a.voided_at.is_none())
        .collect();
    let sport_with_kcal: Vec<i32> = sport.iter().filter_map(|a| a.kcal).collect();
    let totals = Totals {
        kcal: live.iter().map(|m| m.kcal).sum(),
        protein_g: (!with_protein.is_empty()).then(|| with_protein.iter().sum()),
        meals: live.len() as i32,
        meals_without_protein: (live.len() - with_protein.len()) as i32,
        sport_minutes: sport.iter().map(|a| a.minutes).sum(),
        sport_kcal: (!sport_with_kcal.is_empty()).then(|| sport_with_kcal.iter().sum()),
    };
    let target = db.target_for(user.id, day).await?;
    let logged = !live.is_empty();
    let balance = match (&target, logged) {
        (Some(t), true) => Some(totals.kcal - t.kcal),
        _ => None,
    };
    let expenditure = super::stats::expenditure_on(db, user, day).await?;
    let balance_vs_expenditure = match (expenditure.kcal, logged) {
        (Some(e), true) => Some(totals.kcal - e),
        _ => None,
    };
    Ok(DayView {
        day,
        user_id: user.id,
        meals,
        weights,
        activities,
        totals,
        target,
        balance,
        logged,
        expenditure,
        balance_vs_expenditure,
    })
}

/// One calendar day of a range.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct DayRow {
    pub day: NaiveDate,
    /// At least one meal: unlogged days are gaps, never zeros
    pub logged: bool,
    pub kcal: Option<i32>,
    pub protein_g: Option<i32>,
    pub meals: i32,
    pub meals_without_protein: i32,
    pub sport_minutes: i32,
    /// Sum over the sport entries that carry kcal; null when none do
    pub sport_kcal: Option<i32>,
    /// The day's first reading, in grams
    pub weight_g: Option<i32>,
    /// The trend on a day with a reading
    pub trend_g: Option<i32>,
    pub target_kcal: Option<i32>,
    pub balance: Option<i32>,
}

pub fn check_range(from: NaiveDate, to: NaiveDate) -> AppResult<()> {
    if to < from {
        return Err(bad("to is before from"));
    }
    if (to - from).num_days() >= MAX_RANGE_DAYS {
        return Err(bad(format!("at most {MAX_RANGE_DAYS} days per request")));
    }
    Ok(())
}

/// Every day from `from` to `to` inclusive. The trend is computed over the
/// whole weight history up to `to`, so a range never changes it.
pub async fn days(db: &Db, user: &User, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<DayRow>> {
    check_range(from, to)?;
    let meals: HashMap<NaiveDate, _> = db
        .meal_day_totals(user.id, from, to)
        .await?
        .into_iter()
        .map(|t| (t.day, t))
        .collect();
    let sport: HashMap<NaiveDate, (i32, Option<i32>)> = db
        .activity_day_totals(user.id, from, to)
        .await?
        .into_iter()
        .map(|t| (t.day, (t.minutes, t.kcal)))
        .collect();
    let trend_by_day = trend_up_to(db, user, to).await?;
    let targets = db.list_targets(user.id).await?;

    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        let m = meals.get(&d);
        let target_kcal = targets
            .iter()
            .rev()
            .find(|t| t.valid_from <= d)
            .map(|t| t.kcal);
        let kcal = m.map(|m| m.kcal);
        let (weight_g, trend_g) = trend_by_day.get(&d).copied().unzip();
        out.push(DayRow {
            day: d,
            logged: m.is_some(),
            kcal,
            protein_g: m.and_then(|m| m.protein_g),
            meals: m.map(|m| m.meals).unwrap_or(0),
            meals_without_protein: m.map(|m| m.meals_without_protein).unwrap_or(0),
            sport_minutes: sport.get(&d).map(|s| s.0).unwrap_or(0),
            sport_kcal: sport.get(&d).and_then(|s| s.1),
            weight_g,
            trend_g,
            target_kcal,
            balance: kcal.zip(target_kcal).map(|(k, t)| k - t),
        });
        d += Duration::days(1);
    }
    Ok(out)
}

/// `(weight_g, trend_g)` for every day with a reading up to `to`.
pub async fn trend_up_to(
    db: &Db,
    user: &User,
    to: NaiveDate,
) -> AppResult<BTreeMap<NaiveDate, (i32, i32)>> {
    let Some(first) = db.first_day(user.id).await? else {
        return Ok(BTreeMap::new());
    };
    if first > to {
        return Ok(BTreeMap::new());
    }
    let points: Vec<(NaiveDate, i32)> = db
        .day_weights(user.id, first, to)
        .await?
        .into_iter()
        .map(|w| (w.day, w.weight_g))
        .collect();
    let trend = trend::trend(&points);
    Ok(points
        .into_iter()
        .zip(trend)
        .map(|((d, w), (_, t))| (d, (w, t)))
        .collect())
}
