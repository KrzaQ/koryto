//! Route table. Read handlers and write handlers live in separate routers so
//! a read-only role later is one middleware, not an audit.

pub mod dto;
pub mod entries;
pub mod foods;
pub mod profile;
pub mod stats;
pub mod tokens;

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::AppState;
use super::auth;

#[derive(Serialize, ToSchema)]
pub struct Health {
    pub status: String,
}

#[utoipa::path(get, path = "/api/health", tag = "auth", responses((status = 200, body = Health)))]
pub async fn health() -> Json<Health> {
    Json(Health {
        status: "ok".into(),
    })
}

pub fn read_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(profile::list_locations))
        .routes(routes!(profile::list_targets))
        .routes(routes!(foods::list_foods))
        .routes(routes!(entries::day))
        .routes(routes!(entries::days))
        .routes(routes!(entries::list_meals))
        .routes(routes!(entries::list_weights))
        .routes(routes!(entries::list_activities))
        .routes(routes!(stats::weight))
        .routes(routes!(stats::expenditure))
        .routes(routes!(stats::weekly))
}

pub fn write_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(profile::update_profile))
        .routes(routes!(profile::create_location))
        .routes(routes!(profile::update_location, profile::delete_location))
        .routes(routes!(profile::create_target))
        .routes(routes!(profile::update_target, profile::delete_target))
        .routes(routes!(foods::create_food))
        .routes(routes!(foods::update_food))
        .routes(routes!(foods::archive_food))
        .routes(routes!(foods::unarchive_food))
        .routes(routes!(entries::create_meals))
        .routes(routes!(entries::update_meal))
        .routes(routes!(entries::void_meal))
        .routes(routes!(entries::unvoid_meal))
        .routes(routes!(entries::create_weight))
        .routes(routes!(entries::update_weight))
        .routes(routes!(entries::void_weight))
        .routes(routes!(entries::unvoid_weight))
        .routes(routes!(entries::create_activity))
        .routes(routes!(entries::update_activity))
        .routes(routes!(entries::void_activity))
        .routes(routes!(entries::unvoid_activity))
}

pub fn session_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(tokens::list_tokens, tokens::create_token))
        .routes(routes!(tokens::revoke_token))
}

pub fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(health))
        .routes(routes!(auth::me))
        .routes(routes!(auth::login))
        .routes(routes!(auth::callback))
        .routes(routes!(auth::logout))
        .merge(read_routes())
        .merge(write_routes())
        .merge(session_routes())
}
