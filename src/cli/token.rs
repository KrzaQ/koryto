//! `koryto token`: bearer tokens for MCP clients.

use anyhow::{Result, anyhow, bail};
use clap::Subcommand;

use crate::db::Db;
use crate::domain::token;

#[derive(Subcommand)]
pub enum TokenCommand {
    /// Create a token; the secret is printed once and never stored
    Create {
        name: String,
        /// Comma-separated: read, write, edit, delegate
        #[arg(long, default_value = "read")]
        scopes: String,
        /// Email of the person a personal token acts as (not for delegate tokens)
        #[arg(long)]
        user: Option<String>,
    },
    /// List tokens
    List,
    /// Revoke a token by id
    Revoke { id: i32 },
}

pub async fn run(db: &Db, cmd: TokenCommand) -> Result<()> {
    match cmd {
        TokenCommand::Create { name, scopes, user } => {
            let scopes = token::parse_scopes(&scopes).map_err(|e| anyhow!(e))?;
            let user_id = match (token::is_delegate(&scopes), user) {
                (true, Some(_)) => {
                    bail!("a delegate token acts as whoever X-Koryto-User names; drop --user")
                }
                (true, None) => None,
                (false, None) => bail!("a personal token needs --user EMAIL"),
                (false, Some(email)) => Some(
                    db.find_user_by_email(&email)
                        .await?
                        .ok_or_else(|| anyhow!("{email} has never logged in"))?
                        .id,
                ),
            };
            let new = token::generate();
            let t = db
                .create_token(&name, &new.hash, &scopes, user_id, None)
                .await?;
            println!("token {} ({}) scopes {}", t.id, t.name, t.scopes.join(","));
            println!("{}", new.secret);
            println!("This secret is not stored and cannot be shown again.");
        }
        TokenCommand::List => {
            for t in db.list_tokens().await? {
                let state = match (&t.revoked_at, &t.last_used_at) {
                    (Some(r), _) => format!("revoked {}", r.format("%Y-%m-%d")),
                    (None, Some(u)) => format!("last used {}", u.format("%Y-%m-%d %H:%M")),
                    (None, None) => "never used".to_string(),
                };
                let who = match t.user_id {
                    Some(id) => db.get_user(id).await?.display().to_string(),
                    None => "delegate".to_string(),
                };
                println!(
                    "{:>4}  {:<20} {:<24} {:<20} {}",
                    t.id,
                    t.name,
                    t.scopes.join(","),
                    who,
                    state
                );
            }
        }
        TokenCommand::Revoke { id } => {
            db.revoke_token(id).await?;
            println!("revoked token {id}");
        }
    }
    Ok(())
}
