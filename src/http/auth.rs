//! Who is calling: a browser session (private cookie), a personal bearer
//! token, or a delegate token naming the acting person. Every principal
//! resolves to a user, so handlers never care which.

use axum::extract::{FromRequestParts, Query, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::{Cookie, PrivateCookieJar, SameSite};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::AppState;
use super::error::{ApiError, ApiResult};
use super::handlers::dto::{HouseholdDto, TargetDto, UserDto};
use super::oidc::LoginState;
use crate::app::time;
use crate::config::AuthMode;
use crate::db::{ApiToken, Db, DbResult, User, VIA_MCP, VIA_WEB};
use crate::domain::day;
use crate::domain::token;

pub const SESSION_COOKIE: &str = "koryto_session";
const LOGIN_COOKIE: &str = "koryto_login";
const SESSION_DAYS: i64 = 30;
/// A delegate token acts only for people who have logged in through the
/// browser this recently, so a revocation in authentik reaches the gateway
/// path on its own, with the same lag a session cookie has.
pub const DELEGATE_LOGIN_DAYS: i64 = SESSION_DAYS;
pub const DEV_SUBJECT: &str = "dev";
pub const DEV_HOUSEHOLD: &str = "dev";

/// The request header a delegate token uses to name the acting user.
pub const DELEGATE_HEADER: &str = "x-koryto-user";

#[derive(Debug, Clone)]
pub enum Principal {
    Session(User),
    /// A personal token acting as its user.
    Token {
        token: ApiToken,
        user: User,
    },
    /// A delegate token acting for the user it named in `X-Koryto-User`.
    Delegate {
        token: ApiToken,
        user: User,
    },
}

impl Principal {
    pub fn user(&self) -> &User {
        match self {
            Self::Session(u) | Self::Token { user: u, .. } | Self::Delegate { user: u, .. } => u,
        }
    }

    pub fn kind(&self) -> &'static str {
        match self {
            Self::Session(_) => "session",
            Self::Token { .. } => "token",
            Self::Delegate { .. } => "delegate",
        }
    }

    /// How entries made by this principal are marked.
    pub fn via(&self) -> &'static str {
        match self {
            Self::Session(_) => VIA_WEB,
            Self::Token { .. } | Self::Delegate { .. } => VIA_MCP,
        }
    }

    fn has_scope(&self, scope: &str) -> bool {
        match self {
            Self::Session(_) => true,
            Self::Token { token, .. } | Self::Delegate { token, .. } => {
                token.scopes.iter().any(|s| s == scope)
            }
        }
    }

    /// Log entries, add foods, set a location.
    pub fn can_write(&self) -> bool {
        self.has_scope(token::SCOPE_WRITE)
    }

    /// Change and void entries, targets, foods, the profile.
    pub fn can_edit(&self) -> bool {
        self.has_scope(token::SCOPE_EDIT)
    }

    pub fn require_write(&self) -> ApiResult<()> {
        if self.can_write() {
            Ok(())
        } else {
            Err(ApiError::forbidden("write scope required"))
        }
    }

    pub fn require_edit(&self) -> ApiResult<()> {
        if self.can_edit() {
            Ok(())
        } else {
            Err(ApiError::forbidden("edit scope required"))
        }
    }

    pub fn scopes(&self) -> Vec<String> {
        match self {
            Self::Session(_) => vec![
                token::SCOPE_READ.into(),
                token::SCOPE_WRITE.into(),
                token::SCOPE_EDIT.into(),
            ],
            Self::Token { token, .. } | Self::Delegate { token, .. } => token.scopes.clone(),
        }
    }

    /// Token management and logout are for people, never for tokens.
    pub fn require_session(&self) -> ApiResult<&User> {
        match self {
            Self::Session(u) => Ok(u),
            Self::Token { .. } | Self::Delegate { .. } => {
                Err(ApiError::forbidden("session required"))
            }
        }
    }
}

fn session_cookie(state: &AppState, user_id: i32) -> Cookie<'static> {
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    let mut c = Cookie::new(SESSION_COOKIE, format!("{user_id}:{}", expires.timestamp()));
    c.set_path("/");
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_secure(state.config.public_url.scheme() == "https");
    c.set_max_age(::time::Duration::days(SESSION_DAYS));
    c
}

fn login_cookie(state: &AppState, value: String) -> Cookie<'static> {
    let mut c = Cookie::new(LOGIN_COOKIE, value);
    c.set_path("/api/auth");
    c.set_http_only(true);
    c.set_same_site(SameSite::Lax);
    c.set_secure(state.config.public_url.scheme() == "https");
    c.set_max_age(::time::Duration::minutes(10));
    c
}

fn removal(name: &'static str, path: &'static str) -> Cookie<'static> {
    let mut c = Cookie::from(name);
    c.set_path(path);
    c
}

