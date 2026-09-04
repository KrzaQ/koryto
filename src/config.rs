//! Configuration comes from `KORYTO_*` environment variables only. `serve`
//! needs all of it (step 3); the other subcommands only need the database
//! URL and the house zone.

use anyhow::{Context, Result};

pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn require(name: &str) -> Result<String> {
    var(name).with_context(|| format!("{name} is not set"))
}

pub fn database_url_from_env() -> Result<String> {
    require("KORYTO_DATABASE_URL")
}

/// `KORYTO_TIMEZONE`, an IANA name; Europe/Warsaw when unset. The house
/// zone: the origin location of every new user.
pub fn timezone_from_env() -> Result<chrono_tz::Tz> {
    let name = var("KORYTO_TIMEZONE").unwrap_or_else(|| "Europe/Warsaw".into());
    name.parse()
        .map_err(|e| anyhow::anyhow!("KORYTO_TIMEZONE: {e}"))
}
