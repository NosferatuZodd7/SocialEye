//! Instagram Graph API (Instagram API with Facebook Login) for Business/Creator accounts.
//! Follower *lists* are not available from any official API — see `ig_export`.
use super::*;
use crate::error::{AppError, AppResult};
use chrono::{Duration, Utc};
use serde_json::{json, Value};

pub const SCOPES: &[&str] = &[
    "instagram_basic",
    "instagram_manage_insights",
    "instagram_manage_comments",
    "instagram_content_publish",
    "pages_show_list",
    "pages_read_engagement",
    "business_management",
];

pub const SOURCE: &str = "Meta Graph API (Instagram)";

pub fn parse_ig_accounts(pages: &[Value]) -> Vec<DiscoveredAccount> {
    pages
        .iter()
        .filter_map(|p| {
            let ig = &p["instagram_business_account"];
            Some(DiscoveredAccount {
                account_type: "professional".into(),
                external_id: ig["id"].as_str()?.into(),
                name: ig["name"].as_str().or(ig["username"].as_str()).unwrap_or("Instagram account").into(),
                handle: ig["username"].as_str().map(Into::into),
                avatar_url: ig["profile_picture_url"].as_str().map(Into::into),
                token_override: p["access_token"].as_str().map(Into::into),
            })
        })
        .collect()
}

pub async fn discover(ctx: &Ctx<'_>) -> AppResult<Vec<DiscoveredAccount>> {
    let out = parse_ig_accounts(&facebook::me_accounts(ctx).await?);
    if out.is_empty() {
        return Err(AppError::NotFound {
            message: "no Instagram Professional account found. Convert your account to Creator/Business, link it to a Facebook Page, and grant that Page during login.".into(),
        });
    }
    Ok(out)
}

pub fn parse_media(m: &Value) -> ContentRow {
    let kind = match (m["media_product_type"].as_str(), m["media_type"].as_str()) {
        (Some("REELS"), _) => "reel",
        (Some("STORY"), _) => "story",
        (_, Some("CAROUSEL_ALBUM")) => "carousel",
        (_, Some("VIDEO")) => "video",
        _ => "photo",
    };
    let mut stats = serde_json::Map::new();
    if let Some(n) = m["like_count"].as_f64() {
        stats.insert("likes".into(), json!(n));
    }
    if let Some(n) = m["comments_count"].as_f64() {
        stats.insert("comments".into(), json!(n));
    }
    let caption = m["caption"].as_str();
    ContentRow {
        external_id: m["id"].as_str().unwrap_or_default().into(),
        kind: kind.into(),
        title: caption.map(|c| c.chars().take(80).collect()),
        description: caption.map(Into::into),
        url: m["permalink"].as_str().map(Into::into),
        thumbnail_url: m["thumbnail_url"].as_str().or(m["media_url"].as_str()).map(Into::into),
        published_at: m["timestamp"].as_str().map(Into::into),
        duration_secs: None,
        stats,
    }
}

pub async fn sync(ctx: &Ctx<'_>, ig_id: &str) -> AppResult<SyncData> {
    let mut d = SyncData { source: SOURCE, ..Default::default() };
    let today = Utc::now().date_naive().to_string();
    let p = ctx.get(&ctx.graph(ig_id), &[("fields", "id,username,name,profile_picture_url,followers_count,follows_count,media_count")]).await?;
    d.profile = Some(DiscoveredAccount {
        account_type: "professional".into(),
        external_id: ig_id.into(),
        name: p["name"].as_str().or(p["username"].as_str()).unwrap_or("Instagram").into(),
        handle: p["username"].as_str().map(Into::into),
        avatar_url: p["profile_picture_url"].as_str().map(Into::into),
        token_override: None,
    });
    for (k, m) in [("followers_count", "followers"), ("follows_count", "following"), ("media_count", "media_count")] {
        if let Some(n) = p[k].as_f64() {
            d.metric(None, m, n, &today);
        }
    }

    let media = ctx.get(&ctx.graph(&format!("{ig_id}/media")), &[("fields", "id,caption,media_type,media_product_type,permalink,thumbnail_url,media_url,timestamp,like_count,comments_count"), ("limit", "100")]).await;
    if let Some(v) = d.soft("media", media)? {
        d.content.extend(v["data"].as_array().into_iter().flatten().map(parse_media));
    }

    // follower_count is only served for the last 30 days and for accounts with 100+ followers.
    let until = Utc::now().timestamp().to_string();
    let since = (Utc::now() - Duration::days(28)).timestamp().to_string();
    for (api, ours) in [("reach", "reach"), ("follower_count", "follower_delta")] {
        let r = ctx.get(&ctx.graph(&format!("{ig_id}/insights")), &[("metric", api), ("period", "day"), ("since", &since), ("until", &until)]).await;
        if let Some(v) = d.soft(&format!("insight {api}"), r)? {
            for (_, day, val) in facebook::parse_insights(&v) {
                d.metric(None, ours, val, &day);
            }
        }
    }
    d.unavailable.push("followers list / following list: not available via API — use the data export import".into());
    Ok(d)
}