async fn session_user(state: &AppState, jar: &PrivateCookieJar) -> Option<User> {
    let cookie = jar.get(SESSION_COOKIE)?;
    let (id, exp) = cookie.value().split_once(':')?;
    let id: i32 = id.parse().ok()?;
    let exp: i64 = exp.parse().ok()?;
    if exp < Utc::now().timestamp() {
        return None;
    }
    state.db.get_user(id).await.ok()
}

/// The dev user, placed in the dev household so the app is usable at once.
pub async fn dev_user(db: &Db, house_tz: &str) -> DbResult<User> {
    let user = db
        .upsert_user(DEV_SUBJECT, Some("dev@localhost"), Some("Dev"), house_tz)
        .await?;
    if user.household_id.is_some() {
        return Ok(user);
    }
    let household = match db.find_household(DEV_HOUSEHOLD).await {
        Ok(h) => h,
        Err(crate::db::DbError::NotFound) => db.create_household(DEV_HOUSEHOLD).await?,
        Err(e) => return Err(e),
    };
    db.set_user_household(user.id, Some(household.id)).await
}

/// The principal behind an `Authorization: Bearer` header, if there is one.
/// Shared by the API extractor and the MCP middleware so a delegate token is
/// resolved the same way everywhere: it must name a known user in
/// `X-Koryto-User`, and acts as that user.
pub async fn bearer(state: &AppState, headers: &HeaderMap) -> ApiResult<Option<Principal>> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Ok(None);
    };
    let value = value.to_str().map_err(|_| ApiError::unauthorized())?;
    let Some(secret) = value.strip_prefix("Bearer ").map(str::trim) else {
        return Err(ApiError::unauthorized());
    };
    let Some(t) = state.db.find_active_token(&token::hash(secret)).await? else {
        return Err(ApiError::unauthorized());
    };
    if !token::is_delegate(&t.scopes) {
        let user_id = t.user_id.ok_or_else(ApiError::unauthorized)?;
        let user = state.db.get_user(user_id).await?;
        return Ok(Some(Principal::Token { token: t, user }));
    }
    let email = headers
        .get(DELEGATE_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ApiError::forbidden("delegate token needs X-Koryto-User"))?;
    let user = state
        .db
        .find_user_by_email(email)
        .await?
        .ok_or_else(|| ApiError::forbidden(format!("{email} has never logged in here")))?;
    let fresh = user
        .last_login_at
        .is_some_and(|at| Utc::now() - at <= chrono::Duration::days(DELEGATE_LOGIN_DAYS));
    if !fresh {
        return Err(ApiError::forbidden(format!(
            "{email} has not logged in through the browser for {DELEGATE_LOGIN_DAYS} days; \
             log in there once to keep using the gateway"
        )));
    }
    Ok(Some(Principal::Delegate { token: t, user }))
}

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(p) = bearer(state, &parts.headers).await? {
            return Ok(p);
        }
        let jar = PrivateCookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::unauthorized())?;
        if let Some(u) = session_user(state, &jar).await {
            return Ok(Principal::Session(u));
        }
        if let AuthMode::Dev = state.config.auth {
            let u = dev_user(&state.db, state.config.timezone.name()).await?;
            return Ok(Principal::Session(u));
        }
        Err(ApiError::unauthorized())
    }
}

// ----- handlers -------------------------------------------------------------

#[derive(Serialize, ToSchema)]
pub struct Me {
    /// "session", "token" or "delegate"
    pub kind: String,
    pub user: UserDto,
    /// None until the CLI has placed the user in a household
    pub household: Option<HouseholdDto>,
    /// The zone the user is in right now
    pub timezone: String,
    /// Today on the user's clock and boundary
    pub today: chrono::NaiveDate,
    /// The target in force today
    pub target: Option<TargetDto>,
    pub scopes: Vec<String>,
    pub can_write: bool,
    pub can_edit: bool,
}

#[utoipa::path(get, path = "/api/me", tag = "auth", responses((status = 200, body = Me), (status = 401, body = super::error::ErrorBody)))]
pub async fn me(State(state): State<AppState>, p: Principal) -> ApiResult<axum::Json<Me>> {
    let user = p.user();
    let tz = time::current_zone(&state.db, user).await?;
    let today = day::today(tz, user.day_boundary_minutes);
    let household = match user.household_id {
        Some(id) => {
            let h = state.db.get_household(id).await?;
            let members = state.db.household_members(id).await?;
            Some(HouseholdDto::from(h, members))
        }
        None => None,
    };
    let target = state
        .db
        .target_for(user.id, today)
        .await?
        .map(TargetDto::from);
    Ok(axum::Json(Me {
        kind: p.kind().into(),
        user: UserDto::from(user.clone()),
        household,
        timezone: tz.name().to_string(),
        today,
        target,
        scopes: p.scopes(),
        can_write: p.can_write(),
        can_edit: p.can_edit(),
    }))
}

