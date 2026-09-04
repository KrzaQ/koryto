//! `koryto recompute-days`: re-derive zone and day for every non-overridden
//! entry, for one user or everyone.

use anyhow::{Result, anyhow};

use crate::app::time;
use crate::db::Db;

pub async fn run(db: &Db, email: Option<String>) -> Result<()> {
    let users = match email {
        Some(e) => vec![
            db.find_user_by_email(&e)
                .await?
                .ok_or_else(|| anyhow!("{e} has never logged in"))?,
        ],
        None => db.list_users().await?,
    };
    for u in users {
        let changed = time::recompute_days(db, &u).await?;
        println!("{}: {changed} entries changed", u.display());
    }
    Ok(())
}
