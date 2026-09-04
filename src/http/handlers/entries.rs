//! Meals, weigh-ins and sport, and the day views over them.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::NaiveDate;
use serde::Deserialize;
use utoipa::IntoParams;

use super::dto::{ActivityDto, DayDto, DaysDto, MealDto, WeightDto};
use crate::app::entries::{
    self, ActivityInput, ActivityPatchInput, MealInput, MealPatchInput, WeightInput,
    WeightPatchInput,
};
use crate::app::{day as appday, scope, time};
use crate::db::EntryKind;
use crate::domain::day;
use crate::http::AppState;
use crate::http::auth::Principal;
use crate::http::error::{ApiResult, ErrorBody};

#[derive(Deserialize, IntoParams)]
pub struct DayQuery {
    /// Whose day; default the caller
    pub user: Option<i32>,
    /// YYYY-MM-DD; default today on that person's clock
    pub date: Option<NaiveDate>,
    #[serde(default)]
    pub include_voided: bool,
}

#[derive(Deserialize, IntoParams)]
pub struct RangeQuery {
    pub user: Option<i32>,
    pub from: NaiveDate,
    pub to: NaiveDate,
    #[serde(default)]
    pub include_voided: bool,
}

async fn today_for(st: &AppState, user: &crate::db::User) -> ApiResult<NaiveDate> {
    let tz = time::current_zone(&st.db, user).await?;
    Ok(day::today(tz, user.day_boundary_minutes))
}

#[utoipa::path(get, path = "/api/day", tag = "days", params(DayQuery),
    responses((status = 200, body = DayDto), (status = 403, body = ErrorBody)))]
pub async fn day(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<DayQuery>,
) -> ApiResult<Json<DayDto>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    let date = match q.date {
        Some(d) => d,
        None => today_for(&st, &user).await?,
    };
    Ok(Json(
        appday::day_view(&st.db, &user, date, q.include_voided)
            .await?
            .into(),
    ))
}

#[utoipa::path(get, path = "/api/days", tag = "days", params(RangeQuery),
    responses((status = 200, body = DaysDto), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn days(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<DaysDto>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    let days = appday::days(&st.db, &user, q.from, q.to).await?;
    Ok(Json(DaysDto {
        user_id: user.id,
        from: q.from,
        to: q.to,
        days,
    }))
}

// ----- meals ---------------------------------------------------------------

#[utoipa::path(get, path = "/api/meals", tag = "meals", params(RangeQuery),
    responses((status = 200, body = Vec<MealDto>), (status = 403, body = ErrorBody)))]
pub async fn list_meals(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<MealDto>>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    appday::check_range(q.from, q.to)?;
    let rows = st
        .db
        .list_meals(user.id, q.from, q.to, q.include_voided)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(post, path = "/api/meals", tag = "meals", request_body = MealInput,
    responses((status = 201, body = Vec<MealDto>), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn create_meals(
    State(st): State<AppState>,
    p: Principal,
    Json(input): Json<MealInput>,
) -> ApiResult<(StatusCode, Json<Vec<MealDto>>)> {
    p.require_write()?;
    let rows = entries::log_meals(&st.db, p.user(), p.via(), input).await?;
    Ok((
        StatusCode::CREATED,
        Json(rows.into_iter().map(Into::into).collect()),
    ))
}

#[utoipa::path(patch, path = "/api/meals/{id}", tag = "meals", params(("id" = i32, Path)), request_body = MealPatchInput,
    responses((status = 200, body = MealDto), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn update_meal(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<MealPatchInput>,
) -> ApiResult<Json<MealDto>> {
    p.require_edit()?;
    Ok(Json(
        entries::update_meal(&st.db, p.user(), id, input)
            .await?
            .into(),
    ))
}

#[utoipa::path(post, path = "/api/meals/{id}/void", tag = "meals", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn void_meal(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    entries::void(&st.db, p.user(), EntryKind::Meal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/meals/{id}/unvoid", tag = "meals", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn unvoid_meal(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    entries::unvoid(&st.db, p.user(), EntryKind::Meal, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- weights -------------------------------------------------------------

#[utoipa::path(get, path = "/api/weights", tag = "weights", params(RangeQuery),
    responses((status = 200, body = Vec<WeightDto>), (status = 403, body = ErrorBody)))]
pub async fn list_weights(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<WeightDto>>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    appday::check_range(q.from, q.to)?;
    let rows = st
        .db
        .list_weights(user.id, q.from, q.to, q.include_voided)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(post, path = "/api/weights", tag = "weights", request_body = WeightInput,
    responses((status = 201, body = WeightDto), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn create_weight(
    State(st): State<AppState>,
    p: Principal,
    Json(input): Json<WeightInput>,
) -> ApiResult<(StatusCode, Json<WeightDto>)> {
    p.require_write()?;
    let w = entries::log_weight(&st.db, p.user(), p.via(), input).await?;
    Ok((StatusCode::CREATED, Json(w.into())))
}

#[utoipa::path(patch, path = "/api/weights/{id}", tag = "weights", params(("id" = i32, Path)), request_body = WeightPatchInput,
    responses((status = 200, body = WeightDto), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn update_weight(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<WeightPatchInput>,
) -> ApiResult<Json<WeightDto>> {
    p.require_edit()?;
    Ok(Json(
        entries::update_weight(&st.db, p.user(), id, input)
            .await?
            .into(),
    ))
}

#[utoipa::path(post, path = "/api/weights/{id}/void", tag = "weights", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn void_weight(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    entries::void(&st.db, p.user(), EntryKind::Weight, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/weights/{id}/unvoid", tag = "weights", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn unvoid_weight(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    entries::unvoid(&st.db, p.user(), EntryKind::Weight, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- activities ----------------------------------------------------------

#[utoipa::path(get, path = "/api/activities", tag = "activities", params(RangeQuery),
    responses((status = 200, body = Vec<ActivityDto>), (status = 403, body = ErrorBody)))]
pub async fn list_activities(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<RangeQuery>,
) -> ApiResult<Json<Vec<ActivityDto>>> {
    let user = scope::member(&st.db, p.user(), q.user).await?;
    appday::check_range(q.from, q.to)?;
    let rows = st
        .db
        .list_activities(user.id, q.from, q.to, q.include_voided)
        .await?;
    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

#[utoipa::path(post, path = "/api/activities", tag = "activities", request_body = ActivityInput,
    responses((status = 201, body = ActivityDto), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn create_activity(
    State(st): State<AppState>,
    p: Principal,
    Json(input): Json<ActivityInput>,
) -> ApiResult<(StatusCode, Json<ActivityDto>)> {
    p.require_write()?;
    let a = entries::log_activity(&st.db, p.user(), p.via(), input).await?;
    Ok((StatusCode::CREATED, Json(a.into())))
}

#[utoipa::path(patch, path = "/api/activities/{id}", tag = "activities", params(("id" = i32, Path)), request_body = ActivityPatchInput,
    responses((status = 200, body = ActivityDto), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody)))]
pub async fn update_activity(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<ActivityPatchInput>,
) -> ApiResult<Json<ActivityDto>> {
    p.require_edit()?;
    Ok(Json(
        entries::update_activity(&st.db, p.user(), id, input)
            .await?
            .into(),
    ))
}

#[utoipa::path(post, path = "/api/activities/{id}/void", tag = "activities", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn void_activity(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    entries::void(&st.db, p.user(), EntryKind::Activity, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/api/activities/{id}/unvoid", tag = "activities", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn unvoid_activity(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    entries::unvoid(&st.db, p.user(), EntryKind::Activity, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
