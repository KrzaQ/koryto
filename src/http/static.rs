//! The Vue bundle, embedded at build time from frontend/dist. Unknown paths
//! fall back to index.html so client-side routes deep-link.

use axum::body::Body;
use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path == "api" {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }
    if let Some(file) = Assets::get(path) {
        return file_response(path, file.data.into_owned(), path.starts_with("assets/"));
    }
    match Assets::get("index.html") {
        Some(index) => file_response("index.html", index.data.into_owned(), false),
        None => (StatusCode::NOT_FOUND, "frontend bundle not built").into_response(),
    }
}

fn file_response(path: &str, data: Vec<u8>, immutable: bool) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let cache = if immutable {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Response::builder()
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(header::CACHE_CONTROL, cache)
        .body(Body::from(data))
        .expect("static response")
}
