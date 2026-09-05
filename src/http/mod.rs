//! The HTTP server: JSON API under /api, the embedded frontend everywhere
//! else, and the MCP endpoint.

pub mod auth;
pub mod error;
pub mod handlers;
pub mod oidc;
mod r#static;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use anyhow::{Context, Result};
use axum::Router;
use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::config::{AuthMode, Config};
use crate::db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub oidc: Option<Arc<oidc::OidcClient>>,
    pub key: Key,
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Key {
        state.key.clone()
    }
}

impl AppState {
    pub async fn new(config: Config, db: Db) -> Result<Self> {
        let oidc = match &config.auth {
            AuthMode::Oidc(o) => Some(Arc::new(oidc::discover(o, &config.public_url).await?)),
            AuthMode::Dev => None,
        };
        let key = cookie_key(&config.secret);
        Ok(Self {
            db,
            config: Arc::new(config),
            oidc,
            key,
        })
    }
}

/// cookie::Key wants 64 bytes; stretch the configured secret deterministically.
pub fn cookie_key(secret: &[u8]) -> Key {
    use sha2::{Digest, Sha256};
    let mut material = Vec::with_capacity(64);
    material.extend_from_slice(&Sha256::digest([secret, b":signing"].concat()));
    material.extend_from_slice(&Sha256::digest([secret, b":encryption"].concat()));
    Key::from(&material)
}

#[derive(OpenApi)]
#[openapi(
    info(title = "koryto", description = "Calorie and weight log for a household"),
    tags(
        (name = "auth"), (name = "profile"), (name = "foods"), (name = "activity-kinds"),
        (name = "days"), (name = "meals"), (name = "weights"), (name = "activities"),
        (name = "stats"), (name = "tokens")
    )
)]
struct ApiDoc;

pub fn router(state: AppState) -> Router {
    let (api, openapi) = OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(handlers::routes())
        .split_for_parts();
    let openapi_json = axum::Json(openapi);
    Router::new()
        .merge(api)
        .merge(crate::mcp::router(state.clone()))
        .route(
            "/api/openapi.json",
            axum::routing::get(move || async move { openapi_json.clone() }),
        )
        .fallback(r#static::serve)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(config: Config) -> Result<()> {
    let db = Db::connect(&config.database_url).await?;
    if config.auto_migrate {
        db.migrate().await?;
    }
    let bind = config.bind;
    let state = AppState::new(config, db).await?;
    if let AuthMode::Dev = state.config.auth {
        tracing::warn!("KORYTO_AUTH=dev: every request is the dev user");
    }
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("sigterm");
        tokio::select! {
            _ = ctrl_c => {},
            _ = term.recv() => {},
        }
    }
    #[cfg(not(unix))]
    ctrl_c.await.ok();
}
