//! Numbers for the charts. Everything here is a view over /api/days plus the
//! expenditure estimate; the frontend never recomputes a trend.

use axum::Json;
use axum::extract::{Query, State};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use crate::app::stats::{self, Week};
use crate::app::{day as appday, scope};
use crate::domain::expenditure::{Basis, Estimate};
use crate::http::AppState;
use crate::http::auth::Principal;
use crate::http::error::{ApiResult, ErrorBody};

#[derive(Deserialize, IntoParams)]
pub struct StatsQuery {
    pub user: Option<i32>,
    pub from: NaiveDate,
    pub to: NaiveDate,
}

#[derive(Serialize, ToSchema)]
pub struct WeightPoint {
    pub day: NaiveDate,
    pub weight_g: i32,
    pub trend_g: i32,
}

#[derive(Serialize, ToSchema)]
pub struct WeightStats {
    pub user_id: i32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub points: Vec<WeightPoint>,
    /// The goal weight of the target in force on `to`, grams
    pub goal_g: Option<i32>,
}

#[utoipa::path(get, path = "/api/stats/weight", tag = "stats", params(StatsQuery),
    responses((status = 200, body = WeightStats), (status = 400, body = ErrorBody)))]
pub async fn weight(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Json<WeightStats>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    appday::check_range(q.from, q.to)?;
    let trend = appday::trend_up_to(&st.db, &user, q.to).await?;
    let points = trend
        .range(q.from..=q.to)
        .map(|(d, (w, t))| WeightPoint {
            day: *d,
            weight_g: *w,
            trend_g: *t,
        })
        .collect();
    let goal_g = st
        .db
        .target_for(user.id, q.to)
        .await?
        .and_then(|t| t.weight_g);
    Ok(Json(WeightStats {
        user_id: user.id,
        from: q.from,
        to: q.to,
        points,
        goal_g,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct ExpenditurePoint {
    pub day: NaiveDate,
    pub kcal: Option<i32>,
    pub basis: Basis,
    pub logged_days: usize,
}

#[derive(Serialize, ToSchema)]
pub struct ExpenditureStats {
    pub user_id: i32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub days: Vec<ExpenditurePoint>,
    /// As of `to`
    pub latest: Estimate,
}

#[utoipa::path(get, path = "/api/stats/expenditure", tag = "stats", params(StatsQuery),
    responses((status = 200, body = ExpenditureStats), (status = 400, body = ErrorBody)))]
pub async fn expenditure(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Json<ExpenditureStats>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    let series = stats::expenditure_series(&st.db, &user, q.from, q.to).await?;
    let latest = series
        .last()
        .map(|(_, e)| e.clone())
        .unwrap_or_else(|| Estimate {
            kcal: None,
            basis: Basis::None,
            logged_days: 0,
            weight_span_days: 0,
            seed_kcal: None,
        });
    Ok(Json(ExpenditureStats {
        user_id: user.id,
        from: q.from,
        to: q.to,
        days: series
            .into_iter()
            .map(|(day, e)| ExpenditurePoint {
                day,
                kcal: e.kcal,
                basis: e.basis,
                logged_days: e.logged_days,
            })
            .collect(),
        latest,
    }))
}

#[derive(Serialize, ToSchema)]
pub struct WeeklyStats {
    pub user_id: i32,
    pub from: NaiveDate,
    pub to: NaiveDate,
    pub weeks: Vec<Week>,
}

#[utoipa::path(get, path = "/api/stats/weekly", tag = "stats", params(StatsQuery),
    responses((status = 200, body = WeeklyStats), (status = 400, body = ErrorBody)))]
pub async fn weekly(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Json<WeeklyStats>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    let weeks = stats::weekly(&st.db, &user, q.from, q.to).await?;
    Ok(Json(WeeklyStats {
        user_id: user.id,
        from: q.from,
        to: q.to,
        weeks,
    }))
}
