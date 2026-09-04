//! Bearer tokens for MCP clients. Only the SHA-256 of the secret is stored;
//! the secret itself is shown once at creation.

use base64::Engine;
use sha2::{Digest, Sha256};

pub const PREFIX: &str = "ko_";
pub const SCOPE_READ: &str = "read";
/// Log entries, add foods, set a location: additive.
pub const SCOPE_WRITE: &str = "write";
/// Change and void entries, targets, foods, the profile.
pub const SCOPE_EDIT: &str = "edit";
/// A trusted gateway (Open WebUI) that names the acting user per request in
/// the `X-Koryto-User` header. Without the header the token is useless.
pub const SCOPE_DELEGATE: &str = "delegate";
const ORDER: [&str; 4] = [SCOPE_READ, SCOPE_WRITE, SCOPE_EDIT, SCOPE_DELEGATE];

pub struct NewToken {
    pub secret: String,
    pub hash: String,
}

pub fn generate() -> NewToken {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("os randomness");
    let secret = format!(
        "{PREFIX}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    );
    let hash = hash(&secret);
    NewToken { secret, hash }
}

pub fn hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

/// Comma-separated scopes in canonical order. `edit` implies `write`, which
/// implies `read`; `delegate` is orthogonal. Anything else is rejected.
pub fn parse_scopes(s: &str) -> Result<Vec<String>, String> {
    let mut have = [false; 4];
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        match ORDER.iter().position(|o| *o == part) {
            Some(i) => have[i] = true,
            None => {
                return Err(format!(
                    "unknown scope {part:?}; use read, write, edit or delegate"
                ));
            }
        }
    }
    if have[2] {
        have[1] = true;
    }
    if have[1] {
        have[0] = true;
    }
    if !have[0] {
        return Err("at least one scope is required".into());
    }
    Ok(ORDER
        .iter()
        .zip(have)
        .filter(|(_, h)| *h)
        .map(|(o, _)| o.to_string())
        .collect())
}

pub fn is_delegate(scopes: &[String]) -> bool {
    scopes.iter().any(|s| s == SCOPE_DELEGATE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_unique_and_hash_stably() {
        let a = generate();
        let b = generate();
        assert_ne!(a.secret, b.secret);
        assert!(a.secret.starts_with("ko_"));
        assert_eq!(a.hash, hash(&a.secret));
        assert_eq!(a.hash.len(), 64);
    }

    #[test]
    fn scopes() {
        assert_eq!(parse_scopes("read").unwrap(), ["read"]);
        assert_eq!(parse_scopes("read,write").unwrap(), ["read", "write"]);
        assert_eq!(parse_scopes("write").unwrap(), ["read", "write"]);
        assert_eq!(parse_scopes("edit").unwrap(), ["read", "write", "edit"]);
        assert_eq!(
            parse_scopes("delegate,write").unwrap(),
            ["read", "write", "delegate"]
        );
        assert_eq!(parse_scopes(" read , read ").unwrap(), ["read"]);
        assert!(parse_scopes("admin").is_err());
        assert!(parse_scopes("delegate").is_err());
        assert!(parse_scopes("").is_err());
        assert!(is_delegate(&parse_scopes("read,delegate").unwrap()));
    }
}
