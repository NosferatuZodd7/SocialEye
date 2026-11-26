//! Shared HTTP client: retries with exponential backoff, rate-limit handling and
//! error classification across Google, Meta and Spotify error formats.
use crate::error::{AppError, AppResult};
use reqwest::RequestBuilder;
use serde_json::Value;
use std::time::Duration;

const MAX_ATTEMPTS: u32 = 4;

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("SocialEye/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()
        .expect("reqwest client")
}

/// Send `rb`, retrying retryable failures. Returns parsed JSON (Null for empty bodies).
pub async fn send(rb: RequestBuilder) -> AppResult<Value> {
    let mut attempt = 0;
    loop {
        let req = rb.try_clone().ok_or_else(|| AppError::validation("request body cannot be retried"))?;
        let result: AppResult<()> = match req.send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let retry_after = resp
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                let text = resp.text().await.unwrap_or_default();
                if (200..300).contains(&status) {
                    return Ok(if text.trim().is_empty() { Value::Null } else { serde_json::from_str(&text)? });
                }
                Err(classify(status, retry_after, &text))
            }
            Err(e) => Err(AppError::from(e)),
        };
        let err = result.unwrap_err();
        attempt += 1;
        if !err.is_retryable() || attempt >= MAX_ATTEMPTS {
            return Err(err);
        }
        tokio::time::sleep(backoff(attempt, &err)).await;
    }
}

pub fn backoff(attempt: u32, err: &AppError) -> Duration {
    if let AppError::RateLimited { retry_after_secs } = err {
        return Duration::from_secs((*retry_after_secs).clamp(1, 60));
    }
    let jitter = rand::random::<u64>() % 250;
    Duration::from_millis(500 * 2u64.pow(attempt.min(6)) + jitter)
}

/// Map an HTTP failure to an AppError. Pure, so it is unit-tested with real error bodies.
pub fn classify(status: u16, retry_after: Option<u64>, body: &str) -> AppError {
    let json: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let err = &json["error"];
    // Google: {error:{code,message,errors:[{reason}]}}; Meta: {error:{message,code,error_subcode}};
    // Spotify: {error:{status,message}} or OAuth {error:"invalid_grant", error_description}.
    let message = err["message"]
        .as_str()
        .or_else(|| json["error_description"].as_str())
        .or_else(|| err.as_str())
        .unwrap_or("unexpected response")
        .chars()
        .take(500)
        .collect::<String>();
    let reason = err["errors"][0]["reason"].as_str().unwrap_or("");
    let meta_code = err["code"].as_i64().unwrap_or(0);

    // Meta signals throttling with codes on HTTP 400/403.
    if matches!(meta_code, 4 | 17 | 32 | 613 | 80001..=80014) || status == 429 {
        return AppError::RateLimited { retry_after_secs: retry_after.unwrap_or(30) };
    }
    if reason == "quotaExceeded" || reason == "dailyLimitExceeded" {
        return AppError::Api { status, message: format!("API quota exhausted: {message}"), retryable: false };
    }
    if status == 401 || meta_code == 190 || err.as_str() == Some("invalid_grant") {
        return AppError::Auth { message, reconnect_required: true };
    }
    if status == 403 && (meta_code == 10 || meta_code == 200 || reason == "insufficientPermissions" || reason == "forbidden") {
        return AppError::Auth { message: format!("missing permission: {message}"), reconnect_required: true };
    }
    AppError::Api { status, message, retryable: status >= 500 || status == 408 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_google_quota() {
        let b = r#"{"error":{"code":403,"message":"quota","errors":[{"reason":"quotaExceeded"}]}}"#;
        assert!(matches!(classify(403, None, b), AppError::Api { retryable: false, .. }));
    }
    #[test]
    fn classifies_meta_expired_token() {
        let b = r#"{"error":{"message":"Error validating access token","type":"OAuthException","code":190}}"#;
        assert!(matches!(classify(400, None, b), AppError::Auth { reconnect_required: true, .. }));
    }
    #[test]
    fn classifies_meta_throttle_and_429() {
        let b = r#"{"error":{"message":"Application request limit reached","code":4}}"#;
        assert!(matches!(classify(400, None, b), AppError::RateLimited { retry_after_secs: 30 }));
        assert!(matches!(classify(429, Some(7), "{}"), AppError::RateLimited { retry_after_secs: 7 }));
    }
    #[test]
    fn classifies_server_errors_as_retryable() {
        assert!(classify(503, None, "").is_retryable());
        assert!(!classify(404, None, "").is_retryable());
    }
    #[test]
    fn classifies_oauth_invalid_grant() {
        let b = r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#;
        match classify(400, None, b) {
            AppError::Auth { message, .. } => assert!(message.contains("expired")),
            e => panic!("{e:?}"),
        }
    }
    #[test]
    fn backoff_honours_retry_after() {
        assert_eq!(backoff(1, &AppError::RateLimited { retry_after_secs: 5 }), Duration::from_secs(5));
        assert!(backoff(2, &AppError::Network { message: String::new() }) >= Duration::from_millis(2000));
    }
}
