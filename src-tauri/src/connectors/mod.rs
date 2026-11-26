//! Platform connectors. Each platform is an independent module; a change in one API
//! cannot break the others. Operations a platform does not support return
//! `AppError::Unsupported` with a manual alternative — never a fake success.
pub mod facebook;
pub mod instagram;
pub mod spotify;
pub mod youtube;

use crate::error::{AppError, AppResult};
use crate::oauth::OAuthProvider;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Instagram,
    Facebook,
    Youtube,
    Spotify,
}

impl Platform {
    pub fn parse(s: &str) -> AppResult<Self> {
        Ok(match s {
            "instagram" => Platform::Instagram,
            "facebook" => Platform::Facebook,
            "youtube" => Platform::Youtube,
            "spotify" => Platform::Spotify,
            _ => return Err(AppError::validation(format!("unknown platform {s}"))),
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Instagram => "instagram",
            Platform::Facebook => "facebook",
            Platform::Youtube => "youtube",
            Platform::Spotify => "spotify",
        }
    }
    /// Developer-app provider whose credentials this platform uses.
    pub fn provider(self) -> &'static str {
        match self {
            Platform::Instagram | Platform::Facebook => "meta",
            Platform::Youtube => "google",
            Platform::Spotify => "spotify",
        }
    }
    pub fn oauth(self) -> &'static OAuthProvider {
        match self {
            Platform::Instagram | Platform::Facebook => &facebook::OAUTH,
            Platform::Youtube => &youtube::OAUTH,
            Platform::Spotify => &spotify::OAUTH,
        }
    }
    pub fn scopes(self) -> &'static [&'static str] {
        match self {
            Platform::Instagram => instagram::SCOPES,
            Platform::Facebook => facebook::SCOPES,
            Platform::Youtube => youtube::SCOPES,
            Platform::Spotify => spotify::SCOPES,
        }
    }
}

/// Everything a connector call needs. The token never leaves the Rust side.
pub struct Ctx<'a> {
    pub http: &'a reqwest::Client,
    pub token: &'a str,
    pub graph_version: &'a str,
}

impl Ctx<'_> {
    pub async fn get(&self, url: &str, query: &[(&str, &str)]) -> AppResult<Value> {
        crate::http::send(self.http.get(url).bearer_auth(self.token).query(query)).await
    }
    pub async fn post_form(&self, url: &str, form: &[(&str, &str)]) -> AppResult<Value> {
        crate::http::send(self.http.post(url).bearer_auth(self.token).form(form)).await
    }
    pub fn graph(&self, path: &str) -> String {
        format!("https://graph.facebook.com/{}/{}", self.graph_version, path.trim_start_matches('/'))
    }
}

