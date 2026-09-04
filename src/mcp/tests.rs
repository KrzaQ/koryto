//! JSON-RPC over the real router: auth, tool listing, calls and scopes.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::config::{AuthMode, Config, OidcConfig};
use crate::db::User;
use crate::db::test_db::TestDb;
use crate::db_or_skip;
use crate::domain::token;
use crate::http::{AppState, cookie_key, router};

async fn app(t: &TestDb) -> Router {
    let cfg = Config {
        database_url: String::new(),
        bind: "127.0.0.1:0".parse().unwrap(),
        public_url: "https://koryto.example".parse().unwrap(),
        secret: b"0123456789abcdef0123456789abcdef0123456789abcdef".to_vec(),
        auth: AuthMode::Oidc(OidcConfig {
            issuer: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            group: Some("koryto".into()),
        }),
        auto_migrate: false,
        timezone: chrono_tz::Europe::Warsaw,
    };
    let state = AppState {
        db: t.db.clone(),
        config: std::sync::Arc::new(cfg.clone()),
        oidc: None,
        key: cookie_key(&cfg.secret),
    };
    router(state)
}

struct Client {
    app: Router,
    token: String,
    delegate_user: Option<String>,
    session: Option<String>,
    next_id: u64,
}

impl Client {
    fn new(app: Router, token: String) -> Self {
        Self {
            app,
            token,
            delegate_user: None,
            session: None,
            next_id: 0,
        }
    }

