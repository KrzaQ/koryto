//! `koryto household`: sharing. Everyone has a household of their own from
//! the first login; joining someone's is what makes two logs one.

use anyhow::{Result, anyhow};
use clap::Subcommand;

use crate::db::Db;

#[derive(Subcommand)]
pub enum HouseholdCommand {
    /// Move a person (by email) into another person's household
    AddMember {
        email: String,
        /// Email of someone already in the household to join
        #[arg(long)]
        to: String,
    },
    /// Move a person back into a household of their own, with a copy of the foods
    RemoveMember { email: String },
    /// Rename a household, given by a member's email
    Rename { email: String, name: String },
    /// List households and their members
    List,
}

async fn person(db: &Db, email: &str) -> Result<crate::db::User> {
    db.find_user_by_email(email)
        .await?
        .ok_or_else(|| anyhow!("{email} has never logged in"))
}

pub async fn run(db: &Db, cmd: HouseholdCommand) -> Result<()> {
    match cmd {
        HouseholdCommand::AddMember { email, to } => {
            let joiner = person(db, &email).await?;
            let host = person(db, &to).await?;
            let household = host
                .household_id
                .ok_or_else(|| anyhow!("{} has no household yet; log in once", host.display()))?;
            let moved = db.move_user(joiner.id, Some(household)).await?;
            let h = db.get_household(household).await?;
            println!(
                "{} is now in {} ({}), with {}",
                moved.display(),
                h.name,
                h.id,
                db.household_members(h.id)
                    .await?
                    .iter()
                    .filter(|m| m.id != moved.id)
                    .map(|m| m.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        HouseholdCommand::RemoveMember { email } => {
            let u = person(db, &email).await?;
            let moved = db.move_user(u.id, None).await?;
            let h = db
                .get_household(moved.household_id.expect("placed"))
                .await?;
            println!("{} is now alone in {} ({})", moved.display(), h.name, h.id);
        }
        HouseholdCommand::Rename { email, name } => {
            let u = person(db, &email).await?;
            let id = u
                .household_id
                .ok_or_else(|| anyhow!("{} has no household", u.display()))?;
            let h = db.rename_household(id, &name).await?;
            println!("household {} is now {}", h.id, h.name);
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
