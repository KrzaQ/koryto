//! A person's settings: profile, location history, targets. Any household
//! member may edit any member's; the household check is the only gate.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;

use super::dto::{
    LocationDto, LocationInput, LocationPatchInput, ProfilePatchInput, TargetDto, TargetInput,
    TargetPatchInput, UserDto,
};
use crate::app::{scope, time};
use crate::db::{NewTarget, ProfilePatch, TargetPatch};
use crate::domain::day::{self, parse_tz};
use crate::domain::expenditure::Sex;
use crate::domain::units::parse_kg;
use crate::http::AppState;
use crate::http::auth::Principal;
use crate::http::error::{ApiError, ApiResult, ErrorBody};

#[utoipa::path(patch, path = "/api/users/{id}/profile", tag = "profile", params(("id" = i32, Path)), request_body = ProfilePatchInput,
    responses((status = 200, body = UserDto), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn update_profile(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<ProfilePatchInput>,
) -> ApiResult<Json<UserDto>> {
    p.require_edit()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    if let Some(b) = input.day_boundary_minutes
        && !(0..1440).contains(&b)
    {
        return Err(ApiError::bad_request(
            "day_boundary_minutes must be 0 to 1439",
        ));
    }
    if let Some(Some(h)) = input.height_mm
        && !(500..3000).contains(&h)
    {
        return Err(ApiError::bad_request("height_mm must be 500 to 2999"));
    }
    let sex = match input.sex {
        Some(Some(s)) => Some(Some(
            s.parse::<Sex>()
                .map(|s| match s {
                    Sex::Female => "female".to_string(),
                    Sex::Male => "male".to_string(),
                })
                .map_err(ApiError::bad_request)?,
        )),
        other => other,
    };
    let activity_factor = input
        .activity_factor
        .as_deref()
        .map(|a| {
            a.trim()
                .replace(',', ".")
                .parse::<rust_decimal::Decimal>()
                .map_err(|_| ApiError::bad_request("activity_factor must be a number like 1.4"))
        })
        .transpose()?;
    if let Some(a) = activity_factor
        && !(rust_decimal::Decimal::ONE..=rust_decimal::Decimal::new(250, 2)).contains(&a)
    {
        return Err(ApiError::bad_request(
            "activity_factor must be 1.00 to 2.50",
        ));
    }
    let boundary_changed = input
        .day_boundary_minutes
        .is_some_and(|b| b != user.day_boundary_minutes);
    let updated = st
        .db
        .update_profile(
            user.id,
            ProfilePatch {
                name: input
                    .name
                    .map(|n| n.trim().to_string())
                    .filter(|n| !n.is_empty()),
                day_boundary_minutes: input.day_boundary_minutes,
                height_mm: input.height_mm,
                born_on: input.born_on,
                sex,
                activity_factor,
            },
        )
        .await?;
    if boundary_changed {
        time::recompute_days(&st.db, &updated).await?;
    }
    Ok(Json(updated.into()))
}

// ----- locations -------------------------------------------------------------

#[utoipa::path(get, path = "/api/users/{id}/locations", tag = "profile", params(("id" = i32, Path)),
    responses((status = 200, body = Vec<LocationDto>), (status = 403, body = ErrorBody)))]
