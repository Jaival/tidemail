//! Tests against a local mock server (wiremock). No Google, no Credential Manager.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde_json::json;
use tidemail_gmail::*;
use wiremock::{
    matchers::{body_string_contains, header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

#[derive(Default)]
struct MemStore(Mutex<HashMap<String, String>>);

impl RefreshStore for MemStore {
    fn get(&self, email: &str) -> Result<Option<String>, Error> {
        Ok(self.0.lock().unwrap().get(email).cloned())
    }
    fn set(&self, email: &str, token: &str) -> Result<(), Error> {
        self.0.lock().unwrap().insert(email.into(), token.into());
        Ok(())
    }
    fn delete(&self, email: &str) -> Result<(), Error> {
        self.0.lock().unwrap().remove(email);
        Ok(())
    }
}

fn config() -> Config {
    Config {
        client_id: "cid".into(),
        client_secret: "secret".into(),
    }
}

fn fast_retry() -> RetryPolicy {
    RetryPolicy {
        max_retries: 3,
        base_delay: Duration::from_millis(5),
        max_delay: Duration::from_millis(40),
    }
}

async fn token_mock(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=refresh_token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "at-1", "expires_in": 3600, "token_type": "Bearer"}),
        ))
        .mount(server)
        .await;
}

async fn client(server: &MockServer) -> GmailClient {
    token_mock(server).await;
    let store = Arc::new(MemStore::default());
    store.set("me@example.com", "rt-1").unwrap();
    let tokens = Arc::new(TokenSource::new(
        config(),
        format!("{}/token", server.uri()),
        "me@example.com".into(),
        store,
    ));
    GmailClient::new(tokens)
        .with_base_url(server.uri())
        .with_retry(fast_retry())
}

#[tokio::test]
async fn sign_in_runs_pkce_flow_and_stores_refresh_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains("code_verifier="))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "at-1", "refresh_token": "rt-new",
            "expires_in": 3600, "token_type": "Bearer"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/gmail/v1/users/me/profile"))
        .and(header("authorization", "Bearer at-1"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"emailAddress": "me@example.com", "historyId": "1"})),
        )
        .mount(&server)
        .await;

    let endpoints = Endpoints {
        auth_url: format!("{}/auth", server.uri()),
        token_url: format!("{}/token", server.uri()),
        api_base: server.uri(),
    };
    let store = MemStore::default();
    let signed = sign_in_with(&config(), &endpoints, &store, |url| {
        let url = url::Url::parse(url).unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["scope"], SCOPE);
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["access_type"], "offline");
        assert!(q["redirect_uri"].starts_with("http://127.0.0.1:"));
        let redirect = format!("{}?code=the-code&state={}", q["redirect_uri"], q["state"]);
        tokio::spawn(async move {
            // A stray request (favicon) must not end the flow.
            let _ = reqwest::get(format!("{}/favicon.ico", q["redirect_uri"])).await;
            reqwest::get(redirect).await.unwrap();
        });
        Ok(())
    })
    .await
    .unwrap();

    assert_eq!(signed.email, "me@example.com");
    assert_eq!(
        store.get("me@example.com").unwrap().as_deref(),
        Some("rt-new")
    );
}

#[tokio::test]
async fn sign_in_rejects_state_mismatch() {
    let server = MockServer::start().await;
    let endpoints = Endpoints {
        auth_url: format!("{}/auth", server.uri()),
        token_url: format!("{}/token", server.uri()),
        api_base: server.uri(),
    };
    let err = sign_in_with(&config(), &endpoints, &MemStore::default(), |url| {
        let url = url::Url::parse(url).unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        let redirect = format!("{}?code=x&state=wrong", q["redirect_uri"]);
        tokio::spawn(async move { reqwest::get(redirect).await });
        Ok(())
    })
    .await
    .unwrap_err();
    assert!(matches!(err, Error::Auth(_)), "{err}");
}

