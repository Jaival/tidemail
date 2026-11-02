//! `POST /batch/gmail/v1`: multipart/mixed, up to 50 calls per request.

use serde::de::DeserializeOwned;

use crate::{
    client::{api_error, is_rate_limit, message_get_query},
    cost, Error, GmailClient, MessageFormat,
};

pub const MAX_BATCH: usize = 50;

#[derive(Debug, Clone)]
pub struct BatchRequest {
    pub method: &'static str,
    /// Path with query, starting at `/gmail/v1/...`.
    pub path: String,
    pub body: Option<serde_json::Value>,
    /// Quota units of this call.
    pub cost: u32,
}

impl BatchRequest {
    pub fn get_message(id: &str, format: MessageFormat) -> Self {
        let (_, query) = message_get_query(format);
        let query = query
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("&");
        Self {
            method: "GET",
            path: format!("/gmail/v1/users/me/messages/{id}?{query}"),
            body: None,
            cost: cost::MESSAGES_GET,
        }
    }
}

/// One call's result inside a batch. Failures are per part, not per batch.
#[derive(Debug, Clone)]
pub struct BatchResponse {
    pub status: u16,
    pub body: serde_json::Value,
}

impl BatchResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Per-part 429 / `rateLimitExceeded`; the caller may resubmit just this call.
    pub fn is_rate_limited(&self) -> bool {
        let reason = self.body["error"]["errors"][0]["reason"]
            .as_str()
            .unwrap_or("");
        !self.is_success() && is_rate_limit(self.status, reason)
    }

    pub fn into_result<T: DeserializeOwned>(self) -> Result<T, Error> {
        if !self.is_success() {
            return Err(api_error(self.status, &self.body.to_string()));
        }
        serde_json::from_value(self.body).map_err(|e| Error::Parse(e.to_string()))
    }
}

impl GmailClient {
    /// Run `requests` in batches of at most 50. Results keep the input order.
    pub async fn batch(&self, requests: &[BatchRequest]) -> Result<Vec<BatchResponse>, Error> {
        let mut out = Vec::with_capacity(requests.len());
        for chunk in requests.chunks(MAX_BATCH) {
            out.extend(self.batch_once(chunk).await?);
        }
        Ok(out)
    }

    async fn batch_once(&self, chunk: &[BatchRequest]) -> Result<Vec<BatchResponse>, Error> {
        let boundary = format!("tidemail_{:016x}", rand::random::<u64>());
        let mut body = String::new();
        for (i, r) in chunk.iter().enumerate() {
            body.push_str(&format!(
                "--{boundary}\r\nContent-Type: application/http\r\nContent-ID: <item{i}>\r\n\r\n{} {}\r\n",
                r.method, r.path
            ));
            match &r.body {
                Some(json) => {
                    body.push_str(&format!("Content-Type: application/json\r\n\r\n{json}\r\n"))
                }
                None => body.push_str("\r\n"),
            }
        }
        body.push_str(&format!("--{boundary}--\r\n"));

        let url = format!("{}/batch/gmail/v1", self.base);
        let units = chunk.iter().map(|r| r.cost).sum();
        let content_type = format!("multipart/mixed; boundary={boundary}");
        let resp = self
            .send(units, |h| {
                h.post(&url)
                    .header(reqwest::header::CONTENT_TYPE, &content_type)
                    .body(body.clone())
            })
            .await?;
        let resp_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        let text = resp.text().await?;
        parse_batch(&resp_type, &text, chunk.len())
    }
}

fn parse_batch(
    content_type: &str,
    text: &str,
    expected: usize,
) -> Result<Vec<BatchResponse>, Error> {
    let bad = |m: &str| Error::Parse(format!("batch response: {m}"));
    let boundary = content_type
        .split(';')
        .find_map(|p| p.trim().strip_prefix("boundary="))
        .ok_or_else(|| bad("no boundary"))?
        .trim_matches('"');
    let delimiter = format!("--{boundary}");
    let mut slots: Vec<Option<BatchResponse>> = vec![None; expected];
    for part in text.split(delimiter.as_str()).skip(1) {
        if part.starts_with("--") {
            break; // closing delimiter
        }
        let part = part.replace("\r\n", "\n");
        // MIME part headers, blank line, embedded HTTP response.
        let (mime_headers, http) = part
            .trim_start_matches('\n')
            .split_once("\n\n")
            .ok_or_else(|| bad("part"))?;
        let index = mime_headers
            .lines()
            .find(|l| l.to_ascii_lowercase().starts_with("content-id:"))
            .and_then(|l| l.rsplit("item").next())
            .and_then(|n| n.trim().trim_end_matches('>').parse::<usize>().ok())
            .filter(|i| *i < expected)
            .ok_or_else(|| bad("Content-ID"))?;
        let (head, json) = http.split_once("\n\n").unwrap_or((http, ""));
        let status = head
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| bad("status line"))?;
        let json = json.trim();
        let body = if json.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str(json).map_err(|e| bad(&e.to_string()))?
        };
        slots[index] = Some(BatchResponse { status, body });
    }
    slots
        .into_iter()
        .map(|s| s.ok_or_else(|| bad("missing part")))
        .collect()
}
