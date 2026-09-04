//! `koryto user`: who has logged in.

use anyhow::Result;
use clap::Subcommand;

use crate::db::Db;

#[derive(Subcommand)]
pub enum UserCommand {
    /// List users with their household
    List,
}

pub async fn run(db: &Db, cmd: UserCommand) -> Result<()> {
    match cmd {
        UserCommand::List => {
            for u in db.list_users().await? {
                let household = match u.household_id {
                    Some(id) => db.get_household(id).await?.name,
                    None => "-".into(),
                };
                let last = u
                    .last_login_at
                    .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "never".into());
                println!(
                    "{:>4}  {:<24} {:<28} {:<12} {}",
                    u.id,
                    u.display(),
                    u.email.as_deref().unwrap_or(""),
                    household,
                    last
                );
            }
        }
    }
    Ok(())
}