#[tokio::test]
async fn access_token_is_cached_and_revoked_token_is_auth_expired() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"access_token": "at", "expires_in": 3600, "token_type": "Bearer"}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    let store = Arc::new(MemStore::default());
    store.set("a@x.com", "rt").unwrap();
    let ts = TokenSource::new(
        config(),
        format!("{}/token", server.uri()),
        "a@x.com".into(),
        store.clone(),
    );
    assert_eq!(ts.access_token().await.unwrap(), "at");
    assert_eq!(ts.access_token().await.unwrap(), "at"); // cached: mock expects 1 call
    drop(server);

    let revoked = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({"error": "invalid_grant", "error_description": "Token has been expired or revoked."}),
        ))
        .mount(&revoked)
        .await;
    let ts = TokenSource::new(
        config(),
        format!("{}/token", revoked.uri()),
        "a@x.com".into(),
        store.clone(),
    );
    assert!(matches!(ts.access_token().await, Err(Error::AuthExpired)));

    // No stored token at all.
    let ts = TokenSource::new(
        config(),
        format!("{}/token", revoked.uri()),
        "none@x.com".into(),
        store,
    );
    assert!(matches!(ts.access_token().await, Err(Error::AuthExpired)));
}

#[tokio::test]
async fn typed_calls_parse_fixtures() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    let api = "/gmail/v1/users/me";

    Mock::given(method("GET"))
        .and(path(format!("{api}/profile")))
        .and(header("authorization", "Bearer at-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"emailAddress": "me@example.com", "messagesTotal": 5, "historyId": "12345"}),
        ))
        .mount(&server)
        .await;
    assert_eq!(c.profile().await.unwrap().history_id, "12345");

    Mock::given(method("GET"))
        .and(path(format!("{api}/messages")))
        .and(query_param("labelIds", "INBOX"))
        .and(query_param("q", "is:unread"))
        .and(query_param("pageToken", "p2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "messages": [{"id": "m1", "threadId": "t1"}], "nextPageToken": "p3"})))
        .mount(&server)
        .await;
    let page = c
        .list_messages(Some("is:unread"), &["INBOX"], Some("p2"), None)
        .await
        .unwrap();
    assert_eq!(page.messages[0].thread_id, "t1");
    assert_eq!(page.next_page_token.as_deref(), Some("p3"));

    // Empty mailbox: Gmail omits `messages`.
    Mock::given(method("GET"))
        .and(path(format!("{api}/messages")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"resultSizeEstimate": 0})))
        .mount(&server)
        .await;
    assert!(c
        .list_messages(None, &[], None, None)
        .await
        .unwrap()
        .messages
        .is_empty());

    Mock::given(method("GET"))
        .and(path(format!("{api}/messages/m1")))
        .and(query_param("format", "metadata"))
        .and(query_param("metadataHeaders", "Subject"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "m1", "threadId": "t1", "labelIds": ["INBOX", "UNREAD"], "snippet": "hi",
            "historyId": "99", "internalDate": "1700000000000", "sizeEstimate": 321,
            "payload": {"mimeType": "text/plain", "headers": [
                {"name": "Subject", "value": "Hello"}, {"name": "From", "value": "A <a@x.com>"}]}
        })))
        .mount(&server)
        .await;
    let m = c.get_message("m1", MessageFormat::Metadata).await.unwrap();
    assert_eq!(m.internal_date, Some(1_700_000_000_000));
    assert_eq!(m.header("subject"), Some("Hello"));

    Mock::given(method("GET"))
        .and(path(format!("{api}/messages/m1/attachments/a1")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"size": 2, "data": "aGk"})))
        .mount(&server)
        .await;
    let a = c.get_attachment("m1", "a1").await.unwrap();
    assert_eq!(decode_base64url(&a.data).unwrap(), b"hi");

    Mock::given(method("GET")).and(path(format!("{api}/history")))
        .and(query_param("startHistoryId", "100"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "history": [{"id": "101",
                "messagesAdded": [{"message": {"id": "m2", "threadId": "t2", "labelIds": ["INBOX"]}}],
                "labelsRemoved": [{"message": {"id": "m1", "threadId": "t1"}, "labelIds": ["UNREAD"]}]}],
            "historyId": "105"})))
        .mount(&server).await;
    let h = c.list_history("100", None).await.unwrap();
    assert_eq!(h.history[0].messages_added[0].message.id, "m2");
    assert_eq!(h.history[0].labels_removed[0].label_ids, ["UNREAD"]);
    assert_eq!(h.history_id.as_deref(), Some("105"));

    Mock::given(method("GET"))
        .and(path(format!("{api}/labels")))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"labels": [
            {"id": "INBOX", "name": "INBOX", "type": "system"},
            {"id": "Label_1", "name": "Work", "type": "user"}]})))
        .mount(&server)
        .await;
    assert_eq!(c.list_labels().await.unwrap().labels[1].kind, "user");

    Mock::given(method("POST"))
        .and(path(format!("{api}/threads/t1/modify")))
        .and(wiremock::matchers::body_json(
            json!({"addLabelIds": ["STARRED"], "removeLabelIds": ["INBOX"]}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": "t1", "historyId": "7"})),
        )
        .mount(&server)
        .await;
    assert_eq!(
        c.modify_thread("t1", &["STARRED"], &["INBOX"])
            .await
            .unwrap()
            .id,
        "t1"
    );

    for action in ["trash", "untrash"] {
        Mock::given(method("POST"))
            .and(path(format!("{api}/threads/t1/{action}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "t1"})))
            .mount(&server)
            .await;
    }
    c.trash_thread("t1").await.unwrap();
    c.untrash_thread("t1").await.unwrap();
}

#[tokio::test]
async fn history_404_is_history_expired() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("GET")).and(path("/gmail/v1/users/me/history"))
        .respond_with(ResponseTemplate::new(404).set_body_json(
            json!({"error": {"code": 404, "message": "Requested entity was not found.", "status": "NOT_FOUND"}})))
        .mount(&server).await;
    assert!(matches!(
        c.list_history("1", None).await,
        Err(Error::HistoryExpired)
    ));
}

#[tokio::test]
async fn rate_limits_back_off_and_honour_retry_after() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    let p = "/gmail/v1/users/me/profile";
    // 429 with Retry-After: 1, then 403 userRateLimitExceeded, then success.
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": {"code": 403,
            "message": "Rate Limit Exceeded", "errors": [{"reason": "userRateLimitExceeded"}]}})),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"emailAddress": "a", "historyId": "1"})),
        )
        .mount(&server)
        .await;
    let start = Instant::now();
    c.profile().await.unwrap();
    assert!(
        start.elapsed() >= Duration::from_secs(1),
        "Retry-After ignored"
    );

    // Permanent limit: gives up.
    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(429))
        .expect(4)
        .mount(&server)
        .await; // 1 try + 3 retries
    assert!(matches!(c.profile().await, Err(Error::RateLimited)));
}

