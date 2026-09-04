//! Summaries over a range: averages, the weight trend and the expenditure
//! estimate. What the MCP summary and the charts are made of.

use chrono::{Duration, NaiveDate};
use serde::Serialize;
use utoipa::ToSchema;

use super::AppResult;
use super::day::{self, DayRow};
use crate::db::{Db, User};
use crate::domain::expenditure::{self, Estimate, Profile, Sex, WINDOW_DAYS};

pub fn profile_of(user: &User) -> Profile {
    Profile {
        height_mm: user.height_mm,
        born_on: user.born_on,
        sex: user.sex.as_deref().and_then(|s| s.parse::<Sex>().ok()),
        activity_factor: user.activity_factor,
    }
}

fn window_rows(rows: &[DayRow]) -> Vec<expenditure::DayRow> {
    rows.iter()
        .map(|r| expenditure::DayRow {
            day: r.day,
            kcal: r.kcal,
            trend_g: r.trend_g,
        })
        .collect()
}

/// The estimate as of `end`, over the window ending there.
pub async fn expenditure_on(db: &Db, user: &User, end: NaiveDate) -> AppResult<Estimate> {
    let start = end - Duration::days(WINDOW_DAYS - 1);
    let rows = day::days(db, user, start, end).await?;
    let latest = day::trend_up_to(db, user, end)
        .await?
        .values()
        .last()
        .map(|(_, t)| *t);
    Ok(expenditure::estimate(
        &window_rows(&rows),
        latest,
        end,
        &profile_of(user),
    ))
}