    fn request(&self, body: Value) -> Request<Body> {
        let mut b = Request::builder()
            .method("POST")
            .uri("/mcp")
            .header(header::HOST, "koryto.example")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ACCEPT, "application/json, text/event-stream")
            .header(header::AUTHORIZATION, format!("Bearer {}", self.token));
        if let Some(u) = &self.delegate_user {
            b = b.header("x-koryto-user", u);
        }
        if let Some(s) = &self.session {
            b = b.header("mcp-session-id", s);
        }
        b.body(Body::from(body.to_string())).unwrap()
    }

    async fn rpc(&mut self, method: &str, params: Value) -> (StatusCode, Value) {
        self.next_id += 1;
        let body =
            json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params});
        let resp = self.app.clone().oneshot(self.request(body)).await.unwrap();
        let status = resp.status();
        if let Some(s) = resp.headers().get("mcp-session-id") {
            self.session = Some(s.to_str().unwrap().to_string());
        }
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, parse_body(&bytes))
    }

    async fn notify(&mut self, method: &str) {
        let body = json!({"jsonrpc": "2.0", "method": method});
        let resp = self.app.clone().oneshot(self.request(body)).await.unwrap();
        assert!(resp.status().is_success(), "{}", resp.status());
    }

    async fn initialize(&mut self) -> Value {
        let (status, v) = self
            .rpc(
                "initialize",
                json!({"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        self.notify("notifications/initialized").await;
        v["result"].clone()
    }

    async fn call(&mut self, name: &str, args: Value) -> Value {
        let (status, v) = self
            .rpc("tools/call", json!({"name": name, "arguments": args}))
            .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        v
    }
}

/// Plain JSON, or the JSON-RPC message inside an SSE stream (rmcp answers
/// session-mode POSTs with `data:` frames).
fn parse_body(bytes: &[u8]) -> Value {
    if let Ok(v) = serde_json::from_slice::<Value>(bytes) {
        return v;
    }
    let text = String::from_utf8_lossy(bytes);
    let mut last = Value::String(text.to_string());
    for line in text.lines() {
        if let Some(data) = line.strip_prefix("data:")
            && let Ok(v) = serde_json::from_str::<Value>(data.trim())
            && (v.get("result").is_some() || v.get("error").is_some())
        {
            last = v;
        }
    }
    last
}

fn structured(v: &Value) -> Value {
    assert!(v["result"].is_object(), "not a result: {v}");
    assert_ne!(v["result"]["isError"], true, "tool error: {v}");
    v["result"]["structuredContent"].clone()
}

fn is_error(v: &Value) -> bool {
    v["error"].is_object() || v["result"]["isError"] == true
}

fn error_text(v: &Value) -> String {
    if let Some(m) = v["error"]["message"].as_str() {
        return m.to_string();
    }
    v["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or("")
        .to_string()
}

async fn make_token(t: &TestDb, scopes: &[&str], user_id: Option<i32>) -> String {
    let n = token::generate();
    t.db.create_token(
        "t",
        &n.hash,
        &scopes.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        user_id,
        None,
    )
    .await
    .unwrap();
    n.secret
}

struct Home {
    alice: User,
    bob: User,
}

async fn seed(t: &TestDb) -> Home {
    let h = t.db.create_household("home").await.unwrap();
    let alice =
        t.db.upsert_user(
            "alice",
            Some("alice@example.com"),
            Some("Alice"),
            "Europe/Warsaw",
        )
        .await
        .unwrap();
    let bob =
        t.db.upsert_user("bob", Some("bob@example.com"), Some("Bob"), "Europe/Warsaw")
            .await
            .unwrap();
    let alice = t.db.set_user_household(alice.id, Some(h.id)).await.unwrap();
    let bob = t.db.set_user_household(bob.id, Some(h.id)).await.unwrap();
    t.db.insert_food(crate::db::NewFood {
        household_id: h.id,
        name: "Lentil curry".into(),
        aliases: vec!["dal".into()],
        portion: "1 bowl (350 g)".into(),
        kcal: 520,
        protein_g: Some(24),
        created_by: alice.id,
    })
    .await
    .unwrap();
    Home { alice, bob }
}

#[tokio::test]
async fn needs_a_bearer_token() {
    let t = db_or_skip!();
    let app = app(&t).await;
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header(header::HOST, "koryto.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let mut bogus = Client::new(app, "ko_nope".into());
    let (status, _) = bogus.rpc("initialize", json!({})).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    t.finish().await;
}

#[tokio::test]
async fn lists_tools_and_reads_data() {
    let t = db_or_skip!();
    let home = seed(&t).await;
    let token = make_token(&t, &["read"], Some(home.alice.id)).await;
    let mut c = Client::new(app(&t).await, token);
    let info = c.initialize().await;
    assert_eq!(info["serverInfo"]["name"], "koryto");
    assert!(
        info["instructions"]
            .as_str()
            .unwrap()
            .contains("search_foods first")
    );

    let (status, v) = c.rpc("tools/list", json!({})).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let tools = v["result"]["tools"].as_array().unwrap();
    let mut names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "add_food",
            "archive_food",
            "get_day",
            "get_summary",
            "log_activity",
            "log_meal",
            "log_weight",
            "search_foods",
            "set_location",
            "set_target",
            "update_activity",
            "update_food",
            "update_meal",
            "update_weight",
            "void_entry",
            "whoami",
        ]
    );
    let log = tools.iter().find(|t| t["name"] == "log_meal").unwrap();
    assert!(log["inputSchema"]["properties"]["for_users"].is_object());
    assert!(log["description"].as_str().unwrap().contains("write scope"));
    let void = tools.iter().find(|t| t["name"] == "void_entry").unwrap();
    assert!(void["description"].as_str().unwrap().contains("edit scope"));

    let me = structured(&c.call("whoami", json!({})).await);
    assert_eq!(me["you"]["name"], "Alice");
    assert_eq!(me["household"], "home");
    assert_eq!(me["members"].as_array().unwrap().len(), 2);
    assert_eq!(me["timezone"], "Europe/Warsaw");
    assert_eq!(me["expenditure"]["basis"], "none");
    assert_eq!(me["scopes"], json!(["read"]));

    let foods = structured(&c.call("search_foods", json!({"query": "DAL"})).await);
    assert_eq!(foods["items"][0]["name"], "Lentil curry");
    let all = structured(&c.call("search_foods", json!({})).await);
    assert_eq!(all["items"].as_array().unwrap().len(), 1);

    let day = structured(&c.call("get_day", json!({"user": "bob"})).await);
    assert_eq!(day["user"], "Bob");
    assert_eq!(day["logged"], false);
    assert_eq!(day["kcal"], 0);
    let bad = c.call("get_day", json!({"user": "carol"})).await;
    assert!(is_error(&bad), "{bad}");
    assert!(error_text(&bad).contains("household"));
    let bad = c.call("get_day", json!({"date": "yesterday"})).await;
    assert!(is_error(&bad), "{bad}");

    // Read scope cannot log.
    let refused = c.call("log_weight", json!({"weight_kg": "82.4"})).await;
    assert!(is_error(&refused), "{refused}");
    assert!(error_text(&refused).contains("read scope"));
    t.finish().await;
}

#[tokio::test]
async fn logs_for_two_people_confirms_estimates_and_voids() {
    let t = db_or_skip!();
    let home = seed(&t).await;
    let token = make_token(&t, &["read", "write", "edit"], Some(home.alice.id)).await;
    let mut c = Client::new(app(&t).await, token);
    c.initialize().await;

    // An estimate without confirmation is a preview, not a write.
    let v = c
        .call(
            "log_meal",
            json!({"description": "two eggs on toast", "kcal": 420, "protein_g": 22}),
        )
        .await;
    assert!(is_error(&v), "{v}");
    assert!(error_text(&v).contains("confirmed=true"));
    let day = structured(&c.call("get_day", json!({})).await);
    assert_eq!(day["kcal"], 0);

    let v = structured(
        &c.call(
            "log_meal",
            json!({"description": "two eggs on toast", "kcal": 420, "protein_g": 22, "confirmed": true, "eaten_at": "2026-09-04 08:00"}),
        )
        .await,
    );
    assert_eq!(v["meals"].as_array().unwrap().len(), 1);
    assert_eq!(v["meals"][0]["user"], "Alice");
    assert_eq!(v["meals"][0]["day"], "2026-09-04");
    assert_eq!(v["meals"][0]["eaten_at"], "2026-09-04T08:00:00+02:00");
    assert_eq!(v["meals"][0]["source"], "estimate");
    assert_eq!(v["days"][0]["kcal"], 420);
    let eggs = v["meals"][0]["id"].as_i64().unwrap();

    // A saved food needs no confirmation; one dinner for both people.
    let v = structured(
        &c.call(
            "log_meal",
            json!({"food": "dal", "portions": "1.5", "for_users": ["alice", "Bob"], "eaten_at": "2026-09-04 19:30"}),
        )
        .await,
    );
    let meals = v["meals"].as_array().unwrap();
    assert_eq!(meals.len(), 2);
    assert_eq!(meals[0]["kcal"], 780);
    assert_eq!(meals[0]["protein_g"], 36);
    assert_eq!(meals[0]["source"], "food");
    assert_eq!(meals[0]["description"], "Lentil curry");
    assert_eq!(meals[1]["user"], "Bob");
    assert_eq!(v["days"][0]["kcal"], 1200);
    assert_eq!(v["days"][1]["kcal"], 780);
    let unknown = c.call("log_meal", json!({"food": "pizza"})).await;
    assert!(is_error(&unknown), "{unknown}");
    assert!(error_text(&unknown).contains("search_foods"));

    // Weight and sport go straight in; sport does not touch the balance.
    let w = structured(
        &c.call(
            "log_weight",
            json!({"weight_kg": "82,4", "measured_at": "2026-09-04 07:00"}),
        )
        .await,
    );
    assert_eq!(w["weight_kg"], "82.4");
    assert_eq!(w["day"], "2026-09-04");
    let a = structured(
        &c.call(
            "log_activity",
            json!({"kind": "Run", "duration": "1h30", "kcal": 600, "started_at": "2026-09-04 17:00", "for_user": "bob@example.com"}),
        )
        .await,
    );
    assert_eq!(a["user"], "Bob");
    assert_eq!(a["duration"], "1h30");
    let bobs = structured(
        &c.call("get_day", json!({"user": "bob", "date": "2026-09-04"}))
            .await,
    );
    assert_eq!(bobs["kcal"], 780);
    assert_eq!(bobs["sport_minutes"], 90);

    // A target from that day gives a balance; the summary sees everything.
    let tgt = structured(
        &c.call(
            "set_target",
            json!({"kcal": 1800, "protein_g": 120, "from": "2026-09-01"}),
        )
        .await,
    );
    assert_eq!(tgt["since"], "2026-09-01");
    let mine = structured(&c.call("get_day", json!({"date": "2026-09-04"})).await);
    assert_eq!(mine["balance"], 1200 - 1800);
    let s = structured(
        &c.call(
            "get_summary",
            json!({"from": "2026-09-01", "to": "2026-09-07"}),
        )
        .await,
    );
    assert_eq!(s["days"], 7);
    assert_eq!(s["logged_days"], 1);
    assert_eq!(s["mean_kcal"], 1200);
    assert_eq!(s["mean_balance"], -600);
    assert_eq!(s["weight_last_kg"], "82.4");
    assert_eq!(s["expenditure"]["basis"], "none");
    assert_eq!(s["rows"].as_array().unwrap().len(), 7);
    assert_eq!(s["rows"][3]["kcal"], 1200);
    assert!(s["rows"][0]["kcal"].is_null());

    // Fix the eggs, then undo them.
    let m = structured(
        &c.call("update_meal", json!({"id": eggs, "kcal": 380}))
            .await,
    );
    assert_eq!(m["kcal"], 380);
    assert_eq!(m["source"], "manual");
    let m = structured(
        &c.call(
            "update_meal",
            json!({"id": eggs, "food": "dal", "portions": "0.5"}),
        )
        .await,
    );
    assert_eq!(m["kcal"], 260);
    assert_eq!(m["source"], "food");
    let m = structured(
        &c.call("update_meal", json!({"id": eggs, "day": "2026-09-03"}))
            .await,
    );
    assert_eq!(m["day"], "2026-09-03");
    let v = structured(
        &c.call("void_entry", json!({"kind": "meal", "id": eggs}))
            .await,
    );
    assert_eq!(v["done"], true);
    let again = c
        .call("void_entry", json!({"kind": "meal", "id": eggs}))
        .await;
    assert!(is_error(&again), "{again}");
    let bad_kind = c
        .call("void_entry", json!({"kind": "snack", "id": eggs}))
        .await;
    assert!(is_error(&bad_kind), "{bad_kind}");
    let mine = structured(&c.call("get_day", json!({"date": "2026-09-04"})).await);
    assert_eq!(mine["kcal"], 780);

    // Travel: New York since the 4th at noon UTC. The evening curry (19:30
    // Warsaw, 13:30 New York) keeps its day but is relabelled with the new
    // zone, so one entry changes; today is now on the New York clock.
    let loc = structured(
        &c.call(
            "set_location",
            json!({"timezone": "America/New_York", "from": "2026-09-04T12:00:00Z"}),
        )
        .await,
    );
    assert_eq!(loc["timezone"], "America/New_York");
    assert_eq!(loc["entries_recomputed"], 1);
    let me = structured(&c.call("whoami", json!({})).await);
    assert_eq!(me["timezone"], "America/New_York");
    assert_eq!(me["target"]["kcal"], 1800);
    let bad_zone = c
        .call("set_location", json!({"timezone": "Mars/Base"}))
        .await;
    assert!(is_error(&bad_zone), "{bad_zone}");

    // Foods: add needs confirmation, then edit and archive.
    let v = c
        .call(
            "add_food",
            json!({"name": "Oats", "portion": "50 g dry", "kcal": 190, "protein_g": 7}),
        )
        .await;
    assert!(is_error(&v), "{v}");
    let f = structured(
        &c.call("add_food", json!({"name": "Oats", "portion": "50 g dry", "kcal": 190, "protein_g": 7, "aliases": ["porridge"], "confirmed": true}))
            .await,
    );
    let oats = f["id"].as_i64().unwrap();
    let f = structured(
        &c.call("update_food", json!({"id": oats, "kcal": 185}))
            .await,
    );
    assert_eq!(f["kcal"], 185);
    let f = structured(&c.call("archive_food", json!({"id": oats})).await);
    assert_eq!(f["archived"], true);
    let hits = structured(&c.call("search_foods", json!({"query": "porridge"})).await);
    assert_eq!(hits["items"].as_array().unwrap().len(), 0);
    t.finish().await;
}

#[tokio::test]
async fn scopes_and_delegation() {
    let t = db_or_skip!();
    let home = seed(&t).await;
    let writer = make_token(&t, &["read", "write"], Some(home.alice.id)).await;
    let mut w = Client::new(app(&t).await, writer);
    w.initialize().await;
    let v = structured(&w.call("log_weight", json!({"weight_kg": "82"})).await);
    let id = v["id"].as_i64().unwrap();
    let refused = w
        .call("update_weight", json!({"id": id, "weight_kg": "81"}))
        .await;
    assert!(is_error(&refused), "{refused}");
    assert!(error_text(&refused).contains("edit scope"));
    let refused = w
        .call("void_entry", json!({"kind": "weight", "id": id}))
        .await;
    assert!(is_error(&refused), "{refused}");
    let refused = w.call("set_target", json!({"kcal": 2000})).await;
    assert!(is_error(&refused), "{refused}");

    // A delegate token acts as the named person and needs the header.
    let delegate = make_token(&t, &["read", "write", "delegate"], None).await;
    let mut d = Client::new(app(&t).await, delegate);
    let (status, _) = d.rpc("initialize", json!({})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    d.delegate_user = Some("bob@example.com".into());
    d.initialize().await;
    let me = structured(&d.call("whoami", json!({})).await);
    assert_eq!(me["you"]["name"], "Bob");
    let v = structured(
        &d.call(
            "log_meal",
            json!({"food": "Lentil curry", "for_users": ["me"]}),
        )
        .await,
    );
    assert_eq!(v["meals"][0]["user"], "Bob");
    assert_eq!(v["meals"][0]["kcal"], 520);
    let _ = home.bob;
    t.finish().await;
}
