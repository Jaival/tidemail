//! Gmail REST client: quota, token refresh and rate-limit backoff in one send loop.

use std::{sync::Arc, time::Duration};

use serde::de::DeserializeOwned;

use crate::{
    cost, AttachmentBody, Error, HistoryList, LabelList, Message, MessageList, Profile, Quota,
    Thread, TokenSource,
};

pub(crate) const GOOGLE_API_BASE: &str = "https://gmail.googleapis.com";
pub(crate) const METADATA_HEADERS: [&str; 5] = ["From", "To", "Cc", "Subject", "Date"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageFormat {
    /// Labels, snippet and the headers From/To/Cc/Subject/Date.
    Metadata,
    /// Full MIME tree with bodies.
    Full,
}

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Rate-limit retries before `Error::RateLimited`.
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 6,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(32),
        }
    }
}

impl RetryPolicy {
    /// Exponential backoff with jitter (50-100 % of the step); `Retry-After` is a floor.
    fn delay(&self, retry: u32, retry_after: Option<Duration>) -> Duration {
        let step = self
            .base_delay
            .saturating_mul(1 << retry.saturating_sub(1).min(16))
            .min(self.max_delay);
        let jittered = step.mul_f64(rand::random_range(0.5..1.0));
        jittered.max(retry_after.unwrap_or_default())
    }
}

pub(crate) fn is_rate_limit(status: u16, reason: &str) -> bool {
    status == 429
        || (status == 403 && matches!(reason, "rateLimitExceeded" | "userRateLimitExceeded"))
}

/// `(reason, message)` from a Google error body.
fn parse_error_body(body: &str) -> (String, String) {
    let v: serde_json::Value = serde_json::from_str(body).unwrap_or_default();
    let e = &v["error"];
    let reason = e["errors"][0]["reason"]
        .as_str()
        .or_else(|| e["status"].as_str())
        .unwrap_or("");
    let message = e["message"]
        .as_str()
        .map_or_else(|| body.to_string(), String::from);
    (reason.to_string(), message)
}

pub(crate) fn api_error(status: u16, body: &str) -> Error {
    let (reason, message) = parse_error_body(body);
    Error::Api {
        status,
        reason,
        message,
    }
}

/// One account's Gmail API access. Create one per account: the quota bucket is per client.
pub struct GmailClient {
    pub(crate) http: reqwest::Client,
    pub(crate) base: String,
    tokens: Arc<TokenSource>,
    quota: Quota,
    retry: RetryPolicy,
}