pub async fn comments(ctx: &Ctx<'_>, media_id: &str) -> AppResult<Vec<Comment>> {
    let v = ctx.get(&ctx.graph(&format!("{media_id}/comments")), &[("fields", "id,text,username,timestamp,hidden,like_count"), ("limit", "100")]).await?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Comment {
            id: c["id"].as_str().unwrap_or_default().into(),
            author: c["username"].as_str().map(Into::into),
            text: c["text"].as_str().unwrap_or_default().into(),
            published_at: c["timestamp"].as_str().map(Into::into),
            hidden: c["hidden"].as_bool(),
            like_count: c["like_count"].as_i64(),
        })
        .collect())
}

pub async fn moderate(ctx: &Ctx<'_>, comment_id: &str, m: &Moderation) -> AppResult<()> {
    let url = ctx.graph(comment_id);
    match m {
        Moderation::Hide => ctx.post_form(&url, &[("hide", "true")]).await?,
        Moderation::Unhide => ctx.post_form(&url, &[("hide", "false")]).await?,
        Moderation::Delete => crate::http::send(ctx.http.delete(&url).bearer_auth(ctx.token)).await?,
        Moderation::Reply { text } => ctx.post_form(&ctx.graph(&format!("{comment_id}/replies")), &[("message", text)]).await?,
    };
    Ok(())
}

/// Two-step container publish. Only call when the user confirmed publishing now.
pub async fn publish(ctx: &Ctx<'_>, ig_id: &str, req: &PublishRequest) -> AppResult<PublishResult> {
    let url = req.media_public_url.as_deref().ok_or_else(|| AppError::validation("Instagram needs a public media URL"))?;
    let mut form: Vec<(&str, &str)> = vec![("caption", req.body.as_str())];
    if req.media_kind.as_deref() == Some("video") {
        form.extend([("media_type", "REELS"), ("video_url", url)]);
    } else {
        form.push(("image_url", url));
    }
    let container = ctx.post_form(&ctx.graph(&format!("{ig_id}/media")), &form).await?;
    let cid = container["id"].as_str().ok_or_else(|| AppError::Api { status: 500, message: "no media container id".into(), retryable: false })?.to_string();

    // Video containers are processed asynchronously: poll until FINISHED (max ~5 min).
    for _ in 0..60 {
        let s = ctx.get(&ctx.graph(&cid), &[("fields", "status_code,status")]).await?;
        match s["status_code"].as_str() {
            Some("FINISHED") | None => break,
            Some("ERROR") | Some("EXPIRED") => {
                return Err(AppError::Api { status: 422, message: format!("Instagram could not process the media: {}", s["status"].as_str().unwrap_or("unknown error")), retryable: false })
            }
            _ => tokio::time::sleep(std::time::Duration::from_secs(5)).await,
        }
    }
    let r = ctx.post_form(&ctx.graph(&format!("{ig_id}/media_publish")), &[("creation_id", &cid)]).await?;
    let id = r["id"].as_str().ok_or_else(|| AppError::Api { status: 500, message: "publish returned no media id".into(), retryable: false })?.to_string();
    let link = ctx.get(&ctx.graph(&id), &[("fields", "permalink")]).await.ok().and_then(|v| v["permalink"].as_str().map(String::from));
    Ok(PublishResult { external_id: id, url: link, scheduled: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pages_with_ig_accounts() {
        let pages = vec![
            json!({"id":"p1","access_token":"T","instagram_business_account":{"id":"ig1","username":"me"}}),
            json!({"id":"p2","access_token":"T2"}),
        ];
        let a = parse_ig_accounts(&pages);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].handle.as_deref(), Some("me"));
        assert_eq!(a[0].token_override.as_deref(), Some("T"));
    }

    #[test]
    fn media_kinds() {
        assert_eq!(parse_media(&json!({"id":"1","media_type":"VIDEO","media_product_type":"REELS"})).kind, "reel");
        assert_eq!(parse_media(&json!({"id":"2","media_type":"CAROUSEL_ALBUM","media_product_type":"FEED"})).kind, "carousel");
        let p = parse_media(&json!({"id":"3","media_type":"IMAGE","like_count":4,"comments_count":1}));
        assert_eq!(p.kind, "photo");
        assert_eq!(p.stats["likes"], json!(4.0));
    }
}
