//! Spotify Web API — podcast catalogue data. Private Spotify for Creators analytics
//! (plays, listeners, followers) are NOT part of the public Web API; SocialEye imports
//! them from the CSV exports Spotify for Creators provides (see `csv_import`).
use super::*;
use crate::error::{AppError, AppResult};
use crate::oauth::OAuthProvider;
use chrono::Utc;
use serde_json::{json, Value};

pub static OAUTH: OAuthProvider =
    OAuthProvider { auth_url: "https://accounts.spotify.com/authorize", token_url: "https://accounts.spotify.com/api/token", extra_auth_params: &[] };

pub const SCOPES: &[&str] = &["user-read-private", "user-library-read"];
const API: &str = "https://api.spotify.com/v1";
pub const SOURCE: &str = "Spotify Web API";
const MAX_EPISODES: usize = 500;

/// Accepts a show URL, `spotify:show:` URI or bare id.
pub fn parse_show_id(input: &str) -> AppResult<String> {
    let s = input.trim();
    let id = if let Some(rest) = s.strip_prefix("spotify:show:") {
        rest
    } else if let Some(i) = s.find("/show/") {
        s[i + 6..].split(['?', '/', '#']).next().unwrap_or("")
    } else {
        s
    };
    if id.len() == 22 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        Ok(id.into())
    } else {
        Err(AppError::validation("not a Spotify show link or id (expected open.spotify.com/show/…)"))
    }
}

fn image(v: &Value) -> Option<String> {
    v["images"].get(0).and_then(|i| i["url"].as_str()).map(Into::into)
}

pub async fn discover(ctx: &Ctx<'_>) -> AppResult<Vec<DiscoveredAccount>> {
    let me = ctx.get(&format!("{API}/me"), &[]).await?;
    Ok(vec![DiscoveredAccount {
        account_type: "user".into(),
        external_id: me["id"].as_str().ok_or_else(|| AppError::Api { status: 500, message: "no Spotify user id".into(), retryable: false })?.into(),
        name: me["display_name"].as_str().or(me["id"].as_str()).unwrap_or("Spotify").into(),
        handle: me["id"].as_str().map(Into::into),
        avatar_url: image(&me),
        token_override: None,
    }])
}

/// Saved shows on the user's library, offered as podcasts to track.
pub async fn saved_shows(ctx: &Ctx<'_>) -> AppResult<Vec<DiscoveredAccount>> {
    let v = ctx.get(&format!("{API}/me/shows"), &[("limit", "50")]).await?;
    Ok(v["items"].as_array().into_iter().flatten().filter_map(|i| parse_show(&i["show"])).collect())
}

pub fn parse_show(s: &Value) -> Option<DiscoveredAccount> {
    Some(DiscoveredAccount {
        account_type: "podcast".into(),
        external_id: s["id"].as_str()?.into(),
        name: s["name"].as_str().unwrap_or("Podcast").into(),
        handle: s["publisher"].as_str().map(Into::into),
        avatar_url: image(s),
        token_override: None,
    })
}

pub async fn show(ctx: &Ctx<'_>, id: &str) -> AppResult<Value> {
    ctx.get(&format!("{API}/shows/{id}"), &[]).await
}

pub fn parse_episode(e: &Value) -> ContentRow {
    let mut stats = serde_json::Map::new();
    if let Some(p) = e["release_date_precision"].as_str() {
        stats.insert("release_date_precision".into(), json!(p));
    }
    if let Some(x) = e["explicit"].as_bool() {
        stats.insert("explicit".into(), json!(x));
    }
    ContentRow {
        external_id: e["id"].as_str().unwrap_or_default().into(),
        kind: "episode".into(),
        title: e["name"].as_str().map(Into::into),
        description: e["description"].as_str().map(Into::into),
        url: e["external_urls"]["spotify"].as_str().map(Into::into),
        thumbnail_url: image(e),
        published_at: e["release_date"].as_str().map(Into::into),
        duration_secs: e["duration_ms"].as_i64().map(|ms| ms / 1000),
        stats,
    }
}

pub async fn sync(ctx: &Ctx<'_>, account_type: &str, id: &str) -> AppResult<SyncData> {
    let mut d = SyncData { source: SOURCE, ..Default::default() };
    if account_type == "user" {
        d.profile = discover(ctx).await?.into_iter().next();
        return Ok(d);
    }
    let today = Utc::now().date_naive().to_string();
    let s = show(ctx, id).await?;
    let mut prof = parse_show(&s);
    if let Some(p) = &mut prof {
        p.handle = s["publisher"].as_str().map(Into::into);
    }
    d.profile = prof;
    if let Some(n) = s["total_episodes"].as_f64() {
        d.metric(None, "episode_count", n, &today);
    }
    let mut offset = 0;
    while offset < MAX_EPISODES {
        let off = offset.to_string();
        let v = ctx.get(&format!("{API}/shows/{id}/episodes"), &[("limit", "50"), ("offset", &off)]).await?;
        let items: Vec<&Value> = v["items"].as_array().into_iter().flatten().filter(|e| !e.is_null()).collect();
        d.content.extend(items.iter().map(|e| parse_episode(e)));
        if v["next"].is_null() || items.is_empty() {
            break;
        }
        offset += 50;
    }
    d.unavailable
        .push("followers, plays, listeners, consumption, demographics: not exposed by the Spotify Web API — import Spotify for Creators CSV exports".into());
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_ids() {
        assert_eq!(parse_show_id("https://open.spotify.com/show/4rOoJ6Egrf8K2IrywzwOMk?si=abc").unwrap(), "4rOoJ6Egrf8K2IrywzwOMk");
        assert_eq!(parse_show_id("spotify:show:4rOoJ6Egrf8K2IrywzwOMk").unwrap(), "4rOoJ6Egrf8K2IrywzwOMk");
        assert_eq!(parse_show_id("https://open.spotify.com/intl-pt/show/4rOoJ6Egrf8K2IrywzwOMk").unwrap(), "4rOoJ6Egrf8K2IrywzwOMk");
        assert!(parse_show_id("https://open.spotify.com/episode/x").is_err());
        assert!(parse_show_id("../../etc").is_err());
    }

    #[test]
    fn episode_parsing() {
        let e = parse_episode(
            &json!({"id":"e1","name":"Ep 1","release_date":"2026-02-01","duration_ms":61000,"external_urls":{"spotify":"https://open.spotify.com/episode/e1"},"images":[{"url":"i"}]}),
        );
        assert_eq!(e.duration_secs, Some(61));
        assert_eq!(e.kind, "episode");
        assert_eq!(e.thumbnail_url.as_deref(), Some("i"));
    }
}
