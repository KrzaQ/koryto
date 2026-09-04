//! Router tests against a real database. Requests go through the whole
//! stack: extractors, auth, handlers, JSON envelope.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;
use crate::config::{AuthMode, OidcConfig};
use crate::db::User;
use crate::db::test_db::TestDb;
use crate::db_or_skip;
use crate::domain::token;

fn config(auth: AuthMode, public_url: &str) -> Config {
    Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        public_url: public_url.parse().unwrap(),
        secret: b"0123456789abcdef0123456789abcdef0123456789abcdef".to_vec(),
        auth,
        auto_migrate: false,
        timezone: chrono_tz::Europe::Warsaw,
    }
}

async fn dev_app(t: &TestDb) -> Router {
    let cfg = config(AuthMode::Dev, "http://localhost:8000");
    let state = AppState::new(cfg, t.db.clone()).await.unwrap();
    router(state)
}

fn oidc_mode() -> AuthMode {
    AuthMode::Oidc(OidcConfig {
        issuer: "http://127.0.0.1:1/unused".into(),
        client_id: "x".into(),
        client_secret: "y".into(),
        group: "koryto".into(),
    })
}

/// A state whose OIDC discovery would fail is built with `oidc: None` but
/// non-dev auth, which is exactly "cookie or token, nothing else".
async fn strict_app(t: &TestDb) -> Router {
    let cfg = config(oidc_mode(), "https://koryto.example");
    let state = AppState {
        db: t.db.clone(),
        config: std::sync::Arc::new(cfg.clone()),
        oidc: None,
        key: cookie_key(&cfg.secret),
    };
    router(state)
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Value, axum::http::HeaderMap) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into()))
    };
    (status, body, headers)
}

fn req(method: &str, uri: &str, auth: Option<&str>, body: Option<Value>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(a) = auth {
        b = b.header(header::AUTHORIZATION, format!("Bearer {a}"));
    }
    match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    }
}

fn delegated(
    method: &str,
    uri: &str,
    secret: &str,
    email: &str,
    body: Option<Value>,
) -> Request<Body> {
    let mut r = req(method, uri, Some(secret), body);
    r.headers_mut()
        .insert("x-koryto-user", email.parse().unwrap());
    r
}

/// A logged-in user placed in a household (or not).
async fn user(t: &TestDb, subject: &str, email: &str, household: Option<i32>) -> User {
    let u =
        t.db.upsert_user(subject, Some(email), Some(subject), "Europe/Warsaw")
            .await
            .unwrap();
    match household {
        Some(h) => t.db.set_user_household(u.id, Some(h)).await.unwrap(),
        None => u,
    }
}

async fn token_for(t: &TestDb, name: &str, scopes: &[&str], user_id: Option<i32>) -> String {
    let n = token::generate();
    t.db.create_token(
        name,
        &n.hash,
        &scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        user_id,
        None,
    )
    .await
    .unwrap();
    n.secret
}

/// A household with two members and personal tokens for the first.
struct Home {
    household: i32,
    alice: User,
    bob: User,
    reader: String,
    writer: String,
    editor: String,
}

async fn home(t: &TestDb) -> Home {
    let h = t.db.create_household("home").await.unwrap();
    let alice = user(t, "alice", "alice@example.com", Some(h.id)).await;
    let bob = user(t, "bob", "bob@example.com", Some(h.id)).await;
    Home {
        household: h.id,
        reader: token_for(t, "reader", &["read"], Some(alice.id)).await,
        writer: token_for(t, "writer", &["read", "write"], Some(alice.id)).await,
        editor: token_for(t, "editor", &["read", "write", "edit"], Some(alice.id)).await,
        alice,
        bob,
    }
}

#[tokio::test]
async fn health_and_openapi_need_no_auth() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let (s, b, _) = call(&app, req("GET", "/api/health", None, None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["status"], "ok");
    let (s, b, _) = call(&app, req("GET", "/api/openapi.json", None, None)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["paths"]["/api/meals"].is_object());
    assert!(b["paths"]["/api/users/{id}/locations/{loc_id}"].is_object());
    assert!(b["paths"]["/api/day"].is_object());
    t.finish().await;
}

