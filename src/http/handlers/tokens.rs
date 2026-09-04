//! Token management is session-only: a token must never mint or revoke tokens.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use super::dto::{TokenCreated, TokenDto, TokenInput};
use crate::app::scope;
use crate::domain::token;
use crate::http::AppState;
use crate::http::auth::Principal;
use crate::http::error::{ApiError, ApiResult, ErrorBody};

#[utoipa::path(get, path = "/api/tokens", tag = "tokens", responses((status = 200, body = Vec<TokenDto>), (status = 403, body = ErrorBody)))]
pub async fn list_tokens(
    State(st): State<AppState>,
    p: Principal,
) -> ApiResult<Json<Vec<TokenDto>>> {
    p.require_session()?;
    Ok(Json(
        st.db
            .list_tokens()
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    ))
}

#[utoipa::path(post, path = "/api/tokens", tag = "tokens", request_body = TokenInput,
    responses((status = 201, body = TokenCreated), (status = 400, body = ErrorBody), (status = 403, body = ErrorBody)))]
pub async fn create_token(
    State(st): State<AppState>,
    p: Principal,
    Json(input): Json<TokenInput>,
) -> ApiResult<(StatusCode, Json<TokenCreated>)> {
    let me = p.require_session()?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("name cannot be empty"));
    }
    let scopes = token::parse_scopes(&input.scopes).map_err(ApiError::bad_request)?;
    let user_id = if token::is_delegate(&scopes) {
        if input.user_id.is_some() {
            return Err(ApiError::bad_request(
                "a delegate token acts as whoever X-Koryto-User names; drop user_id",
            ));
        }
        None
    } else {
        Some(scope::member(&st.db, me, input.user_id).await?.id)
    };
    let new = token::generate();
    let t = st
        .db
        .create_token(name, &new.hash, &scopes, user_id, Some(me.id))
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(TokenCreated {
            id: t.id,
            name: t.name,
            scopes: t.scopes,
            user_id: t.user_id,
            secret: new.secret,
        }),
    ))
}

#[utoipa::path(delete, path = "/api/tokens/{id}", tag = "tokens", params(("id" = i32, Path)),
    responses((status = 204), (status = 404, body = ErrorBody)))]
pub async fn revoke_token(
    State(st): State<AppState>,
    p: Principal,
    Path(id): Path<i32>,
) -> ApiResult<StatusCode> {
    p.require_session()?;
    st.db.revoke_token(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
