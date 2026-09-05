//! Sport kinds and their MET rates: what a session costs per kilogram per
//! hour. Reference data, shared by every household, editable with `edit`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use rust_decimal::Decimal;
use serde::Deserialize;
use utoipa::IntoParams;

use super::dto::{ActivityKindDto, ActivityKindInput, ActivityKindPatchInput};
use crate::db::{ActivityKindPatch, NewActivityKind};
use crate::http::AppState;
use crate::http::auth::Principal;
use crate::http::error::{ApiError, ApiResult, ErrorBody};

#[derive(Deserialize, IntoParams)]
pub struct KindQuery {
    /// Fragment of the name or an alias; empty lists everything
    pub q: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
}

/// The Compendium's range: 1.0 is lying still and 23 is running a 3-minute
/// kilometre, so anything outside says the number is not a MET.
fn parse_met(input: &str) -> ApiResult<Decimal> {
    let met: Decimal = input
        .trim()
        .replace(',', ".")
        .parse()
        .map_err(|_| ApiError::bad_request("met must be a number like 3.5"))?;
    if met < Decimal::ONE || met > Decimal::from(25) {
        return Err(ApiError::bad_request("met must be between 1.0 and 25.0"));
    }
    Ok(met)
}

fn clean_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in aliases {
        let a = a.trim().to_lowercase();
        if !a.is_empty() && !out.contains(&a) {
            out.push(a);
        }
    }
    out
}

async fn uses(st: &AppState, id: i32) -> ApiResult<i64> {
    Ok(st.db.activity_kind_uses(id).await?)
}

#[utoipa::path(get, path = "/api/activity-kinds", tag = "activity-kinds", params(KindQuery),
    responses((status = 200, body = Vec<ActivityKindDto>)))]
pub async fn list_kinds(
    State(st): State<AppState>,
    _p: Principal,
    Query(q): Query<KindQuery>,
) -> ApiResult<Json<Vec<ActivityKindDto>>> {
    let kinds = st
        .db
        .search_activity_kinds(q.q.as_deref().unwrap_or(""), q.include_archived)
        .await?;
    let mut out = Vec::with_capacity(kinds.len());
    for k in kinds {
        let n = uses(&st, k.id).await?;
        out.push(ActivityKindDto::from(k, n));
    }
    Ok(Json(out))
}

#[utoipa::path(post, path = "/api/activity-kinds", tag = "activity-kinds", request_body = ActivityKindInput,
    responses((status = 201, body = ActivityKindDto), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_kind(
    State(st): State<AppState>,
    p: Principal,
    Json(input): Json<ActivityKindInput>,
) -> ApiResult<(StatusCode, Json<ActivityKindDto>)> {
    p.require_write()?;
    if input.name.trim().is_empty() {
        return Err(ApiError::bad_request("name cannot be empty"));
    }
    let k = st
        .db
        .insert_activity_kind(NewActivityKind {
            name: input.name,
            aliases: clean_aliases(input.aliases),
            met: parse_met(&input.met)?,
            note: input.note,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(ActivityKindDto::from(k, 0))))
}

#[utoipa::path(patch, path = "/api/activity-kinds/{id}", tag = "activity-kinds",
    params(("id" = i32, Path)), request_body = ActivityKindPatchInput,
    responses((status = 200, body = ActivityKindDto), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn update_kind(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<ActivityKindPatchInput>,
) -> ApiResult<Json<ActivityKindDto>> {
    p.require_edit()?;
    if input.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
        return Err(ApiError::bad_request("name cannot be empty"));
    }
    let met = input.met.as_deref().map(parse_met).transpose()?;
    let k = st
        .db
        .update_activity_kind(
            id,
            ActivityKindPatch {
                name: input.name,
                aliases: input.aliases.map(clean_aliases),
                met,
                note: input.note,
            },
        )
        .await?;
    let n = uses(&st, id).await?;
    Ok(Json(ActivityKindDto::from(k, n)))
}

#[utoipa::path(post, path = "/api/activity-kinds/{id}/archive", tag = "activity-kinds",
    params(("id" = i32, Path)),
    responses((status = 200, body = ActivityKindDto), (status = 404, body = ErrorBody)))]
pub async fn archive_kind(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<Json<ActivityKindDto>> {
    p.require_edit()?;
    let k = st.db.set_activity_kind_archived(id, true).await?;
    let n = uses(&st, id).await?;
    Ok(Json(ActivityKindDto::from(k, n)))
}

#[utoipa::path(post, path = "/api/activity-kinds/{id}/unarchive", tag = "activity-kinds",
    params(("id" = i32, Path)),
    responses((status = 200, body = ActivityKindDto), (status = 404, body = ErrorBody)))]
pub async fn unarchive_kind(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<Json<ActivityKindDto>> {
    p.require_edit()?;
    let k = st.db.set_activity_kind_archived(id, false).await?;
    let n = uses(&st, id).await?;
    Ok(Json(ActivityKindDto::from(k, n)))
}