#[tokio::test]
async fn api_never_redirects_on_missing_auth() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let (s, b, h) = call(&app, req("GET", "/api/day", None, None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(b["error"]["code"], "unauthorized");
    assert!(h.get(header::LOCATION).is_none());
    let (s, _, _) = call(&app, req("GET", "/api/day", Some("ko_bogus"), None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _, _) = call(&app, req("GET", "/api/me", None, None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    t.finish().await;
}

#[tokio::test]
async fn scopes_are_enforced() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let h = home(&t).await;

    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&h.reader), None)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["kind"], "token");
    assert_eq!(b["user"]["email"], "alice@example.com");
    assert_eq!(b["household"]["members"].as_array().unwrap().len(), 2);
    assert_eq!(b["can_write"], false);
    assert_eq!(b["timezone"], "Europe/Warsaw");

    let meal = json!({"description": "eggs", "kcal": 300});
    let (s, b, _) = call(
        &app,
        req("POST", "/api/meals", Some(&h.reader), Some(meal.clone())),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(b["error"]["code"], "forbidden");

    let (s, b, _) = call(&app, req("POST", "/api/meals", Some(&h.writer), Some(meal))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = b[0]["id"].as_i64().unwrap();
    assert_eq!(b[0]["created_via"], "mcp");
    assert_eq!(b[0]["source"], "estimate");

    // write is additive: changing and voiding need edit.
    let patch = json!({"kcal": 350});
    let (s, _, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{id}"),
            Some(&h.writer),
            Some(patch.clone()),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/meals/{id}/void"),
            Some(&h.writer),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{id}"),
            Some(&h.editor),
            Some(patch),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["kcal"], 350);
    assert_eq!(b["source"], "manual");

    // Tokens never manage tokens, whatever their scope.
    let (s, _, _) = call(&app, req("GET", "/api/tokens", Some(&h.editor), None)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    t.finish().await;
}

#[tokio::test]
async fn delegate_tokens_act_as_the_named_user() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let h = home(&t).await;
    let d = token_for(&t, "openwebui", &["read", "write", "delegate"], None).await;

    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&d), None)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "no header: {b}");
    assert!(
        b["error"]["message"]
            .as_str()
            .unwrap()
            .contains("X-Koryto-User")
    );

    let (s, b, _) = call(
        &app,
        delegated("GET", "/api/me", &d, "nobody@example.com", None),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "unknown user: {b}");

    let (s, b, _) = call(
        &app,
        delegated("GET", "/api/me", &d, "Bob@Example.com", None),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["kind"], "delegate");
    assert_eq!(b["user"]["id"], h.bob.id);
    assert_eq!(b["can_write"], true);
    assert_eq!(b["can_edit"], false);

    // Bob logs for himself; the row is his and marks him as creator.
    let meal = json!({"description": "toast", "kcal": 200});
    let (s, b, _) = call(
        &app,
        delegated("POST", "/api/meals", &d, "bob@example.com", Some(meal)),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b[0]["user_id"], h.bob.id);
    assert_eq!(b[0]["created_by"], h.bob.id);
    let _ = h.alice;

    let (s, _, _) = call(
        &app,
        delegated("GET", "/api/tokens", &d, "bob@example.com", None),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    t.finish().await;
}