/// An account discovered during connection (a channel, a Page, an IG business account…).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiscoveredAccount {
    pub account_type: String,
    pub external_id: String,
    pub name: String,
    pub handle: Option<String>,
    pub avatar_url: Option<String>,
    /// Some platforms (Facebook Pages) issue a per-account token.
    #[serde(skip)]
    pub token_override: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct ContentRow {
    pub external_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub published_at: Option<String>,
    pub duration_secs: Option<i64>,
    pub stats: serde_json::Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MetricRow {
    pub content_external_id: Option<String>,
    pub metric: String,
    pub value: f64,
    pub day: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Breakdown {
    pub kind: String,
    pub period_start: String,
    pub period_end: String,
    pub data: Value,
}

#[derive(Debug, Default, Serialize)]
pub struct SyncData {
    pub profile: Option<DiscoveredAccount>,
    pub content: Vec<ContentRow>,
    pub metrics: Vec<MetricRow>,
    pub breakdowns: Vec<Breakdown>,
    /// Metrics/features that could not be read, with the reason (shown in the UI).
    pub unavailable: Vec<String>,
    pub source: &'static str,
}

impl SyncData {
    pub fn metric(&mut self, content: Option<&str>, metric: &str, value: f64, day: &str) {
        self.metrics.push(MetricRow { content_external_id: content.map(Into::into), metric: metric.into(), value, day: day.into() });
    }
    /// Record a failure for an optional metric instead of failing the whole sync.
    /// Authorization and rate-limit failures still abort, since every later call would fail too.
    pub fn soft<T>(&mut self, label: &str, r: AppResult<T>) -> AppResult<Option<T>> {
        match r {
            Ok(v) => Ok(Some(v)),
            Err(e @ (AppError::Auth { .. } | AppError::RateLimited { .. } | AppError::Network { .. })) => Err(e),
            Err(e) => {
                self.unavailable.push(format!("{label}: {e}"));
                Ok(None)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Comment {
    pub id: String,
    pub author: Option<String>,
    pub text: String,
    pub published_at: Option<String>,
    pub hidden: Option<bool>,
    pub like_count: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Moderation {
    Hide,
    Unhide,
    Delete,
    Reply { text: String },
}

/// What to publish, assembled from a draft variant + its media asset.
#[derive(Debug, Clone, Default)]
pub struct PublishRequest {
    pub title: Option<String>,
    pub body: String,
    pub link: Option<String>,
    pub privacy: Option<String>,
    pub media_path: Option<String>,
    pub media_kind: Option<String>,
    pub media_public_url: Option<String>,
    pub thumbnail_path: Option<String>,
    pub tags: Vec<String>,
    /// RFC 3339. Platform-native scheduling when supported.
    pub scheduled_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PublishResult {
    pub external_id: String,
    pub url: Option<String>,
    /// true when the platform accepted it as a scheduled (not yet live) post.
    pub scheduled: bool,
}

// ---------------------------------------------------------------- capabilities

#[derive(Debug, Clone, Serialize)]
pub struct Capability {
    pub id: &'static str,
    pub status: &'static str, // "available" | "restricted" | "unsupported"
    pub requirement: &'static str,
    pub note: &'static str,
    pub manual_alternative: Option<&'static str>,
}

const fn cap(id: &'static str, status: &'static str, requirement: &'static str, note: &'static str, manual: Option<&'static str>) -> Capability {
    Capability { id, status, requirement, note, manual_alternative: manual }
}

pub fn capabilities(p: Platform) -> Vec<Capability> {
    match p {
        Platform::Instagram => vec![
            cap("profile", "available", "Instagram Business or Creator account linked to a Facebook Page", "Username, picture, follower/following/media counts.", None),
            cap("followers_count", "available", "instagram_basic", "Counts only. Daily net follower change for the last 30 days via insights.", None),
            cap("followers_list", "unsupported", "—", "Meta does not expose follower or following lists through any official API.", Some("Download your data in Instagram (Accounts Centre → Your information → Download your information, JSON) and use the Import wizard.")),
            cap("insights", "restricted", "instagram_manage_insights", "Reach and follower deltas. Some metrics are only available for accounts with 100+ followers.", None),
            cap("media", "available", "instagram_basic", "Posts, reels and carousels with like/comment counts.", None),
            cap("comments", "available", "instagram_manage_comments", "Read, reply, hide and delete comments on your media.", None),
            cap("publish", "restricted", "instagram_content_publish + app review", "Media must be at a public HTTPS URL. 50 API-published posts per 24h.", Some("Publish from the Instagram app, then sync.")),
            cap("schedule", "restricted", "SocialEye running at the scheduled time", "The API has no native scheduling; SocialEye publishes at the scheduled time while open.", Some("Use Meta Business Suite for server-side scheduling.")),
            cap("personal_account", "unsupported", "—", "Personal Instagram accounts are not supported by the Graph API.", Some("Switch to a Professional (Creator/Business) account in the Instagram app settings.")),
            cap("dm", "unsupported", "—", "Direct messages are out of scope.", None),
        ],
        Platform::Facebook => vec![
            cap("pages", "available", "pages_show_list", "All Pages you have a task on.", None),
            cap("page_insights", "restricted", "read_insights", "Reach, engagement, page views. Meta retires metrics regularly; missing ones are flagged.", None),
            cap("posts", "available", "pages_read_engagement", "Published posts with reactions, comments and shares.", None),
            cap("publish", "available", "pages_manage_posts", "Text, link and photo posts.", None),
            cap("schedule", "available", "pages_manage_posts", "Native scheduling 10 minutes to 30 days ahead.", None),
            cap("comments", "available", "pages_manage_engagement", "Read, reply, hide and delete.", None),
            cap("personal_profile", "unsupported", "—", "Meta removed API publishing and analytics for personal profiles.", Some("Manage your personal profile on facebook.com.")),
            cap("notifications", "restricted", "—", "Activity is derived from sync (new posts, comments); Page notifications are not exposed.", None),
        ],
        Platform::Youtube => vec![
            cap("channels", "available", "youtube.readonly", "Every channel on the authorized Google account / brand account.", None),
            cap("videos", "available", "youtube.readonly", "Uploads with views, likes, comments, duration.", None),
            cap("analytics", "available", "yt-analytics.readonly", "Daily views, watch time, average view duration, subscriber gains/losses, traffic sources, demographics, retention.", None),
            cap("impressions_ctr", "unsupported", "YouTube Reporting API (bulk reports)", "Impressions and CTR are not available in the YouTube Analytics API.", Some("See YouTube Studio → Analytics → Reach.")),
            cap("shorts_detection", "restricted", "—", "The API has no Shorts flag; videos ≤ 3 minutes are labelled 'probable Short'.", None),
            cap("upload", "available", "youtube.upload", "Resumable upload with progress. Unverified API projects can only upload as private.", None),
            cap("schedule", "available", "youtube.upload", "Native scheduling via private + publishAt.", None),
            cap("thumbnail", "restricted", "Verified YouTube account", "Custom thumbnails require a phone-verified channel.", None),
            cap("comments", "available", "youtube.force-ssl", "Read comments; hold for review, approve or reject.", None),
        ],
        Platform::Spotify => vec![
            cap("podcast_catalog", "available", "Spotify Web API", "Show name, cover, description, episodes, release dates, durations.", None),
            cap("podcast_followers", "unsupported", "Spotify for Creators", "The Web API does not expose show followers.", Some("Export analytics CSVs from Spotify for Creators and import them in SocialEye.")),
            cap("podcast_analytics", "unsupported", "Spotify for Creators", "Plays, listeners, consumption and demographics are not in any public Spotify API.", Some("Export analytics CSVs from Spotify for Creators and import them in SocialEye.")),
            cap("publish", "unsupported", "—", "Spotify offers no public podcast publishing endpoint.", Some("Publish at https://creators.spotify.com or via your podcast host's RSS feed.")),
            cap("comments", "unsupported", "—", "No public API for episode comments or Q&A.", Some("Moderate in Spotify for Creators.")),
        ],
    }
}

// ---------------------------------------------------------------- dispatch

pub async fn discover(p: Platform, ctx: &Ctx<'_>) -> AppResult<Vec<DiscoveredAccount>> {
    match p {
        Platform::Youtube => youtube::discover(ctx).await,
        Platform::Facebook => facebook::discover(ctx).await,
        Platform::Instagram => instagram::discover(ctx).await,
        Platform::Spotify => spotify::discover(ctx).await,
    }
}

pub async fn sync(p: Platform, ctx: &Ctx<'_>, account_type: &str, external_id: &str) -> AppResult<SyncData> {
    match p {
        Platform::Youtube => youtube::sync(ctx, external_id).await,
        Platform::Facebook => facebook::sync(ctx, external_id).await,
        Platform::Instagram => instagram::sync(ctx, external_id).await,
        Platform::Spotify => spotify::sync(ctx, account_type, external_id).await,
    }
}

pub async fn comments(p: Platform, ctx: &Ctx<'_>, content_id: &str) -> AppResult<Vec<Comment>> {
    match p {
        Platform::Youtube => youtube::comments(ctx, content_id).await,
        Platform::Facebook => facebook::comments(ctx, content_id).await,
        Platform::Instagram => instagram::comments(ctx, content_id).await,
        Platform::Spotify => Err(unsupported(p, "comments")),
    }
}

pub async fn moderate(p: Platform, ctx: &Ctx<'_>, comment_id: &str, m: &Moderation) -> AppResult<()> {
    match p {
        Platform::Youtube => youtube::moderate(ctx, comment_id, m).await,
        Platform::Facebook => facebook::moderate(ctx, comment_id, m).await,
        Platform::Instagram => instagram::moderate(ctx, comment_id, m).await,
        Platform::Spotify => Err(unsupported(p, "comments")),
    }
}

/// Build an Unsupported error from the capability matrix.
pub fn unsupported(p: Platform, id: &str) -> AppError {
    let c = capabilities(p).into_iter().find(|c| c.id == id);
    AppError::Unsupported {
        capability: format!("{}.{id}", p.as_str()),
        reason: c.as_ref().map(|c| c.note).unwrap_or("not supported by the official API").into(),
        manual_alternative: c.and_then(|c| c.manual_alternative).map(Into::into),
    }
}

/// Platform publishing rules checked before any API call.
pub fn validate_publish(p: Platform, req: &PublishRequest, now: chrono::DateTime<chrono::Utc>) -> Vec<String> {
    let mut errs = vec![];
    let len = req.body.chars().count();
    let sched = req.scheduled_at.as_deref().map(chrono::DateTime::parse_from_rfc3339);
    if let Some(Err(_)) = sched {
        errs.push("scheduled time is not a valid date".into());
    }
    let sched = sched.and_then(Result::ok).map(|d| d.with_timezone(&chrono::Utc));
    if let Some(s) = sched {
        if s <= now {
            errs.push("scheduled time must be in the future".into());
        }
    }
    match p {
        Platform::Instagram => {
            if len > 2200 {
                errs.push(format!("caption is {len} characters; Instagram allows 2,200"));
            }
            let tags = req.body.matches('#').count();
            if tags > 30 {
                errs.push(format!("{tags} hashtags; Instagram allows 30"));
            }
            match req.media_public_url.as_deref() {
                None => errs.push("Instagram requires an image or video at a public HTTPS URL (set it on the media asset)".into()),
                Some(u) if !u.starts_with("https://") => errs.push("media URL must use HTTPS".into()),
                _ => {}
            }
            if !matches!(req.media_kind.as_deref(), Some("image" | "video")) {
                errs.push("Instagram posts need an image or a video".into());
            }
        }
        Platform::Facebook => {
            if len == 0 && req.media_path.is_none() && req.link.is_none() {
                errs.push("a Facebook post needs text, a link or a photo".into());
            }
            if len > 63_206 {
                errs.push("text exceeds Facebook's 63,206 character limit".into());
            }
            if req.media_path.is_some() && req.media_kind.as_deref() != Some("image") {
                errs.push("SocialEye publishes photos to Pages; upload videos in Meta Business Suite".into());
            }
            if let Some(s) = sched {
                let d = s - now;
                if d < chrono::Duration::minutes(10) || d > chrono::Duration::days(30) {
                    errs.push("Facebook scheduling must be between 10 minutes and 30 days ahead".into());
                }
            }
        }
        Platform::Youtube => {
            let title = req.title.as_deref().unwrap_or("").trim();
            if title.is_empty() {
                errs.push("YouTube videos need a title".into());
            }
            if title.chars().count() > 100 {
                errs.push("YouTube titles are limited to 100 characters".into());
            }
            if title.contains('<') || title.contains('>') || req.body.contains('<') || req.body.contains('>') {
                errs.push("YouTube does not allow < or > in titles or descriptions".into());
            }
            if req.body.len() > 5000 {
                errs.push("YouTube descriptions are limited to 5,000 bytes".into());
            }
            if req.media_kind.as_deref() != Some("video") || req.media_path.is_none() {
                errs.push("select a local video file to upload".into());
            }
            if !matches!(req.privacy.as_deref(), None | Some("private" | "unlisted" | "public")) {
                errs.push("privacy must be private, unlisted or public".into());
            }
        }
        Platform::Spotify => errs.push("Spotify has no public publishing API — publish in Spotify for Creators".into()),
    }
    errs
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn every_platform_has_a_capability_matrix() {
        for p in [Platform::Instagram, Platform::Facebook, Platform::Youtube, Platform::Spotify] {
            let caps = capabilities(p);
            assert!(!caps.is_empty());
            for c in caps {
                assert!(matches!(c.status, "available" | "restricted" | "unsupported"));
                if c.status == "unsupported" {
                    assert!(!c.note.is_empty());
                }
            }
        }
    }

    #[test]
    fn unsupported_carries_manual_alternative() {
        match unsupported(Platform::Spotify, "publish") {
            AppError::Unsupported { manual_alternative: Some(m), .. } => assert!(m.contains("creators.spotify.com")),
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn instagram_validation() {
        let mut r = PublishRequest { body: "#a ".repeat(31), ..Default::default() };
        let e = validate_publish(Platform::Instagram, &r, now());
        assert!(e.iter().any(|m| m.contains("hashtags")));
        assert!(e.iter().any(|m| m.contains("public HTTPS")));
        r = PublishRequest { body: "hi".into(), media_kind: Some("image".into()), media_public_url: Some("https://x/y.jpg".into()), ..Default::default() };
        assert!(validate_publish(Platform::Instagram, &r, now()).is_empty());
    }

    #[test]
    fn facebook_schedule_window() {
        let r = PublishRequest { body: "hello".into(), scheduled_at: Some("2026-06-01T12:05:00Z".into()), ..Default::default() };
        assert!(validate_publish(Platform::Facebook, &r, now()).iter().any(|m| m.contains("10 minutes")));
        let r = PublishRequest { body: "hello".into(), scheduled_at: Some("2026-06-02T12:00:00Z".into()), ..Default::default() };
        assert!(validate_publish(Platform::Facebook, &r, now()).is_empty());
        let r = PublishRequest { body: "hello".into(), scheduled_at: Some("2026-05-01T12:00:00Z".into()), ..Default::default() };
        assert!(validate_publish(Platform::Facebook, &r, now()).iter().any(|m| m.contains("future")));
    }

    #[test]
    fn youtube_validation() {
        let r = PublishRequest { title: Some("<b>".into()), ..Default::default() };
        let e = validate_publish(Platform::Youtube, &r, now());
        assert!(e.iter().any(|m| m.contains("< or >")));
        assert!(e.iter().any(|m| m.contains("video file")));
        let r = PublishRequest { title: Some("Ok".into()), media_kind: Some("video".into()), media_path: Some("a.mp4".into()), ..Default::default() };
        assert!(validate_publish(Platform::Youtube, &r, now()).is_empty());
    }

    #[test]
    fn spotify_never_validates() {
        assert!(!validate_publish(Platform::Spotify, &PublishRequest::default(), now()).is_empty());
    }
}