#[tokio::test]
async fn other_errors_are_not_retried_and_401_refreshes_once() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    let p = "/gmail/v1/users/me/labels";
    Mock::given(method("GET")).and(path(p))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error": {"code": 403,
            "message": "Insufficient Permission", "errors": [{"reason": "insufficientPermissions"}]}})))
        .expect(1).mount(&server).await;
    match c.list_labels().await {
        Err(Error::Api {
            status: 403,
            reason,
            ..
        }) => assert_eq!(reason, "insufficientPermissions"),
        other => panic!("{other:?}"),
    }

    let server = MockServer::start().await;
    let c = client(&server).await;
    Mock::given(method("GET"))
        .and(path(p))
        .respond_with(ResponseTemplate::new(401))
        .expect(2)
        .mount(&server)
        .await;
    assert!(matches!(c.list_labels().await, Err(Error::AuthExpired)));
}

fn batch_part(i: usize, status: &str, body: &str) -> String {
    format!(
        "--batch_resp\r\nContent-Type: application/http\r\nContent-ID: <response-item{i}>\r\n\r\nHTTP/1.1 {status}\r\nContent-Type: application/json; charset=UTF-8\r\nContent-Length: {}\r\n\r\n{body}\r\n",
        body.len()
    )
}

#[tokio::test]
async fn batch_splits_at_50_and_returns_per_part_results() {
    let server = MockServer::start().await;
    let c = client(&server).await;

    // First batch: 50 calls, replies out of order, one part fails.
    let mut first = String::new();
    for i in (0..50).rev() {
        if i == 7 {
            first += &batch_part(
                i,
                "404 Not Found",
                r#"{"error":{"code":404,"message":"Requested entity was not found."}}"#,
            );
        } else {
            first += &batch_part(
                i,
                "200 OK",
                &format!(r#"{{"id":"m{i}","threadId":"t{i}","internalDate":"1000"}}"#),
            );
        }
    }
    first += "--batch_resp--\r\n";
    let second = batch_part(0, "200 OK", r#"{"id":"last","threadId":"tl"}"#) + "--batch_resp--\r\n";

    Mock::given(method("POST"))
        .and(path("/batch/gmail/v1"))
        .and(header("authorization", "Bearer at-1"))
        .and(body_string_contains(
            "GET /gmail/v1/users/me/messages/m49?format=metadata&metadataHeaders=From",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(first, "multipart/mixed; boundary=batch_resp"),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/batch/gmail/v1"))
        .and(body_string_contains("messages/last?format=full"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(second, "multipart/mixed; boundary=batch_resp"),
        )
        .expect(1)
        .mount(&server)
        .await;

    let mut reqs: Vec<_> = (0..50)
        .map(|i| BatchRequest::get_message(&format!("m{i}"), MessageFormat::Metadata))
        .collect();
    reqs.push(BatchRequest::get_message("last", MessageFormat::Full));
    let out = c.batch(&reqs).await.unwrap();
    assert_eq!(out.len(), 51);
    assert_eq!(out[3].clone().into_result::<Message>().unwrap().id, "m3");
    assert_eq!(out[7].status, 404);
    assert!(matches!(
        out[7].clone().into_result::<Message>(),
        Err(Error::Api { status: 404, .. })
    ));
    assert_eq!(out[50].clone().into_result::<Message>().unwrap().id, "last");
}

#[tokio::test]
async fn batch_part_rate_limit_is_reported_per_part() {
    let server = MockServer::start().await;
    let c = client(&server).await;
    let body = batch_part(
        0,
        "429 Too Many Requests",
        r#"{"error":{"code":429,"errors":[{"reason":"rateLimitExceeded"}]}}"#,
    ) + &batch_part(1, "204 No Content", "")
        + "--batch_resp--\r\n";
    Mock::given(method("POST"))
        .and(path("/batch/gmail/v1"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body, "multipart/mixed; boundary=batch_resp"),
        )
        .mount(&server)
        .await;
    let out = c
        .batch(&[
            BatchRequest::get_message("a", MessageFormat::Full),
            BatchRequest::get_message("b", MessageFormat::Full),
        ])
        .await
        .unwrap();
    assert!(out[0].is_rate_limited());
    assert!(out[1].is_success() && out[1].body.is_null());
}

#[tokio::test]
async fn quota_throttles_client_calls() {
    let server = MockServer::start().await;
    let c = client(&server).await.with_quota(Quota::new(100.0));
    Mock::given(method("POST"))
        .and(path_regex_modify())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "t"})))
        .mount(&server)
        .await;
    let start = Instant::now();
    // 15 modifies x 10 units = 150 units at 100/s with 100 banked: >= 0.5 s.
    for _ in 0..15 {
        c.modify_thread("t", &[], &["INBOX"]).await.unwrap();
    }
    assert!(start.elapsed() >= Duration::from_millis(450));
}

fn path_regex_modify() -> wiremock::matchers::PathExactMatcher {
    path("/gmail/v1/users/me/threads/t/modify")
}
