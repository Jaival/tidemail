//! Gmail API access: OAuth (installed app, PKCE, loopback redirect),
//! refresh tokens in Windows Credential Manager, `config.toml`, REST client,
//! batch helper (`/batch/gmail/v1`, max 50 calls).
//!
//! Phase 0 state: stub. Phase 1 (sign-in agent) owns this crate. Keep the
//! public signatures below; add the API client surface the sync engine needs.

use std::path::PathBuf;

pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.modify";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not implemented yet")]
    NotImplemented,
    #[error("config: {0}")]
    Config(String),
}

/// `client_id` / `client_secret` from the user's own Google Cloud project.
#[derive(Debug, Clone)]
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
    Err(Error::NotImplemented)
}

/// Result of a completed browser sign-in. The refresh token is already
/// stored in Credential Manager under `email`.
#[derive(Debug, Clone)]
pub struct SignedIn {
    pub email: String,
    pub display_name: Option<String>,
}

/// Run the OAuth PKCE flow in the default browser.
pub async fn sign_in(config: &Config) -> Result<SignedIn, Error> {
    let _ = config;
    Err(Error::NotImplemented)
}

/// Remove the stored refresh token for `email`.
pub fn sign_out(email: &str) -> Result<(), Error> {
    let _ = email;
    Err(Error::NotImplemented)
}
