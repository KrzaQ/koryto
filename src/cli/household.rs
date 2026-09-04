//! `koryto household`: the only place membership is managed.

use anyhow::{Result, anyhow};
use clap::Subcommand;

use crate::db::Db;

#[derive(Subcommand)]
pub enum HouseholdCommand {
    /// Create a household
    Create { name: String },
    /// Put a user (by email, must have logged in once) into a household
    AddMember { household: String, email: String },
    /// Take a user out of their household
    RemoveMember { email: String },
    /// List households and their members
    List,
}

pub async fn run(db: &Db, cmd: HouseholdCommand) -> Result<()> {
    match cmd {
        HouseholdCommand::Create { name } => {
            let h = db.create_household(&name).await?;
            println!("created household {} ({})", h.id, h.name);
        }
        HouseholdCommand::AddMember { household, email } => {
            let h = db.find_household(&household).await?;
            let u = db
                .find_user_by_email(&email)
                .await?
                .ok_or_else(|| anyhow!("{email} has never logged in"))?;
            db.set_user_household(u.id, Some(h.id)).await?;
            println!("{} is now in {}", u.display(), h.name);
        }
        HouseholdCommand::RemoveMember { email } => {
            let u = db
                .find_user_by_email(&email)
                .await?
                .ok_or_else(|| anyhow!("{email} has never logged in"))?;
            db.set_user_household(u.id, None).await?;
            println!("{} is in no household", u.display());
        }
        HouseholdCommand::List => {
            for h in db.list_households().await? {
                println!("{:>4}  {}", h.id, h.name);
                for m in db.household_members(h.id).await? {
                    println!(
                        "      {}  {}",
                        m.display(),
                        m.email.as_deref().unwrap_or("")
                    );
                }
            }
        }
    }
    Ok(())
}