impl GmailClient {
    pub fn new(tokens: Arc<TokenSource>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: GOOGLE_API_BASE.to_string(),
            tokens,
            quota: Quota::default(),
            retry: RetryPolicy::default(),
        }
    }

    /// Host root, no trailing slash (for tests).
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    pub fn with_retry(mut self, retry: RetryPolicy) -> Self {
        self.retry = retry;
        self
    }

    pub fn with_quota(mut self, quota: Quota) -> Self {
        self.quota = quota;
        self
    }

    /// Send with quota accounting. Retries rate limits with backoff and one 401 with a fresh
    /// token. Any other non-2xx status becomes `Error::Api`.
    pub(crate) async fn send(
        &self,
        units: u32,
        build: impl Fn(&reqwest::Client) -> reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, Error> {
        let mut retries = 0;
        let mut refreshed = false;
        loop {
            self.quota.acquire(units).await;
            let token = self.tokens.access_token().await?;
            let resp = build(&self.http).bearer_auth(token).send().await?;
            let status = resp.status().as_u16();
            if resp.status().is_success() {
                return Ok(resp);
            }
            if status == 401 {
                if refreshed {
                    return Err(Error::AuthExpired);
                }
                refreshed = true;
                self.tokens.invalidate().await;
                continue;
            }
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let body = resp.text().await?;
            let err = api_error(status, &body);
            if let Error::Api { reason, .. } = &err {
                if is_rate_limit(status, reason) {
                    retries += 1;
                    if retries > self.retry.max_retries {
                        return Err(Error::RateLimited);
                    }
                    tokio::time::sleep(self.retry.delay(retries, retry_after)).await;
                    continue;
                }
            }
            return Err(err);
        }
    }

    async fn get<T: DeserializeOwned>(
        &self,
        units: u32,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, Error> {
        let url = format!("{}/gmail/v1/users/me/{path}", self.base);
        let resp = self.send(units, |h| h.get(&url).query(query)).await?;
        resp.json().await.map_err(|e| Error::Parse(e.to_string()))
    }

    async fn post<T: DeserializeOwned>(
        &self,
        units: u32,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, Error> {
        let url = format!("{}/gmail/v1/users/me/{path}", self.base);
        let resp = self.send(units, |h| h.post(&url).json(body)).await?;
        resp.json().await.map_err(|e| Error::Parse(e.to_string()))
    }

    pub async fn profile(&self) -> Result<Profile, Error> {
        self.get(cost::GET_PROFILE, "profile", &[]).await
    }

    pub async fn list_messages(
        &self,
        q: Option<&str>,
        label_ids: &[&str],
        page_token: Option<&str>,
        max_results: Option<u32>,
    ) -> Result<MessageList, Error> {
        // Spam and trash are synced like any other label.
        let mut query = vec![("includeSpamTrash", "true".to_string())];
        query.extend(q.map(|q| ("q", q.to_string())));
        query.extend(label_ids.iter().map(|l| ("labelIds", (*l).to_string())));
        query.extend(page_token.map(|t| ("pageToken", t.to_string())));
        query.extend(max_results.map(|n| ("maxResults", n.to_string())));
        self.get(cost::MESSAGES_LIST, "messages", &query).await
    }

    pub async fn get_message(&self, id: &str, format: MessageFormat) -> Result<Message, Error> {
        let (_, query) = message_get_query(format);
        self.get(cost::MESSAGES_GET, &format!("messages/{id}"), &query)
            .await
    }

    pub async fn get_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
    ) -> Result<AttachmentBody, Error> {
        self.get(
            cost::ATTACHMENTS_GET,
            &format!("messages/{message_id}/attachments/{attachment_id}"),
            &[],
        )
        .await
    }

    /// `Error::HistoryExpired` when Google answers 404 (start id too old).
    pub async fn list_history(
        &self,
        start_history_id: &str,
        page_token: Option<&str>,
    ) -> Result<HistoryList, Error> {
        let mut query = vec![("startHistoryId", start_history_id.to_string())];
        query.extend(page_token.map(|t| ("pageToken", t.to_string())));
        match self.get(cost::HISTORY_LIST, "history", &query).await {
            Err(Error::Api { status: 404, .. }) => Err(Error::HistoryExpired),
            other => other,
        }
    }

    pub async fn list_labels(&self) -> Result<LabelList, Error> {
        self.get(cost::LABELS_LIST, "labels", &[]).await
    }

    pub async fn modify_thread(
        &self,
        thread_id: &str,
        add_label_ids: &[&str],
        remove_label_ids: &[&str],
    ) -> Result<Thread, Error> {
        let body = serde_json::json!({
            "addLabelIds": add_label_ids,
            "removeLabelIds": remove_label_ids,
        });
        self.post(
            cost::THREADS_MODIFY,
            &format!("threads/{thread_id}/modify"),
            &body,
        )
        .await
    }

    pub async fn trash_thread(&self, thread_id: &str) -> Result<Thread, Error> {
        self.post(
            cost::THREADS_TRASH,
            &format!("threads/{thread_id}/trash"),
            &serde_json::Value::Null,
        )
        .await
    }

    pub async fn untrash_thread(&self, thread_id: &str) -> Result<Thread, Error> {
        self.post(
            cost::THREADS_UNTRASH,
            &format!("threads/{thread_id}/untrash"),
            &serde_json::Value::Null,
        )
        .await
    }
}

/// `(format name, query pairs)` for `messages.get`; shared with the batch builder.
pub(crate) fn message_get_query(
    format: MessageFormat,
) -> (&'static str, Vec<(&'static str, String)>) {
    match format {
        MessageFormat::Full => ("full", vec![("format", "full".to_string())]),
        MessageFormat::Metadata => {
            let mut q = vec![("format", "metadata".to_string())];
            q.extend(
                METADATA_HEADERS
                    .iter()
                    .map(|h| ("metadataHeaders", (*h).to_string())),
            );
            ("metadata", q)
        }
    }
}
