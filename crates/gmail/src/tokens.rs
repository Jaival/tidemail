//! Refresh-token storage (Windows Credential Manager) and access-token cache.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::{Config, Error};

const KEYRING_SERVICE: &str = "Tidemail";
/// Refresh this long before the token expires.
const REFRESH_MARGIN: Duration = Duration::from_secs(60);

pub trait RefreshStore: Send + Sync {
    fn get(&self, email: &str) -> Result<Option<String>, Error>;
    fn set(&self, email: &str, token: &str) -> Result<(), Error>;
    fn delete(&self, email: &str) -> Result<(), Error>;
}

/// Windows Credential Manager via `keyring`, keyed by account email.
pub struct KeyringStore;

impl KeyringStore {
    fn entry(email: &str) -> Result<keyring::Entry, Error> {
        Ok(keyring::Entry::new(KEYRING_SERVICE, email)?)
    }
}

impl RefreshStore for KeyringStore {
    fn get(&self, email: &str) -> Result<Option<String>, Error> {
        match Self::entry(email)?.get_password() {
            Ok(t) => Ok(Some(t)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn set(&self, email: &str, token: &str) -> Result<(), Error> {
        Ok(Self::entry(email)?.set_password(token)?)
    }

    fn delete(&self, email: &str) -> Result<(), Error> {
        match Self::entry(email)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct OAuthError {
    error: String,
}

/// Access tokens for one account. Cached; refreshed shortly before expiry.
pub struct TokenSource {
    http: reqwest::Client,
    config: Config,
    token_url: String,
    email: String,
    store: Arc<dyn RefreshStore>,
    cache: Mutex<Option<(String, Instant)>>,
}

impl TokenSource {
    pub fn new(
        config: Config,
        token_url: String,
        email: String,
        store: Arc<dyn RefreshStore>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            token_url,
            email,
            store,
            cache: Mutex::new(None),
        }
    }

    pub async fn access_token(&self) -> Result<String, Error> {
        let mut cache = self.cache.lock().await;
        if let Some((token, expires)) = cache.as_ref() {
            if Instant::now() + REFRESH_MARGIN < *expires {
                return Ok(token.clone());
            }
        }
        let refresh = self.store.get(&self.email)?.ok_or(Error::AuthExpired)?;
        let resp = self
            .http
            .post(&self.token_url)
            .form(&[
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh.as_str()),
                ("client_id", self.config.client_id.as_str()),
                ("client_secret", self.config.client_secret.as_str()),
            ])
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await?;
            // `invalid_grant` = revoked, expired or deleted refresh token.
            if status.as_u16() == 400 || status.as_u16() == 401 {
                if let Ok(e) = serde_json::from_str::<OAuthError>(&body) {
                    if e.error == "invalid_grant" || e.error == "invalid_client" {
                        return Err(Error::AuthExpired);
                    }
                }
            }
            return Err(Error::Auth(format!("token refresh {status}: {body}")));
        }
        let r: RefreshResponse = resp.json().await.map_err(|e| Error::Parse(e.to_string()))?;
        *cache = Some((
            r.access_token.clone(),
            Instant::now() + Duration::from_secs(r.expires_in),
        ));
        Ok(r.access_token)
    }

    /// Drop the cached token (after a 401).
    pub async fn invalidate(&self) {
        *self.cache.lock().await = None;
    }
}
