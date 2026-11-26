//! OAuth 2.0 authorization-code flow with PKCE (RFC 7636) and a loopback redirect
//! (RFC 8252) — the flow Google, Spotify and Meta document for native apps.
//! SocialEye never sees or stores social network passwords.
use crate::error::{AppError, AppResult};
use crate::secrets::TokenSet;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub struct OAuthProvider {
    pub auth_url: &'static str,
    pub token_url: &'static str,
    /// Extra query parameters for the authorization request.
    pub extra_auth_params: &'static [(&'static str, &'static str)],
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

pub fn random_token(bytes: usize) -> String {
    let mut b = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}

pub fn pkce() -> Pkce {
    let verifier = random_token(48);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    Pkce { verifier, challenge, state: random_token(16) }
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

pub fn authorize_url(p: &OAuthProvider, client_id: &str, port: u16, scopes: &[&str], pk: &Pkce) -> String {
    let mut u = url::Url::parse(p.auth_url).expect("static auth url");
    u.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", &redirect_uri(port))
        .append_pair("response_type", "code")
        .append_pair("scope", &scopes.join(if p.auth_url.contains("facebook") { "," } else { " " }))
        .append_pair("state", &pk.state)
        .append_pair("code_challenge", &pk.challenge)
        .append_pair("code_challenge_method", "S256");
    for (k, v) in p.extra_auth_params {
        u.query_pairs_mut().append_pair(k, v);
    }
    u.to_string()
}

/// Parse the request line of the browser's redirect and return the authorization code.
pub fn parse_callback(request_line: &str, expected_state: &str) -> AppResult<String> {
    let path = request_line.split_whitespace().nth(1).unwrap_or("");
    let u = url::Url::parse(&format!("http://127.0.0.1{path}")).map_err(|_| AppError::validation("bad callback"))?;
    let q: std::collections::HashMap<_, _> = u.query_pairs().into_owned().collect();
    if let Some(e) = q.get("error") {
        let d = q.get("error_description").cloned().unwrap_or_default();
        return Err(AppError::Auth { message: format!("authorization denied: {e} {d}").trim().into(), reconnect_required: false });
    }
    if q.get("state").map(String::as_str) != Some(expected_state) {
        return Err(AppError::Auth { message: "state mismatch — possible CSRF, aborted".into(), reconnect_required: false });
    }
    q.get("code").cloned().ok_or_else(|| AppError::Auth { message: "no authorization code returned".into(), reconnect_required: false })
}

/// Bind the loopback listener *before* opening the browser so the redirect cannot race us.
pub async fn bind(port: u16) -> AppResult<TcpListener> {
    TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| AppError::Io {
        message: format!("cannot listen on 127.0.0.1:{port} ({e}). Change the redirect port in Settings → Platform apps."),
    })
}

/// Wait (max 5 minutes) for the browser redirect and return the authorization code.
pub async fn wait_for_code(listener: TcpListener, state: &str) -> AppResult<String> {
    let fut = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await?;
            let req = String::from_utf8_lossy(&buf[..n]);
            let line = req.lines().next().unwrap_or("");
            if !line.contains("/callback") {
                let _ = sock.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n").await;
                continue; // e.g. favicon requests
            }
            let result = parse_callback(line, state);
            let msg = match &result {
                Ok(_) => "Authorization complete. You can close this tab and return to SocialEye.",
                Err(_) => "Authorization failed. Return to SocialEye for details.",
            };
            let body = format!("<!doctype html><meta charset=utf-8><title>SocialEye</title><body style=\"font-family:sans-serif;padding:3rem\"><h2>{msg}</h2>");
            let _ = sock
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes())
                .await;
            return result;
        }
    };
    tokio::time::timeout(Duration::from_secs(300), fut)
        .await
        .map_err(|_| AppError::Auth { message: "timed out waiting for browser authorization".into(), reconnect_required: false })?
}

pub fn parse_token_response(v: &Value, now: i64) -> AppResult<TokenSet> {
    let access = v["access_token"].as_str().ok_or_else(|| AppError::Auth {
        message: "token response did not include an access token".into(),
        reconnect_required: true,
    })?;
    Ok(TokenSet {
        access_token: access.into(),
        refresh_token: v["refresh_token"].as_str().map(Into::into),
        expires_at: v["expires_in"].as_i64().map(|s| now + s),
    })
}

pub async fn exchange_code(
    http: &reqwest::Client,
    p: &OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
    port: u16,
    code: &str,
    verifier: &str,
) -> AppResult<TokenSet> {
    let ru = redirect_uri(port);
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", ru.as_str()),
        ("client_id", client_id),
        ("code_verifier", verifier),
    ];
    if let Some(s) = client_secret {
        form.push(("client_secret", s));
    }
    let v = crate::http::send(http.post(p.token_url).form(&form)).await?;
    parse_token_response(&v, chrono::Utc::now().timestamp())
}

pub async fn refresh(
    http: &reqwest::Client,
    p: &OAuthProvider,
    client_id: &str,
    client_secret: Option<&str>,
    refresh_token: &str,
) -> AppResult<TokenSet> {
    let mut form = vec![("grant_type", "refresh_token"), ("refresh_token", refresh_token), ("client_id", client_id)];
    if let Some(s) = client_secret {
        form.push(("client_secret", s));
    }
    let v = crate::http::send(http.post(p.token_url).form(&form)).await?;
    let mut t = parse_token_response(&v, chrono::Utc::now().timestamp())?;
    // Google and Spotify may omit the refresh token on refresh: keep the old one.
    t.refresh_token = t.refresh_token.or_else(|| Some(refresh_token.into()));
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let p = pkce();
        assert_eq!(p.challenge, URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes())));
        assert!(p.verifier.len() >= 43 && p.verifier.len() <= 128);
    }

    #[test]
    fn callback_validates_state() {
        assert_eq!(parse_callback("GET /callback?code=abc&state=s1 HTTP/1.1", "s1").unwrap(), "abc");
        assert!(parse_callback("GET /callback?code=abc&state=evil HTTP/1.1", "s1").is_err());
        assert!(parse_callback("GET /callback?error=access_denied&state=s1 HTTP/1.1", "s1").is_err());
    }

    #[test]
    fn authorize_url_contains_pkce_and_redirect() {
        let p = OAuthProvider { auth_url: "https://accounts.google.com/o/oauth2/v2/auth", token_url: "", extra_auth_params: &[("access_type", "offline")] };
        let pk = pkce();
        let u = authorize_url(&p, "cid", 5000, &["a", "b"], &pk);
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2Fcallback"));
        assert!(u.contains("scope=a+b"));
        assert!(u.contains("access_type=offline"));
    }

    #[test]
    fn token_response_parsing() {
        let v = serde_json::json!({"access_token":"x","expires_in":3600,"refresh_token":"r"});
        let t = parse_token_response(&v, 100).unwrap();
        assert_eq!(t.expires_at, Some(3700));
        assert!(parse_token_response(&serde_json::json!({}), 0).is_err());
    }

    #[tokio::test]
    async fn loopback_receives_code() {
        let listener = bind(0).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let h = tokio::spawn(async move { wait_for_code(listener, "st").await });
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(b"GET /callback?code=c0de&state=st HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        assert!(out.contains("Authorization complete"));
        assert_eq!(h.await.unwrap().unwrap(), "c0de");
    }
}
