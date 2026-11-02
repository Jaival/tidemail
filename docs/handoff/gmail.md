# Handoff: gmail (`crates/gmail`, branch `phase1-gmail`)

## Done

- `load_config()` / `load_config_from(path)`: TOML `client_id`, `client_secret`; errors name the path; empty values rejected.
- `sign_in(config)` (kept signature): PKCE (S256) + loopback `127.0.0.1:<random port>`, scope `gmail.modify`, `access_type=offline&prompt=consent`. Ignores stray connections (favicon), checks `state`, 5 min timeout. Email from `users.getProfile`; `display_name` is always `None` (the scope has no profile access).
- `sign_in_with(config, &Endpoints, &dyn RefreshStore, opener)`: same flow with injectable endpoints, store and browser opener. Default opener `open_in_browser` uses `rundll32 url.dll,FileProtocolHandler` (no extra dependency, no tauri). The app may pass a closure that calls `tauri-plugin-opener`.
- `sign_out(email)`: deletes the Credential Manager entry (service `Tidemail`, user = email). Missing entry is OK.
- `TokenSource::new(config, token_url, email, Arc<dyn RefreshStore>)`: access-token cache, refresh 60 s before expiry. Revoked/missing refresh token (`invalid_grant`) -> `Error::AuthExpired`. Use `Endpoints::default().token_url` and `KeyringStore`.
- `GmailClient::new(Arc<TokenSource>)`, one per account (the quota bucket is per client). Methods: `profile`, `list_messages(q, label_ids, page_token, max_results)`, `get_message(id, Metadata|Full)`, `get_attachment`, `list_history` (404 -> `Error::HistoryExpired`), `list_labels`, `modify_thread`, `trash_thread`, `untrash_thread`. `with_base_url` / `with_retry` / `with_quota` for tests.
- `batch(&[BatchRequest])`: splits at 50, results in input order, per-part `BatchResponse { status, body }` with `into_result::<T>()`, `is_success()`, `is_rate_limited()`. `BatchRequest::get_message(id, format)` builds the call.
- Quota: token bucket 250 units/s with documented costs (`cost::*`). 429 and 403 `rateLimitExceeded`/`userRateLimitExceeded`: exponential backoff, 50-100 % jitter, `Retry-After` as a floor, 6 retries then `Error::RateLimited`. One 401 triggers a token refresh, a second 401 gives `AuthExpired`.
- MIME: `decode_base64url`, `extract_bodies` (first `text/html` and `text/plain` without filename; honours `charset` via `encoding_rs`), `list_attachments` (parts with a filename or `Content-ID`; `size`, `attachment_id`, `content_id` without `<>`).
- README: Google Cloud setup (Gmail API, consent screen "In production", Desktop client, `config.toml`) and Defender exclusion steps.
- Tests: 6 unit + 10 mock-server tests (wiremock; fake `RefreshStore`, so no Credential Manager use). The real `KeyringStore` and real Google are not exercised.

## Requests for the main thread

1. Root `Cargo.toml`: none required. New dependencies are in `crates/gmail/Cargo.toml` only (`oauth2` 5, `reqwest` 0.12 rustls, `keyring` 4 (v1 API), `toml` 0.9, `base64` 0.22, `rand` 0.9, `url`, `encoding_rs`; dev: `wiremock`). `Cargo.lock` changed.
2. For sync: map `Error::AuthExpired` to `authError`; map `Error::HistoryExpired` to the full id diff. Message `internalDate` is `Option<i64>` ms; `historyId` stays a `String`.
3. For the app/IPC owner: a sign-in command calls `sign_in_with` (or `sign_in`), then inserts the account row; sign-out calls `sign_out`. No schema change needed from this crate.

## Open questions

- `messages.list` does not pass `includeSpamTrash`, so spam and trash are not synced. Spec does not say. Add a parameter if wanted.
- `keyring` 4.2 is used through its default `v1` API. It is a new major version; check once on a real Windows machine that entries appear in Credential Manager.

## Known gaps (edge cases left out on purpose)

- Per-part 429 inside a batch is reported, not retried. The caller resubmits those calls.
- `Retry-After` HTTP-date form is ignored (only seconds).
- The quota bucket is not drained on a 429; backoff alone slows the client.
- `history.list` has no `historyTypes` filter and no `labelId` filter.
- `display_name` is `None`.
