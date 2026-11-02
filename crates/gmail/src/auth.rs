//! Installed-app OAuth: PKCE + loopback redirect on `127.0.0.1:<random port>`.

use std::time::Duration;

use oauth2::{
    basic::BasicClient, reqwest as oauth_reqwest, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use serde::Deserialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

use crate::{tokens::RefreshStore, Config, Error, KeyringStore, SCOPE};

/// How long to wait for the user to finish in the browser.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Google endpoints; replaced in tests.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth_url: String,
    pub token_url: String,
    /// Gmail API host root.
    pub api_base: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            api_base: crate::client::GOOGLE_API_BASE.into(),
        }
    }
}

/// Result of a completed browser sign-in. The refresh token is already
/// stored in Credential Manager under `email`.
#[derive(Debug, Clone)]
pub struct SignedIn {
    pub email: String,
    pub display_name: Option<String>,
}

/// Open `url` in the default browser. Library crates cannot use `tauri-plugin-opener`;
/// the app can pass its own opener to [`sign_in_with`].
pub fn open_in_browser(url: &str) -> std::io::Result<()> {
    // rundll32 takes the URL as one argument; `cmd /c start` would mangle `&`.
    std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn()
        .map(drop)
}

/// Run the OAuth PKCE flow in the default browser.
pub async fn sign_in(config: &Config) -> Result<SignedIn, Error> {
    sign_in_with(
        config,
        &Endpoints::default(),
        &KeyringStore,
        open_in_browser,
    )
    .await
}

/// Same flow with explicit endpoints, token store and browser opener.
pub async fn sign_in_with(
    config: &Config,
    endpoints: &Endpoints,
    store: &dyn RefreshStore,
    open: impl FnOnce(&str) -> std::io::Result<()>,
) -> Result<SignedIn, Error> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());

    let bad = |e: &dyn std::fmt::Display| Error::Auth(e.to_string());
    let client = BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_client_secret(ClientSecret::new(config.client_secret.clone()))
        .set_auth_uri(AuthUrl::new(endpoints.auth_url.clone()).map_err(|e| bad(&e))?)
        .set_token_uri(TokenUrl::new(endpoints.token_url.clone()).map_err(|e| bad(&e))?)
        .set_redirect_uri(RedirectUrl::new(redirect).map_err(|e| bad(&e))?);

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(SCOPE.into()))
        .set_pkce_challenge(challenge)
        // offline + consent: always return a refresh token.
        .add_extra_param("access_type", "offline")
        .add_extra_param("prompt", "consent")
        .url();
    open(auth_url.as_str())?;

    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait_for_redirect(&listener, csrf.secret()))
        .await
        .map_err(|_| Error::Auth("timed out waiting for the browser".into()))??;

    let http = oauth_reqwest::ClientBuilder::new()
        .redirect(oauth_reqwest::redirect::Policy::none())
        .build()?;
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request_async(&http)
        .await
        .map_err(|e| bad(&e))?;
    let refresh = token
        .refresh_token()
        .ok_or_else(|| Error::Auth("Google returned no refresh token".into()))?;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Profile {
        email_address: String,
    }
    let profile: Profile = http
        .get(format!("{}/gmail/v1/users/me/profile", endpoints.api_base))
        .bearer_auth(token.access_token().secret())
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    store.set(&profile.email_address, refresh.secret())?;
    Ok(SignedIn {
        email: profile.email_address,
        // The gmail.modify scope does not expose a display name.
        display_name: None,
    })
}

/// Accept connections until one carries the OAuth redirect; return the code.
async fn wait_for_redirect(listener: &TcpListener, state: &str) -> Result<String, Error> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = stream.read(&mut buf).await?;
        let request = String::from_utf8_lossy(&buf[..n]);
        let target = request
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("/");
        let params: Vec<(String, String)> = url::Url::parse(&format!("http://127.0.0.1{target}"))
            .map(|u| u.query_pairs().into_owned().collect())
            .unwrap_or_default();
        let get = |k: &str| params.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());

        let result = match (get("code"), get("error")) {
            (_, Some(e)) => Some(Err(Error::Auth(format!("Google denied access: {e}")))),
            (Some(code), None) if get("state").as_deref() == Some(state) => Some(Ok(code)),
            (Some(_), None) => Some(Err(Error::Auth("state mismatch".into()))),
            // Favicon or speculative connections: ignore.
            (None, None) => None,
        };
        let (status, page) = match &result {
            Some(Ok(_)) => (
                "200 OK",
                "Signed in. You can close this tab and return to Tidemail.",
            ),
            Some(Err(_)) => (
                "400 Bad Request",
                "Sign-in failed. Return to Tidemail for details.",
            ),
            None => ("404 Not Found", ""),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
            page.len()
        );
        // The browser tab may already be gone; the code is what matters.
        let _ = stream.write_all(response.as_bytes()).await;
        if let Some(r) = result {
            return r;
        }
    }
}

/// Remove the stored refresh token for `email`.
pub fn sign_out(email: &str) -> Result<(), Error> {
    KeyringStore.delete(email)
}
