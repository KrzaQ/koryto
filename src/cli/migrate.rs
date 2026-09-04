use anyhow::Result;

use crate::db::Db;

pub async fn run(db: &Db, status_only: bool) -> Result<()> {
    if !status_only {
        db.migrate().await?;
    }
    for (version, description, applied) in db.migration_status().await? {
        println!(
            "{:>4}  {:<9}  {}",
            version,
            if applied { "applied" } else { "pending" },
            description
        );
    }
    Ok(())
}