/// One estimate per day of the range; each day looks back over its own
/// window. Built from one wide query, not one per day.
pub async fn expenditure_series(
    db: &Db,
    user: &User,
    from: NaiveDate,
    to: NaiveDate,
) -> AppResult<Vec<(NaiveDate, Estimate)>> {
    day::check_range(from, to)?;
    let start = from - Duration::days(WINDOW_DAYS - 1);
    let rows = day::days(db, user, start, to).await?;
    let all = window_rows(&rows);
    let trend = day::trend_up_to(db, user, to).await?;
    let profile = profile_of(user);
    let mut out = Vec::new();
    let mut d = from;
    while d <= to {
        let window_start = d - Duration::days(WINDOW_DAYS - 1);
        let window: Vec<expenditure::DayRow> = all
            .iter()
            .filter(|r| r.day >= window_start && r.day <= d)
            .copied()
            .collect();
        let latest = trend.range(..=d).next_back().map(|(_, (_, t))| *t);
        out.push((d, expenditure::estimate(&window, latest, d, &profile)));
        d += Duration::days(1);
    }
    Ok(out)
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WeightSummary {
    /// First and last readings in the range, grams
    pub first_g: Option<i32>,
    pub last_g: Option<i32>,
    pub trend_first_g: Option<i32>,
    pub trend_last_g: Option<i32>,
    pub trend_delta_g: Option<i32>,
    pub readings: usize,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Summary {
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: usize,
    pub logged_days: usize,
    /// Over logged days
    pub mean_kcal: Option<i32>,
    /// Over logged days that have any protein
    pub mean_protein_g: Option<i32>,
    pub total_kcal: i32,
    pub sport_minutes: i32,
    /// Mean balance against the target over logged days with a target
    pub mean_balance: Option<i32>,
    pub weight: WeightSummary,
    /// As of the last day of the range
    pub expenditure: Estimate,
    pub rows: Vec<DayRow>,
}

pub async fn summary(db: &Db, user: &User, from: NaiveDate, to: NaiveDate) -> AppResult<Summary> {
    let rows = day::days(db, user, from, to).await?;
    let logged: Vec<&DayRow> = rows.iter().filter(|r| r.logged).collect();
    let mean = |xs: Vec<i64>| -> Option<i32> {
        (!xs.is_empty()).then(|| {
            crate::domain::trend::round_div(xs.iter().sum::<i64>(), xs.len() as i64) as i32
        })
    };
    let weights: Vec<&DayRow> = rows.iter().filter(|r| r.weight_g.is_some()).collect();
    let weight = WeightSummary {
        first_g: weights.first().and_then(|r| r.weight_g),
        last_g: weights.last().and_then(|r| r.weight_g),
        trend_first_g: weights.first().and_then(|r| r.trend_g),
        trend_last_g: weights.last().and_then(|r| r.trend_g),
        trend_delta_g: match (weights.first(), weights.last()) {
            (Some(a), Some(b)) if weights.len() > 1 => a.trend_g.zip(b.trend_g).map(|(x, y)| y - x),
            _ => None,
        },
        readings: weights.len(),
    };
    let expenditure = expenditure_on(db, user, to).await?;
    Ok(Summary {
        from,
        to,
        days: rows.len(),
        logged_days: logged.len(),
        mean_kcal: mean(
            logged
                .iter()
                .filter_map(|r| r.kcal.map(i64::from))
                .collect(),
        ),
        mean_protein_g: mean(
            logged
                .iter()
                .filter_map(|r| r.protein_g.map(i64::from))
                .collect(),
        ),
        total_kcal: logged.iter().filter_map(|r| r.kcal).sum(),
        sport_minutes: rows.iter().map(|r| r.sport_minutes).sum(),
        mean_balance: mean(
            logged
                .iter()
                .filter_map(|r| r.balance.map(i64::from))
                .collect(),
        ),
        weight,
        expenditure,
        rows,
    })
}

/// One ISO week of a range, for the weekly balance chart.
#[derive(Debug, Clone, Serialize, ToSchema, PartialEq)]
pub struct Week {
    /// ISO week, e.g. 2026-W36
    pub week: String,
    /// The Monday
    pub start: NaiveDate,
    /// Days of the week that fall inside the requested range
    pub days: usize,
    pub logged_days: usize,
    pub mean_kcal: Option<i32>,
    pub total_kcal: i32,
    pub sport_minutes: i32,
    /// Mean intake minus target over logged days with a target
    pub mean_balance_vs_target: Option<i32>,
    /// Mean expenditure estimate over the week's days that have one
    pub mean_expenditure: Option<i32>,
    /// Mean intake minus mean expenditure, when both exist
    pub mean_balance_vs_expenditure: Option<i32>,
}

pub async fn weekly(db: &Db, user: &User, from: NaiveDate, to: NaiveDate) -> AppResult<Vec<Week>> {
    use chrono::Datelike;
    let rows = day::days(db, user, from, to).await?;
    let estimates = expenditure_series(db, user, from, to).await?;
    let mean = |xs: &[i64]| -> Option<i32> {
        (!xs.is_empty()).then(|| {
            crate::domain::trend::round_div(xs.iter().sum::<i64>(), xs.len() as i64) as i32
        })
    };
    let mut out: Vec<Week> = Vec::new();
    for row in &rows {
        let iso = row.day.iso_week();
        let label = format!("{}-W{:02}", iso.year(), iso.week());
        let monday = row.day - Duration::days(i64::from(row.day.weekday().num_days_from_monday()));
        if out.last().is_none_or(|w| w.week != label) {
            out.push(Week {
                week: label,
                start: monday,
                days: 0,
                logged_days: 0,
                mean_kcal: None,
                total_kcal: 0,
                sport_minutes: 0,
                mean_balance_vs_target: None,
                mean_expenditure: None,
                mean_balance_vs_expenditure: None,
            });
        }
        let w = out.last_mut().expect("pushed");
        w.days += 1;
        w.sport_minutes += row.sport_minutes;
        if row.logged {
            w.logged_days += 1;
            w.total_kcal += row.kcal.unwrap_or(0);
        }
    }
    // Second pass for the means, per week.
    for w in &mut out {
        let in_week = |d: NaiveDate| d >= w.start && d < w.start + Duration::days(7);
        let kcal: Vec<i64> = rows
            .iter()
            .filter(|r| in_week(r.day))
            .filter_map(|r| r.kcal.map(i64::from))
            .collect();
        let balance: Vec<i64> = rows
            .iter()
            .filter(|r| in_week(r.day))
            .filter_map(|r| r.balance.map(i64::from))
            .collect();
        let exp: Vec<i64> = estimates
            .iter()
            .filter(|(d, _)| in_week(*d))
            .filter_map(|(_, e)| e.kcal.map(i64::from))
            .collect();
        w.mean_kcal = mean(&kcal);
        w.mean_balance_vs_target = mean(&balance);
        w.mean_expenditure = mean(&exp);
        w.mean_balance_vs_expenditure = w.mean_kcal.zip(w.mean_expenditure).map(|(k, e)| k - e);
    }
    Ok(out)
}
