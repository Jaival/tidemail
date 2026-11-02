//! Gmail API access: OAuth (installed app, PKCE, loopback redirect),
//! refresh tokens in Windows Credential Manager, `config.toml`, REST client,
//! batch helper (`/batch/gmail/v1`, max 50 calls), quota and MIME helpers.

mod auth;
mod batch;
mod client;
mod mime;
mod quota;
mod tokens;
mod types;

use std::path::{Path, PathBuf};

pub use auth::{open_in_browser, sign_in, sign_in_with, sign_out, Endpoints, SignedIn};
pub use batch::{BatchRequest, BatchResponse, MAX_BATCH};
pub use client::{GmailClient, MessageFormat, RetryPolicy};
pub use mime::{decode_base64url, extract_bodies, list_attachments, AttachmentInfo, Bodies};
pub use quota::{cost, Quota};
pub use tokens::{KeyringStore, RefreshStore, TokenSource};
pub use types::*;

pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config: {0}")]
    Config(String),
    /// Refresh token missing, revoked or invalid. Sync maps this to `authError`.
    #[error("Google sign-in expired or revoked; sign in again")]
    AuthExpired,
    #[error("sign-in failed: {0}")]
    Auth(String),
    /// `history.list` returned 404: the start history id is too old.
    #[error("history id expired")]
    HistoryExpired,
    #[error("Gmail API {status} ({reason}): {message}")]
    Api {
        status: u16,
        reason: String,
        message: String,
    },
    /// Rate-limit retries exhausted.
    #[error("Gmail rate limit: retries exhausted")]
    RateLimited,
    #[error("unexpected response: {0}")]
    Parse(String),
    #[error("credential store: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// `client_id` / `client_secret` from the user's own Google Cloud project.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct Config {
    pub client_id: String,
    pub client_secret: String,
}

/// `%LOCALAPPDATA%\Tidemail\config.toml`.
pub fn config_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map_or_else(|| PathBuf::from("."), PathBuf::from);
    base.join("Tidemail").join("config.toml")
}

pub fn load_config() -> Result<Config, Error> {
    load_config_from(&config_path())
}

pub fn load_config_from(path: &Path) -> Result<Config, Error> {
    let at = path.display();
    let text = std::fs::read_to_string(path).map_err(|e| {
        Error::Config(format!(
            "cannot read {at}: {e}. Create it with client_id and client_secret (see README)"
        ))
    })?;
    let config: Config = toml::from_str(&text).map_err(|e| {
        Error::Config(format!(
            "invalid {at}: {e}. Expected client_id and client_secret strings"
        ))
    })?;
    if config.client_id.trim().is_empty() || config.client_secret.trim().is_empty() {
        return Err(Error::Config(format!(
            "{at}: client_id and client_secret must not be empty"
        )));
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_errors_name_the_path() {
        let dir = std::env::temp_dir().join(format!("tidemail-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        let err = load_config_from(&p).unwrap_err().to_string();
        assert!(err.contains("config.toml"), "{err}");

        std::fs::write(&p, "client_id = \"a\"\n").unwrap();
        assert!(load_config_from(&p)
            .unwrap_err()
            .to_string()
            .contains("config.toml"));

        std::fs::write(&p, "client_id = \"\"\nclient_secret = \"s\"\n").unwrap();
        assert!(load_config_from(&p).is_err());

        std::fs::write(&p, "client_id = \"a\"\nclient_secret = \"s\"\n").unwrap();
        assert_eq!(load_config_from(&p).unwrap().client_id, "a");
    }
}
