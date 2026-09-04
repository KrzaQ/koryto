//! The household's well-known foods.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::Deserialize;
use utoipa::IntoParams;

use super::dto::{FoodDto, FoodInput, FoodPatchInput};
use crate::app::scope;
use crate::db::{FoodPatch, NewFood};
use crate::http::AppState;
use crate::http::auth::Principal;
use crate::http::error::{ApiError, ApiResult, ErrorBody};

#[derive(Deserialize, IntoParams)]
pub struct FoodQuery {
    /// Fragment of the name or an alias; empty lists everything
    pub q: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
}

fn clean_aliases(aliases: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for a in aliases {
        let a = a.trim().to_string();
        if !a.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(&a)) {
            out.push(a);
        }
    }
    out
}

fn check_numbers(kcal: i32, protein_g: Option<i32>) -> ApiResult<()> {
    if kcal < 0 || protein_g.is_some_and(|p| p < 0) {
        return Err(ApiError::bad_request(
            "kcal and protein_g cannot be negative",
        ));
    }
    Ok(())
}

#[utoipa::path(get, path = "/api/foods", tag = "foods", params(FoodQuery),
    responses((status = 200, body = Vec<FoodDto>), (status = 403, body = ErrorBody)))]
pub async fn list_foods(
    State(st): State<AppState>,
    p: Principal,
    Query(q): Query<FoodQuery>,
) -> ApiResult<Json<Vec<FoodDto>>> {
    let household = scope::household_of(p.user())?;
    let foods = st
        .db
        .search_foods(household, q.q.as_deref().unwrap_or(""), q.include_archived)
        .await?;
    Ok(Json(foods.into_iter().map(Into::into).collect()))
}

#[utoipa::path(post, path = "/api/foods", tag = "foods", request_body = FoodInput,
    responses((status = 201, body = FoodDto), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_food(
    State(st): State<AppState>,
    p: Principal,
    Json(input): Json<FoodInput>,
) -> ApiResult<(StatusCode, Json<FoodDto>)> {
    p.require_write()?;
    let household = scope::household_of(p.user())?;
    if input.name.trim().is_empty() || input.portion.trim().is_empty() {
        return Err(ApiError::bad_request("name and portion cannot be empty"));
    }
    check_numbers(input.kcal, input.protein_g)?;
    let f = st
        .db
        .insert_food(NewFood {
            household_id: household,
            name: input.name,
            aliases: clean_aliases(input.aliases),
            portion: input.portion,
            kcal: input.kcal,
            protein_g: input.protein_g,
            created_by: p.user().id,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(FoodDto::from(f, 0))))
}

#[utoipa::path(patch, path = "/api/foods/{id}", tag = "foods", params(("id" = i32, Path)), request_body = FoodPatchInput,
    responses((status = 200, body = FoodDto), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn update_food(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<FoodPatchInput>,
) -> ApiResult<Json<FoodDto>> {
    p.require_edit()?;
    let current = scope::food(&st.db, p.user(), id).await?;
    if input.name.as_deref().is_some_and(|n| n.trim().is_empty())
        || input
            .portion
            .as_deref()
            .is_some_and(|n| n.trim().is_empty())
    {
        return Err(ApiError::bad_request("name and portion cannot be empty"));
    }
    check_numbers(
        input.kcal.unwrap_or(current.kcal),
        input.protein_g.flatten(),
    )?;
    let f = st
        .db
        .update_food(
            id,
            FoodPatch {
                name: input.name,
                aliases: input.aliases.map(clean_aliases),
                portion: input.portion.map(|p| p.trim().to_string()),
                kcal: input.kcal,
                protein_g: input.protein_g,
            },
        )
        .await?;
    Ok(Json(FoodDto::from(f, uses(&st, id).await?)))
}

async fn uses(st: &AppState, id: i32) -> ApiResult<i64> {
    let f = st.db.get_food(id).await?;
    Ok(st
        .db
        .search_foods(f.household_id, &f.name, true)
        .await?
        .into_iter()
        .find(|x| x.food.id == id)
        .map(|x| x.uses)
        .unwrap_or(0))
}

#[utoipa::path(post, path = "/api/foods/{id}/archive", tag = "foods", params(("id" = i32, Path)),
    responses((status = 200, body = FoodDto), (status = 404, body = ErrorBody)))]
pub async fn archive_food(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<Json<FoodDto>> {
    p.require_edit()?;
    scope::food(&st.db, p.user(), id).await?;
    let f = st.db.set_food_archived(id, true).await?;
    Ok(Json(FoodDto::from(f, uses(&st, id).await?)))
}

#[utoipa::path(post, path = "/api/foods/{id}/unarchive", tag = "foods", params(("id" = i32, Path)),
    responses((status = 200, body = FoodDto), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn unarchive_food(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<Json<FoodDto>> {
    p.require_edit()?;
    scope::food(&st.db, p.user(), id).await?;
    let f = st.db.set_food_archived(id, false).await?;
    Ok(Json(FoodDto::from(f, uses(&st, id).await?)))
}