pub async fn list_locations(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<Json<Vec<LocationDto>>> {
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    Ok(Json(
        st.db
            .list_locations(user.id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

#[utoipa::path(post, path = "/api/users/{id}/locations", tag = "profile", params(("id" = i32, Path)), request_body = LocationInput,
    responses((status = 201, body = LocationDto), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_location(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<LocationInput>,
) -> ApiResult<(StatusCode, Json<LocationDto>)> {
    p.require_write()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    let tz = parse_tz(&input.timezone).map_err(ApiError::bad_request)?;
    let from = input.valid_from.unwrap_or_else(Utc::now);
    let loc = st.db.insert_location(user.id, from, tz.name()).await?;
    time::recompute_days(&st.db, &user).await?;
    Ok((StatusCode::CREATED, Json(loc.into())))
}

#[utoipa::path(patch, path = "/api/users/{id}/locations/{loc_id}", tag = "profile", params(("id" = i32, Path), ("loc_id" = i32, Path)), request_body = LocationPatchInput,
    responses((status = 200, body = LocationDto), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn update_location(
    State(st): State<AppState>,
    p: Principal,
    Path((id, loc_id)): Path<(i32, i32)>,
    Json(input): Json<LocationPatchInput>,
) -> ApiResult<Json<LocationDto>> {
    p.require_edit()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    let current = st.db.get_location(loc_id).await?;
    if current.user_id != user.id {
        return Err(ApiError::not_found());
    }
    let tz = input
        .timezone
        .as_deref()
        .map(|t| parse_tz(t).map_err(ApiError::bad_request))
        .transpose()?;
    let loc = st
        .db
        .update_location(loc_id, input.valid_from, tz.map(|t| t.name()))
        .await?;
    time::recompute_days(&st.db, &user).await?;
    Ok(Json(loc.into()))
}

#[utoipa::path(delete, path = "/api/users/{id}/locations/{loc_id}", tag = "profile", params(("id" = i32, Path), ("loc_id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn delete_location(
    State(st): State<AppState>,
    p: Principal,
    Path((id, loc_id)): Path<(i32, i32)>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    let current = st.db.get_location(loc_id).await?;
    if current.user_id != user.id {
        return Err(ApiError::not_found());
    }
    st.db.delete_location(loc_id).await?;
    time::recompute_days(&st.db, &user).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ----- targets ---------------------------------------------------------------

#[utoipa::path(get, path = "/api/users/{id}/targets", tag = "profile", params(("id" = i32, Path)),
    responses((status = 200, body = Vec<TargetDto>), (status = 403, body = ErrorBody)))]
pub async fn list_targets(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<Json<Vec<TargetDto>>> {
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    Ok(Json(
        st.db
            .list_targets(user.id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

fn check_target(kcal: i32, protein_g: Option<i32>) -> ApiResult<()> {
    if kcal <= 0 {
        return Err(ApiError::bad_request("kcal must be positive"));
    }
    if protein_g.is_some_and(|p| p <= 0) {
        return Err(ApiError::bad_request("protein_g must be positive"));
    }
    Ok(())
}

fn goal_grams(kg: Option<&str>) -> ApiResult<Option<i32>> {
    kg.map(|k| parse_kg(k).map_err(|e| ApiError::bad_request(e.to_string())))
        .transpose()
}

#[utoipa::path(post, path = "/api/users/{id}/targets", tag = "profile", params(("id" = i32, Path)), request_body = TargetInput,
    responses((status = 201, body = TargetDto), (status = 400, body = ErrorBody), (status = 409, body = ErrorBody)))]
pub async fn create_target(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
    Json(input): Json<TargetInput>,
) -> ApiResult<(StatusCode, Json<TargetDto>)> {
    p.require_edit()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    check_target(input.kcal, input.protein_g)?;
    let valid_from = match input.valid_from {
        Some(d) => d,
        None => {
            let tz = time::current_zone(&st.db, &user).await?;
            day::today(tz, user.day_boundary_minutes)
        }
    };
    let t = st
        .db
        .insert_target(NewTarget {
            user_id: user.id,
            valid_from,
            kcal: input.kcal,
            protein_g: input.protein_g,
            weight_g: goal_grams(input.weight_kg.as_deref())?,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(t.into())))
}

#[utoipa::path(patch, path = "/api/users/{id}/targets/{target_id}", tag = "profile", params(("id" = i32, Path), ("target_id" = i32, Path)), request_body = TargetPatchInput,
    responses((status = 200, body = TargetDto), (status = 404, body = ErrorBody)))]
pub async fn update_target(
    State(st): State<AppState>,
    p: Principal,
    Path((id, target_id)): Path<(i32, i32)>,
    Json(input): Json<TargetPatchInput>,
) -> ApiResult<Json<TargetDto>> {
    p.require_edit()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    let current = st.db.get_target(target_id).await?;
    if current.user_id != user.id {
        return Err(ApiError::not_found());
    }
    check_target(
        input.kcal.unwrap_or(current.kcal),
        input.protein_g.flatten(),
    )?;
    let weight_g = match input.weight_kg {
        None => None,
        Some(None) => Some(None),
        Some(Some(kg)) => Some(goal_grams(Some(&kg))?),
    };
    let t = st
        .db
        .update_target(
            target_id,
            TargetPatch {
                valid_from: input.valid_from,
                kcal: input.kcal,
                protein_g: input.protein_g,
                weight_g,
            },
        )
        .await?;
    Ok(Json(t.into()))
}

#[utoipa::path(delete, path = "/api/users/{id}/targets/{target_id}", tag = "profile", params(("id" = i32, Path), ("target_id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn delete_target(
    State(st): State<AppState>,
    p: Principal,
    Path((id, target_id)): Path<(i32, i32)>,
) -> ApiResult<StatusCode> {
    p.require_edit()?;
    let user = scope::member(&st.db, p.user(), Some(id)).await?;
    let current = st.db.get_target(target_id).await?;
    if current.user_id != user.id {
        return Err(ApiError::not_found());
    }
    st.db.delete_target(target_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
