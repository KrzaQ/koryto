//! A fresh database per test. `TEST_DATABASE_URL` must point at a Postgres
//! whose database name ends in `_test`; that database is only used as the
//! maintenance connection, each test gets its own `<name>_<random>` database
//! that is dropped at the end.

use sqlx::postgres::PgPoolOptions;
use sqlx::{Connection, PgConnection};

use super::Db;

pub struct TestDb {
    pub db: Db,
    admin_url: String,
    name: String,
}

/// `None` (after printing why) when no database is configured, so callers can
/// return early. Panics if the URL is configured but unusable: a broken
/// harness must fail loudly, not look like a skip.
pub async fn test_db() -> Option<TestDb> {
    let Ok(base) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIPPED: TEST_DATABASE_URL not set (run via scripts/test-db.sh)");
        return None;
    };
    let base_url: url::Url = base.parse().expect("TEST_DATABASE_URL must be a URL");
    let base_name = base_url.path().trim_start_matches('/').to_string();
    assert!(
        base_name.ends_with("_test"),
        "refusing TEST_DATABASE_URL database {base_name:?}: the name must end in _test"
    );

    let mut suffix = [0u8; 4];
    getrandom::fill(&mut suffix).expect("random");
    let name = format!("{base_name}_{}", hex::encode(suffix));

    let mut admin = PgConnection::connect(&base)
        .await
        .expect("connect to TEST_DATABASE_URL");
    sqlx::query(&format!("CREATE DATABASE \"{name}\""))
        .execute(&mut admin)
        .await
        .expect("create test database");
    admin.close().await.ok();

    let mut url = base_url.clone();
    url.set_path(&format!("/{name}"));
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(url.as_str())
        .await
        .expect("connect test db");
    let db = Db::from_pool(pool);
    db.migrate().await.expect("migrate test db");
    Some(TestDb {
        db,
        admin_url: base,
        name,
    })
}

impl TestDb {
    /// Drop the per-test database. Not called on panic; the throwaway server
    /// scripts/test-db.sh starts is discarded anyway.
    pub async fn finish(self) {
        self.db.pool.close().await;
        if let Ok(mut admin) = PgConnection::connect(&self.admin_url).await {
            let _ = sqlx::query(&format!(
                "DROP DATABASE IF EXISTS \"{}\" WITH (FORCE)",
                self.name
            ))
            .execute(&mut admin)
            .await;
        }
    }
}

/// `let t = db_or_skip!();` at the top of a DB-backed test.
#[macro_export]
macro_rules! db_or_skip {
    () => {
        match $crate::db::test_db::test_db().await {
            Some(t) => t,
            None => return,
        }
    };
}
