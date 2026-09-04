//! Configuration comes from `KORYTO_*` environment variables only.

use std::net::SocketAddr;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: SocketAddr,
    /// Public origin of the deployment; the OIDC redirect URI derives from it.
    pub public_url: url::Url,
    pub secret: Vec<u8>,
    pub auth: AuthMode,
    pub auto_migrate: bool,
    /// The house zone: the origin location of every new user.
    pub timezone: chrono_tz::Tz,
}

#[derive(Debug, Clone)]
pub enum AuthMode {
    Oidc(OidcConfig),
    /// Everyone is a fixed user in a fixed household. Loopback only.
    Dev,
}

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// Only members of this authentik group may log in; None lets any
    /// account in, and household membership does the gating.
    pub group: Option<String>,
}

pub fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn require(name: &str) -> Result<String> {
    var(name).with_context(|| format!("{name} is not set"))
}

pub fn database_url_from_env() -> Result<String> {
    require("KORYTO_DATABASE_URL")
}

/// `KORYTO_TIMEZONE`, an IANA name; Europe/Warsaw when unset.
pub fn timezone_from_env() -> Result<chrono_tz::Tz> {
    let name = var("KORYTO_TIMEZONE").unwrap_or_else(|| "Europe/Warsaw".into());
    name.parse()
        .map_err(|e| anyhow::anyhow!("KORYTO_TIMEZONE: {e}"))
}

impl Config {
    /// Everything `serve` needs.
    pub fn from_env() -> Result<Self> {
        let database_url = database_url_from_env()?;
        let bind = var("KORYTO_BIND")
            .unwrap_or_else(|| "0.0.0.0:8000".into())
            .parse()
            .context("KORYTO_BIND must be host:port")?;
        let public_url: url::Url = require("KORYTO_PUBLIC_URL")?
            .parse()
            .context("KORYTO_PUBLIC_URL must be an absolute URL")?;
        let auth = match var("KORYTO_AUTH").as_deref().unwrap_or("oidc") {
            "dev" => {
                check_dev_url(&public_url)?;
                AuthMode::Dev
            }
            "oidc" => AuthMode::Oidc(OidcConfig {
                issuer: require("KORYTO_OIDC_ISSUER")?,
                client_id: require("KORYTO_OIDC_CLIENT_ID")?,
                client_secret: require("KORYTO_OIDC_CLIENT_SECRET")?,
                group: var("KORYTO_OIDC_GROUP"),
            }),
            other => bail!("KORYTO_AUTH must be oidc or dev, got {other:?}"),
        };
        let secret = match (&auth, var("KORYTO_SECRET")) {
            (_, Some(s)) if s.len() >= 32 => s.into_bytes(),
            (_, Some(_)) => bail!("KORYTO_SECRET must be at least 32 bytes"),
            // Dev mode is loopback-only and stateless, a random per-process key is fine.
            (AuthMode::Dev, None) => {
                let mut buf = vec![0u8; 64];
                getrandom::fill(&mut buf).map_err(|e| anyhow::anyhow!("random secret: {e}"))?;
                buf
            }
            (AuthMode::Oidc(_), None) => bail!("KORYTO_SECRET is not set"),
        };
        Ok(Self {
            database_url,
            bind,
            public_url,
            secret,
            auth,
            auto_migrate: var("KORYTO_AUTO_MIGRATE").as_deref() != Some("0"),
            timezone: timezone_from_env()?,
        })
    }
}

/// Dev auth logs everyone in as the same person, so it must never be
/// reachable from anywhere but the developer's own machine.
pub fn check_dev_url(public_url: &url::Url) -> Result<()> {
    let host = public_url.host_str().unwrap_or("");
    if public_url.scheme() == "http" && (host == "localhost" || host == "127.0.0.1") {
        Ok(())
    } else {
        bail!(
            "KORYTO_AUTH=dev is only allowed with KORYTO_PUBLIC_URL on http://localhost or http://127.0.0.1, got {public_url}"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_guard() {
        assert!(check_dev_url(&"http://localhost:8000".parse().unwrap()).is_ok());
        assert!(check_dev_url(&"http://127.0.0.1:1234".parse().unwrap()).is_ok());
        assert!(check_dev_url(&"https://localhost:8000".parse().unwrap()).is_err());
        assert!(check_dev_url(&"http://koryto.int.krzaq.cc".parse().unwrap()).is_err());
        assert!(check_dev_url(&"http://0.0.0.0:8000".parse().unwrap()).is_err());
    }
}