#[tokio::test]
async fn dev_mode_is_a_session_in_a_household_and_manages_tokens() {
    let t = db_or_skip!();
    let app = dev_app(&t).await;
    let (s, b, _) = call(&app, req("GET", "/api/me", None, None)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["kind"], "session");
    assert_eq!(b["household"]["name"], "dev");
    let dev_id = b["user"]["id"].as_i64().unwrap() as i32;

    let (s, b, _) = call(
        &app,
        req(
            "POST",
            "/api/tokens",
            None,
            Some(json!({"name": "cli", "scopes": "write"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let secret = b["secret"].as_str().unwrap().to_string();
    assert!(secret.starts_with("ko_"));
    assert_eq!(b["scopes"], json!(["read", "write"]));
    assert_eq!(b["user_id"], dev_id);
    let id = b["id"].as_i64().unwrap();

    // The token acts as the dev user.
    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&secret), None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["kind"], "token");
    assert_eq!(b["user"]["id"], dev_id);

    let (s, b, _) = call(&app, req("GET", "/api/tokens", None, None)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(
        b.as_array()
            .unwrap()
            .iter()
            .all(|t| t.get("token_hash").is_none())
    );

    let (s, _, _) = call(
        &app,
        req("DELETE", &format!("/api/tokens/{id}"), None, None),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _, _) = call(&app, req("GET", "/api/me", Some(&secret), None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "revoked");

    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/tokens",
            None,
            Some(json!({"name": "x", "scopes": "admin"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A delegate token names nobody.
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/tokens",
            None,
            Some(json!({"name": "x", "scopes": "read,delegate", "user_id": dev_id})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A personal token for someone outside the household is refused.
    let other = t.db.create_household("other").await.unwrap();
    let carol = user(&t, "carol", "carol@example.com", Some(other.id)).await;
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/tokens",
            None,
            Some(json!({"name": "x", "scopes": "read", "user_id": carol.id})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    t.finish().await;
}

#[tokio::test]
async fn no_household_means_no_data() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let lonely = user(&t, "lonely", "lonely@example.com", None).await;
    let tok = token_for(&t, "lonely", &["read", "write", "edit"], Some(lonely.id)).await;
    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&tok), None)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["household"].is_null());
    let (s, b, _) = call(&app, req("GET", "/api/day", Some(&tok), None)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    assert!(
        b["error"]["message"]
            .as_str()
            .unwrap()
            .contains("household")
    );
    let (s, _, _) = call(&app, req("GET", "/api/foods", Some(&tok), None)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/meals",
            Some(&tok),
            Some(json!({"description": "x", "kcal": 1})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    t.finish().await;
}

#[tokio::test]
async fn household_scoping_on_every_person_route() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let h = home(&t).await;
    let other = t.db.create_household("other").await.unwrap();
    let carol = user(&t, "carol", "carol@example.com", Some(other.id)).await;
    let carol_tok = token_for(&t, "carol", &["read", "write", "edit"], Some(carol.id)).await;

    // Bob is fine, Carol is not, on reads and writes.
    let (s, _, _) = call(
        &app,
        req(
            "GET",
            &format!("/api/day?user={}", h.bob.id),
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, b, _) = call(
        &app,
        req(
            "GET",
            &format!("/api/day?user={}", carol.id),
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{b}");
    let (s, _, _) = call(
        &app,
        req(
            "GET",
            &format!("/api/users/{}/targets", carol.id),
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/meals",
            Some(&h.writer),
            Some(json!({"user_ids": [h.bob.id, carol.id], "description": "x", "kcal": 1})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/weights",
            Some(&h.writer),
            Some(json!({"user_id": carol.id, "weight_kg": "70"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Carol cannot touch Alice's entry, and cannot see Alice's foods.
    let (s, b, _) = call(
        &app,
        req(
            "POST",
            "/api/meals",
            Some(&h.writer),
            Some(json!({"description": "soup", "kcal": 250})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let id = b[0]["id"].as_i64().unwrap();
    let (s, _, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{id}"),
            Some(&carol_tok),
            Some(json!({"kcal": 1})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/meals/{id}/void"),
            Some(&carol_tok),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, b, _) = call(
        &app,
        req(
            "POST",
            "/api/foods",
            Some(&h.writer),
            Some(json!({"name": "Dal", "portion": "1 bowl", "kcal": 500})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let food = b["id"].as_i64().unwrap();
    let (s, b, _) = call(&app, req("GET", "/api/foods", Some(&carol_tok), None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b.as_array().unwrap().len(), 0);
    let (s, _, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/foods/{food}"),
            Some(&carol_tok),
            Some(json!({"kcal": 1})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/meals",
            Some(&carol_tok),
            Some(json!({"food_id": food})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let _ = h.household;
    t.finish().await;
}

#[tokio::test]
async fn meals_for_two_people_from_a_food_and_the_day_view() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let h = home(&t).await;
    let e = &h.editor;

    let (s, b, _) = call(
        &app,
        req("POST", "/api/foods", Some(e), Some(json!({"name": "Lentil curry", "aliases": ["dal", " Dal "], "portion": "1 bowl (350 g)", "kcal": 520, "protein_g": 24}))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["aliases"], json!(["dal"]));
    let food = b["id"].as_i64().unwrap();

    // One dinner for both, a portion and a half each, on a fixed evening.
    let (s, b, _) = call(
        &app,
        req("POST", "/api/meals", Some(e), Some(json!({"user_ids": [h.alice.id, h.bob.id, h.alice.id], "food_id": food, "portions": "1.5", "eaten_at": "2026-09-04 19:30"}))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let rows = b.as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["user_id"], h.alice.id);
    assert_eq!(rows[1]["user_id"], h.bob.id);
    assert_eq!(rows[0]["kcal"], 780);
    assert_eq!(rows[0]["protein_g"], 36);
    assert_eq!(rows[0]["source"], "food");
    assert_eq!(rows[0]["description"], "Lentil curry");
    assert_eq!(rows[0]["portions"], "1.5");
    assert_eq!(rows[0]["day"], "2026-09-04");
    assert_eq!(rows[0]["timezone"], "Europe/Warsaw");
    assert_eq!(rows[0]["eaten_at"], "2026-09-04T17:30:00Z");
    let alice_meal = rows[0]["id"].as_i64().unwrap();

    // Validation: kcal needed without a food, portions need a food, bad kg.
    for (body, why) in [
        (json!({"description": "x"}), "no kcal"),
        (json!({"description": "x", "kcal": -1}), "negative"),
        (
            json!({"description": "x", "kcal": 1, "portions": "2"}),
            "portions without food",
        ),
        (json!({"kcal": 1}), "no description"),
        (
            json!({"description": "x", "kcal": 1, "source": "food"}),
            "source food",
        ),
        (
            json!({"description": "x", "kcal": 1, "timezone": "Mars/Base"}),
            "bad zone",
        ),
    ] {
        let (s, _, _) = call(&app, req("POST", "/api/meals", Some(e), Some(body))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{why}");
    }

    // A late snack after midnight lands on the 4th; a weight and a run too.
    let (s, b, _) = call(
        &app,
        req("POST", "/api/meals", Some(e), Some(json!({"description": "kebab", "kcal": 700, "eaten_at": "2026-09-05T01:00:00+02:00", "source": "label"}))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b[0]["day"], "2026-09-04");
    let (s, b, _) = call(
        &app,
        req(
            "POST",
            "/api/weights",
            Some(e),
            Some(json!({"weight_kg": "82,4", "measured_at": "2026-09-04 07:00"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["weight_kg"], "82.4");
    assert_eq!(b["weight_g"], 82400);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            "/api/weights",
            Some(e),
            Some(json!({"weight_kg": "heavy"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, b, _) = call(
        &app,
        req("POST", "/api/activities", Some(e), Some(json!({"kind": " Run ", "duration": "1h30", "kcal": 600, "started_at": "2026-09-04 17:00"}))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["kind"], "run");
    assert_eq!(b["minutes"], 90);
    assert_eq!(b["duration"], "1h30");

    let (s, b, _) = call(
        &app,
        req("GET", "/api/day?date=2026-09-04", Some(&h.reader), None),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["totals"]["kcal"], 1480);
    assert_eq!(b["totals"]["protein_g"], 36);
    assert_eq!(b["totals"]["meals"], 2);
    assert_eq!(b["totals"]["meals_without_protein"], 1);
    assert_eq!(b["totals"]["sport_minutes"], 90);
    assert_eq!(b["logged"], true);
    assert!(b["target"].is_null());
    assert!(b["balance"].is_null());
    assert_eq!(b["weights"][0]["weight_kg"], "82.4");
    // Bob's day only has the curry.
    let (s, b, _) = call(
        &app,
        req(
            "GET",
            &format!("/api/day?date=2026-09-04&user={}", h.bob.id),
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["totals"]["kcal"], 780);

    // A target gives a balance.
    let (s, b, _) = call(
        &app,
        req("POST", &format!("/api/users/{}/targets", h.alice.id), Some(e), Some(json!({"valid_from": "2026-09-01", "kcal": 1800, "protein_g": 120, "weight_kg": "75"}))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["weight_kg"], "75");
    let (s, b, _) = call(
        &app,
        req("GET", "/api/day?date=2026-09-04", Some(&h.reader), None),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["balance"], 1480 - 1800);

    // Change the curry to two portions, then unlink it with a manual number.
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{alice_meal}"),
            Some(e),
            Some(json!({"portions": "2"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["kcal"], 1040);
    assert_eq!(b["portions"], "2");
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{alice_meal}"),
            Some(e),
            Some(json!({"kcal": 900, "protein_g": null})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["kcal"], 900);
    assert!(b["food_id"].is_null());
    assert!(b["protein_g"].is_null());
    assert_eq!(b["source"], "manual");
    // Move it by hand to the 5th, then back to the computed day.
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{alice_meal}"),
            Some(e),
            Some(json!({"day": "2026-09-05"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["day"], "2026-09-05");
    assert_eq!(b["day_override"], true);
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/meals/{alice_meal}"),
            Some(e),
            Some(json!({"day": null})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["day"], "2026-09-04");
    assert_eq!(b["day_override"], false);

    // Void, see it gone, unvoid, double void is a conflict.
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/meals/{alice_meal}/void"),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, b, _) = call(
        &app,
        req("GET", "/api/day?date=2026-09-04", Some(&h.reader), None),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["totals"]["kcal"], 700);
    assert_eq!(b["meals"].as_array().unwrap().len(), 1);
    let (s, b, _) = call(
        &app,
        req(
            "GET",
            "/api/day?date=2026-09-04&include_voided=true",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["meals"].as_array().unwrap().len(), 2);
    assert_eq!(b["totals"]["kcal"], 700);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/meals/{alice_meal}/void"),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/meals/{alice_meal}/unvoid"),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);

    // The range view: gaps stay gaps, the trend follows the weight.
    let (s, b, _) = call(
        &app,
        req(
            "GET",
            "/api/days?from=2026-09-03&to=2026-09-05",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let days = b["days"].as_array().unwrap();
    assert_eq!(days.len(), 3);
    assert_eq!(days[0]["logged"], false);
    assert!(days[0]["kcal"].is_null());
    assert_eq!(days[1]["kcal"], 1600);
    assert_eq!(days[1]["weight_g"], 82400);
    assert_eq!(days[1]["trend_g"], 82400);
    assert_eq!(days[1]["target_kcal"], 1800);
    assert_eq!(days[1]["balance"], -200);
    assert_eq!(days[1]["sport_minutes"], 90);
    assert!(days[2]["weight_g"].is_null());
    let (s, _, _) = call(
        &app,
        req(
            "GET",
            "/api/days?from=2026-09-05&to=2026-09-03",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = call(
        &app,
        req(
            "GET",
            "/api/days?from=2020-01-01&to=2026-09-03",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Foods: usage count, archive hides it from logging.
    let (s, b, _) = call(&app, req("GET", "/api/foods?q=dal", Some(&h.reader), None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b[0]["uses"], 1);
    let (s, b, _) = call(
        &app,
        req("POST", &format!("/api/foods/{food}/archive"), Some(e), None),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(!b["archived_at"].is_null());
    let (s, b, _) = call(
        &app,
        req(
            "POST",
            "/api/meals",
            Some(e),
            Some(json!({"food_id": food})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    let _ = h.household;
    t.finish().await;
}

#[tokio::test]
async fn profile_locations_and_targets() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let h = home(&t).await;
    let e = &h.editor;

    let (s, b, _) = call(
        &app,
        req("PATCH", &format!("/api/users/{}/profile", h.alice.id), Some(e), Some(json!({"height_mm": 1700, "born_on": "1990-06-15", "sex": "F", "activity_factor": "1.55"}))),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["sex"], "female");
    assert_eq!(b["activity_factor"], "1.55");
    let (s, _, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/users/{}/profile", h.alice.id),
            Some(e),
            Some(json!({"activity_factor": "9"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/users/{}/profile", h.alice.id),
            Some(&h.writer),
            Some(json!({"name": "A"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "profile needs edit");

    // A late meal in Warsaw, then "I was in New York since the 1st": the
    // meal moves to the evening before on the traveller's clock.
    let (s, b, _) = call(
        &app,
        req("POST", "/api/meals", Some(e), Some(json!({"description": "late", "kcal": 100, "eaten_at": "2026-09-05T03:00:00+02:00"}))),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(b[0]["day"], "2026-09-04");
    let id = b[0]["id"].as_i64().unwrap();
    let (s, b, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/users/{}/locations", h.alice.id),
            Some(&h.writer),
            Some(json!({"valid_from": "2026-09-01T00:00:00Z", "timezone": "America/New_York"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!(b["origin"], false);
    let loc = b["id"].as_i64().unwrap();
    let (s, b, _) = call(
        &app,
        req(
            "GET",
            "/api/meals?from=2026-09-01&to=2026-09-10",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let moved = b
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == id)
        .unwrap();
    assert_eq!(moved["timezone"], "America/New_York");
    assert_eq!(moved["day"], "2026-09-04"); // 21:00 on the 4th in New York
    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&h.reader), None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["timezone"], "America/New_York");

    let (s, b, _) = call(
        &app,
        req(
            "GET",
            &format!("/api/users/{}/locations", h.alice.id),
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b[0]["origin"], true);
    let origin = b[0]["id"].as_i64().unwrap();
    let (s, _, _) = call(
        &app,
        req(
            "DELETE",
            &format!("/api/users/{}/locations/{origin}", h.alice.id),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    // Bob's location is not reachable through Alice's path.
    let (s, _, _) = call(
        &app,
        req(
            "DELETE",
            &format!("/api/users/{}/locations/{loc}", h.bob.id),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _, _) = call(
        &app,
        req(
            "DELETE",
            &format!("/api/users/{}/locations/{loc}", h.alice.id),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&h.reader), None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["timezone"], "Europe/Warsaw");

    // Targets: default from today, duplicate day is a conflict, patch clears.
    let (s, b, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/users/{}/targets", h.alice.id),
            Some(e),
            Some(json!({"kcal": 1900})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let tid = b["id"].as_i64().unwrap();
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/users/{}/targets", h.alice.id),
            Some(e),
            Some(json!({"kcal": 2000})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, b, _) = call(&app, req("GET", "/api/me", Some(&h.reader), None)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b["target"]["kcal"], 1900);
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/users/{}/targets/{tid}", h.alice.id),
            Some(e),
            Some(json!({"protein_g": 130, "weight_kg": "70.5"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["weight_kg"], "70.5");
    let (s, b, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/users/{}/targets/{tid}", h.alice.id),
            Some(e),
            Some(json!({"weight_kg": null})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b["weight_kg"].is_null());
    let (s, _, _) = call(
        &app,
        req(
            "DELETE",
            &format!("/api/users/{}/targets/{tid}", h.alice.id),
            Some(e),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let _ = h.household;
    t.finish().await;
}

#[tokio::test]
async fn stats_endpoints_follow_the_days() {
    let t = db_or_skip!();
    let app = strict_app(&t).await;
    let h = home(&t).await;
    let e = &h.editor;
    // Four weeks of 2000 kcal with weekly weigh-ins going down, a profile
    // and a target, so every basis shows up.
    let (s, _, _) = call(
        &app,
        req(
            "PATCH",
            &format!("/api/users/{}/profile", h.alice.id),
            Some(e),
            Some(json!({"height_mm": 1800, "born_on": "1990-06-15", "sex": "male"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = call(
        &app,
        req(
            "POST",
            &format!("/api/users/{}/targets", h.alice.id),
            Some(e),
            Some(json!({"valid_from": "2026-08-01", "kcal": 1900, "weight_kg": "76"})),
        ),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    for i in 0..28 {
        let day = chrono::NaiveDate::from_ymd_opt(2026, 8, 3).unwrap() + chrono::Duration::days(i);
        let (s, _, _) = call(
            &app,
            req(
                "POST",
                "/api/meals",
                Some(e),
                Some(
                    json!({"description": "day", "kcal": 2000, "eaten_at": format!("{day} 12:00")}),
                ),
            ),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED);
        if i % 7 == 0 {
            let kg = format!("{}", 80.0 - 0.1 * (i as f64));
            let (s, _, _) = call(
                &app,
                req(
                    "POST",
                    "/api/weights",
                    Some(e),
                    Some(json!({"weight_kg": kg, "measured_at": format!("{day} 07:00")})),
                ),
            )
            .await;
            assert_eq!(s, StatusCode::CREATED);
        }
    }
    let (s, b, _) = call(
        &app,
        req(
            "GET",
            "/api/stats/weight?from=2026-08-01&to=2026-08-31",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert_eq!(b["points"].as_array().unwrap().len(), 4);
    assert_eq!(b["points"][0]["weight_g"], 80000);
    assert_eq!(b["points"][0]["trend_g"], 80000);
    assert!(b["points"][3]["trend_g"].as_i64().unwrap() < 80000);
    assert_eq!(b["goal_g"], 76000);

    let (s, b, _) = call(
        &app,
        req(
            "GET",
            "/api/stats/expenditure?from=2026-08-03&to=2026-08-30",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let days = b["days"].as_array().unwrap();
    assert_eq!(days.len(), 28);
    assert_eq!(days[0]["basis"], "seed");
    assert_eq!(days[27]["basis"], "adaptive");
    assert_eq!(b["latest"]["basis"], "adaptive");
    // Lost 2.1 kg of trend... only the trend moves, slowly: expenditure is a bit above 2000.
    assert!(b["latest"]["kcal"].as_i64().unwrap() > 2000, "{b}");

    let (s, b, _) = call(
        &app,
        req(
            "GET",
            "/api/stats/weekly?from=2026-08-03&to=2026-08-30",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let weeks = b["weeks"].as_array().unwrap();
    assert_eq!(weeks.len(), 4);
    assert_eq!(weeks[0]["week"], "2026-W32");
    assert_eq!(weeks[0]["start"], "2026-08-03");
    assert_eq!(weeks[0]["days"], 7);
    assert_eq!(weeks[0]["logged_days"], 7);
    assert_eq!(weeks[0]["mean_kcal"], 2000);
    assert_eq!(weeks[0]["mean_balance_vs_target"], 100);
    assert!(weeks[3]["mean_expenditure"].as_i64().unwrap() > 2000);
    let (s, _, _) = call(
        &app,
        req(
            "GET",
            &format!(
                "/api/stats/weekly?from=2026-08-03&to=2026-08-30&user={}",
                h.bob.id
            ),
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, _, _) = call(
        &app,
        req(
            "GET",
            "/api/stats/weight?from=2026-08-31&to=2026-08-01",
            Some(&h.reader),
            None,
        ),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    t.finish().await;
}

mod oidc_flow {
    use chrono::{Duration, Utc};
    use openidconnect::core::{
        CoreGenderClaim, CoreJsonWebKeySet, CoreJweContentEncryptionAlgorithm,
        CoreJwsSigningAlgorithm, CoreProviderMetadata, CoreResponseType, CoreRsaPrivateSigningKey,
        CoreSubjectIdentifierType,
    };
    use openidconnect::{
        Audience, AuthUrl, EmptyAdditionalProviderMetadata, EndUserEmail, IdToken, IdTokenClaims,
        IssuerUrl, JsonWebKeyId, JsonWebKeySetUrl, Nonce, PrivateSigningKey, ResponseTypes,
        StandardClaims, SubjectIdentifier, TokenUrl,
    };
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::http::oidc::GroupsClaims;

    struct Issuer {
        server: MockServer,
        key: CoreRsaPrivateSigningKey,
    }

    async fn issuer() -> Issuer {
        let server = MockServer::start().await;
        let pem = include_str!("../../tests/fixtures/oidc-test-key.pem");
        let key = CoreRsaPrivateSigningKey::from_pem(pem, Some(JsonWebKeyId::new("test".into())))
            .unwrap();
        let base = server.uri();
        let metadata = CoreProviderMetadata::new(
            IssuerUrl::new(base.clone()).unwrap(),
            AuthUrl::new(format!("{base}/authorize")).unwrap(),
            JsonWebKeySetUrl::new(format!("{base}/jwks")).unwrap(),
            vec![ResponseTypes::new(vec![CoreResponseType::Code])],
            vec![CoreSubjectIdentifierType::Public],
            vec![CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256],
            EmptyAdditionalProviderMetadata {},
        )
        .set_token_endpoint(Some(TokenUrl::new(format!("{base}/token")).unwrap()));
        Mock::given(method("GET"))
            .and(path("/.well-known/openid-configuration"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&metadata))
            .mount(&server)
            .await;
        let jwks = CoreJsonWebKeySet::new(vec![key.as_verification_key()]);
        Mock::given(method("GET"))
            .and(path("/jwks"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks))
            .mount(&server)
            .await;
        Issuer { server, key }
    }

    impl Issuer {
        async fn mount_token(&self, nonce: &str, groups: &[&str]) {
            let now = Utc::now();
            let claims: IdTokenClaims<GroupsClaims, CoreGenderClaim> = IdTokenClaims::new(
                IssuerUrl::new(self.server.uri()).unwrap(),
                vec![Audience::new("client".into())],
                now + Duration::minutes(5),
                now,
                StandardClaims::<CoreGenderClaim>::new(SubjectIdentifier::new("subject-1".into()))
                    .set_email(Some(EndUserEmail::new("k@example.test".into()))),
                GroupsClaims {
                    groups: groups.iter().map(|g| g.to_string()).collect(),
                },
            )
            .set_nonce(Some(Nonce::new(nonce.to_string())));
            let token: IdToken<
                GroupsClaims,
                CoreGenderClaim,
                CoreJweContentEncryptionAlgorithm,
                CoreJwsSigningAlgorithm,
            > = IdToken::new(
                claims,
                &self.key,
                CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
                None,
                None,
            )
            .unwrap();
            let body = json!({"access_token": "at", "token_type": "Bearer", "id_token": token.to_string()});
            Mock::given(method("POST"))
                .and(path("/token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(&self.server)
                .await;
        }
    }

    async fn oidc_app(t: &TestDb, iss: &Issuer) -> Router {
        let cfg = config(
            AuthMode::Oidc(OidcConfig {
                issuer: iss.server.uri(),
                client_id: "client".into(),
                client_secret: "secret".into(),
                group: "koryto".into(),
            }),
            "http://localhost:8000",
        );
        router(AppState::new(cfg, t.db.clone()).await.unwrap())
    }

    fn cookie_header(headers: &axum::http::HeaderMap) -> String {
        headers
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
            .collect::<Vec<_>>()
            .join("; ")
    }

    async fn start_login(app: &Router) -> (String, String, String) {
        let (s, _, h) = call(
            app,
            req("GET", "/api/auth/login?next=/d/2026-09-04", None, None),
        )
        .await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        let location: url::Url = h[header::LOCATION].to_str().unwrap().parse().unwrap();
        let q: std::collections::HashMap<_, _> = location.query_pairs().into_owned().collect();
        assert_eq!(q["redirect_uri"], "http://localhost:8000/api/auth/callback");
        assert_eq!(q["code_challenge_method"], "S256");
        assert!(q["scope"].contains("openid"));
        (q["state"].clone(), q["nonce"].clone(), cookie_header(&h))
    }

    #[tokio::test]
    async fn member_logs_in_and_gets_a_session_without_a_household() {
        let t = db_or_skip!();
        let iss = issuer().await;
        let app = oidc_app(&t, &iss).await;
        let (state, nonce, cookies) = start_login(&app).await;
        iss.mount_token(&nonce, &["staff", "koryto"]).await;

        let cb = Request::builder()
            .method("GET")
            .uri(format!("/api/auth/callback?code=abc&state={state}"))
            .header(header::COOKIE, &cookies)
            .body(Body::empty())
            .unwrap();
        let (s, _, h) = call(&app, cb).await;
        assert_eq!(s, StatusCode::SEE_OTHER);
        assert_eq!(h[header::LOCATION], "/d/2026-09-04");
        let session = cookie_header(&h);
        assert!(session.contains("koryto_session="));

        let me = Request::builder()
            .method("GET")
            .uri("/api/me")
            .header(header::COOKIE, &session)
            .body(Body::empty())
            .unwrap();
        let (s, b, _) = call(&app, me).await;
        assert_eq!(s, StatusCode::OK, "{b}");
        assert_eq!(b["kind"], "session");
        assert_eq!(b["user"]["email"], "k@example.test");
        assert!(b["household"].is_null());
        assert_eq!(b["timezone"], "Europe/Warsaw");

        let out = Request::builder()
            .method("POST")
            .uri("/api/auth/logout")
            .header(header::COOKIE, &session)
            .body(Body::empty())
            .unwrap();
        let (s, _, h) = call(&app, out).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert!(cookie_header(&h).contains("koryto_session="));
        t.finish().await;
    }

    #[tokio::test]
    async fn non_member_is_refused_and_no_user_is_created() {
        let t = db_or_skip!();
        let iss = issuer().await;
        let app = oidc_app(&t, &iss).await;
        let (state, nonce, cookies) = start_login(&app).await;
        iss.mount_token(&nonce, &["staff"]).await;
        let cb = Request::builder()
            .method("GET")
            .uri(format!("/api/auth/callback?code=abc&state={state}"))
            .header(header::COOKIE, &cookies)
            .body(Body::empty())
            .unwrap();
        let (s, _, h) = call(&app, cb).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert!(!cookie_header(&h).contains("koryto_session="));
        assert!(t.db.list_users().await.unwrap().is_empty());
        t.finish().await;
    }

    #[tokio::test]
    async fn state_mismatch_and_missing_cookie_fail() {
        let t = db_or_skip!();
        let iss = issuer().await;
        let app = oidc_app(&t, &iss).await;
        let (_state, nonce, cookies) = start_login(&app).await;
        iss.mount_token(&nonce, &["koryto"]).await;
        let cb = Request::builder()
            .method("GET")
            .uri("/api/auth/callback?code=abc&state=wrong")
            .header(header::COOKIE, &cookies)
            .body(Body::empty())
            .unwrap();
        let (s, _, _) = call(&app, cb).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        let (s, _, _) = call(
            &app,
            req("GET", "/api/auth/callback?code=abc&state=x", None, None),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
        t.finish().await;
    }
}