#[derive(Deserialize)]
pub struct LoginQuery {
    pub next: Option<String>,
}

fn safe_next(next: Option<String>) -> String {
    match next {
        Some(n) if n.starts_with('/') && !n.starts_with("//") => n,
        _ => "/".to_string(),
    }
}

/// Start the login. Dev mode signs the dev user in directly.
#[utoipa::path(get, path = "/api/auth/login", tag = "auth", responses((status = 302, description = "redirect to the identity provider")))]
pub async fn login(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<LoginQuery>,
) -> Response {
    let next = safe_next(q.next);
    match &state.oidc {
        None => {
            let user = match dev_user(&state.db, state.config.timezone.name()).await {
                Ok(u) => u,
                Err(e) => return ApiError::from(e).into_response(),
            };
            let jar = jar.add(session_cookie(&state, user.id));
            (jar, Redirect::to(&next)).into_response()
        }
        Some(oidc) => {
            let (url, login) = oidc.authorize();
            let value = serde_json::to_string(&(login, next)).expect("serialize login state");
            let jar = jar.add(login_cookie(&state, value));
            (jar, Redirect::to(url.as_str())).into_response()
        }
    }
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

fn page(status: StatusCode, title: &str, body: &str) -> Response {
    let html = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
         <body style=\"font-family:sans-serif;max-width:40em;margin:4em auto\"><h1>{title}</h1><p>{body}</p>\
         <p><a href=\"/\">Back</a></p></body>"
    );
    (status, Html(html)).into_response()
}

#[utoipa::path(get, path = "/api/auth/callback", tag = "auth", responses((status = 302), (status = 403, description = "not a member of the required group, when one is configured")))]
pub async fn callback(
    State(state): State<AppState>,
    jar: PrivateCookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(oidc) = &state.oidc else {
        return page(StatusCode::NOT_FOUND, "Not found", "OIDC is not enabled.");
    };
    if let Some(err) = q.error {
        let desc = q.error_description.unwrap_or_default();
        return page(
            StatusCode::BAD_REQUEST,
            "Login failed",
            &format!("{err}: {desc}"),
        );
    }
    let Some(cookie) = jar.get(LOGIN_COOKIE) else {
        return page(
            StatusCode::BAD_REQUEST,
            "Login expired",
            "Start again from the login page.",
        );
    };
    let Ok((login, next)) = serde_json::from_str::<(LoginState, String)>(cookie.value()) else {
        return page(
            StatusCode::BAD_REQUEST,
            "Login expired",
            "Start again from the login page.",
        );
    };
    let jar = jar.remove(removal(LOGIN_COOKIE, "/api/auth"));
    if q.state.as_deref() != Some(login.csrf.as_str()) {
        return page(StatusCode::BAD_REQUEST, "Login failed", "State mismatch.");
    }
    let Some(code) = q.code else {
        return page(
            StatusCode::BAD_REQUEST,
            "Login failed",
            "No authorization code.",
        );
    };
    let identity = match oidc.exchange(&code, &login).await {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("OIDC callback: {e:#}");
            return page(
                StatusCode::BAD_GATEWAY,
                "Login failed",
                "The identity provider rejected the login.",
            );
        }
    };
    if let Some(group) = &oidc.group
        && !identity.groups.iter().any(|g| g == group)
    {
        tracing::warn!(
            "login refused for {}: not in group {group}",
            identity.subject
        );
        return page(
            StatusCode::FORBIDDEN,
            "Not allowed",
            &format!("Your account is not in the <code>{group}</code> group."),
        );
    }
    let user = match state
        .db
        .upsert_user(
            &identity.subject,
            identity.email.as_deref(),
            identity.name.as_deref(),
            state.config.timezone.name(),
        )
        .await
    {
        Ok(u) => u,
        Err(e) => return ApiError::from(e).into_response(),
    };
    let jar = jar.add(session_cookie(&state, user.id));
    (jar, Redirect::to(&next)).into_response()
}

#[utoipa::path(post, path = "/api/auth/logout", tag = "auth", responses((status = 204)))]
pub async fn logout(jar: PrivateCookieJar) -> Response {
    (
        jar.remove(removal(SESSION_COOKIE, "/")),
        StatusCode::NO_CONTENT,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_must_be_a_local_path() {
        assert_eq!(safe_next(Some("/d/2026-09-04".into())), "/d/2026-09-04");
        assert_eq!(safe_next(Some("//evil.example".into())), "/");
        assert_eq!(safe_next(Some("https://evil.example".into())), "/");
        assert_eq!(safe_next(None), "/");
    }
}
