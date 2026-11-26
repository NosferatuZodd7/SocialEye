//! Facebook Pages via the Meta Graph API (Facebook Login for Business).
//! Personal profiles are not manageable through any official API.
use super::*;
use crate::error::{AppError, AppResult};
use crate::oauth::OAuthProvider;
use chrono::{Duration, Utc};
use serde_json::{json, Value};

pub static OAUTH: OAuthProvider = OAuthProvider {
    auth_url: "https://www.facebook.com/dialog/oauth",
    token_url: "https://graph.facebook.com/oauth/access_token",
    extra_auth_params: &[],
};

pub const SCOPES: &[&str] = &[
    "pages_show_list",
    "pages_read_engagement",
    "pages_manage_posts",
    "pages_manage_engagement",
    "read_insights",
    "business_management",
];

pub const SOURCE: &str = "Meta Graph API (Facebook Page)";
pub const DEFAULT_GRAPH_VERSION: &str = "v24.0";

/// Page insights metric → SocialEye metric name. Requested one by one so a retired
/// metric is reported as unavailable instead of failing the sync.
const PAGE_METRICS: &[(&str, &str)] = &[
    ("page_impressions_unique", "reach"),
    ("page_post_engagements", "engagement"),
    ("page_views_total", "page_views"),
    ("page_daily_follows_unique", "new_followers"),
];

pub const PAGES_FIELDS: &str = "id,name,username,access_token,picture{url},instagram_business_account{id,username,name,profile_picture_url}";

pub async fn me_accounts(ctx: &Ctx<'_>) -> AppResult<Vec<Value>> {
    let mut out = vec![];
    let mut url = ctx.graph("me/accounts");
    let mut first = true;
    loop {
        let v = if first {
            ctx.get(&url, &[("fields", PAGES_FIELDS), ("limit", "100")]).await?
        } else {
            ctx.get(&url, &[]).await?
        };
        out.extend(v["data"].as_array().cloned().unwrap_or_default());
        match v["paging"]["next"].as_str() {
            Some(n) if out.len() < 1000 => {
                url = n.to_string();
                first = false;
            }
            _ => break,
        }
    }
    Ok(out)
}

pub fn parse_pages(pages: &[Value]) -> Vec<DiscoveredAccount> {
    pages
        .iter()
        .filter_map(|p| {
            Some(DiscoveredAccount {
                account_type: "page".into(),
                external_id: p["id"].as_str()?.into(),
                name: p["name"].as_str().unwrap_or("Facebook Page").into(),
                handle: p["username"].as_str().map(Into::into),
                avatar_url: p["picture"]["data"]["url"].as_str().map(Into::into),
                token_override: p["access_token"].as_str().map(Into::into),
            })
        })
        .collect()
}

pub async fn discover(ctx: &Ctx<'_>) -> AppResult<Vec<DiscoveredAccount>> {
    let pages = parse_pages(&me_accounts(ctx).await?);
    if pages.is_empty() {
        return Err(AppError::NotFound {
            message: "no Facebook Pages found. Personal profiles cannot be managed via the API — you need a Page where you have a task (admin/editor) and must grant it during login.".into(),
        });
    }
    Ok(pages)
}

/// Graph insights → (metric day, value). `end_time` marks the end of the daily period.
pub fn parse_insights(v: &Value) -> Vec<(String, String, f64)> {
    let mut out = vec![];
    for m in v["data"].as_array().into_iter().flatten() {
        let name = m["name"].as_str().unwrap_or_default();
        for p in m["values"].as_array().into_iter().flatten() {
            let (Some(val), Some(end)) = (p["value"].as_f64(), p["end_time"].as_str()) else { continue };
            if let Ok(t) = chrono::DateTime::parse_from_str(end, "%Y-%m-%dT%H:%M:%S%z") {
                let day = (t.date_naive() - Duration::days(1)).to_string();
                out.push((name.to_string(), day, val));
            }
        }
    }
    out
}

pub fn parse_post(p: &Value) -> ContentRow {
    let mut stats = serde_json::Map::new();
    if let Some(n) = p["reactions"]["summary"]["total_count"].as_f64() {
        stats.insert("reactions".into(), json!(n));
    }
    if let Some(n) = p["comments"]["summary"]["total_count"].as_f64() {
        stats.insert("comments".into(), json!(n));
    }
    if let Some(n) = p["shares"]["count"].as_f64() {
        stats.insert("shares".into(), json!(n));
    }
    let msg = p["message"].as_str();
    ContentRow {
        external_id: p["id"].as_str().unwrap_or_default().into(),
        kind: "post".into(),
        title: msg.map(|m| m.chars().take(80).collect()),
        description: msg.map(Into::into),
        url: p["permalink_url"].as_str().map(Into::into),
        thumbnail_url: p["full_picture"].as_str().map(Into::into),
        published_at: p["created_time"].as_str().map(Into::into),
        duration_secs: None,
        stats,
    }
}

pub async fn sync(ctx: &Ctx<'_>, page_id: &str) -> AppResult<SyncData> {
    let mut d = SyncData { source: SOURCE, ..Default::default() };
    let today = Utc::now().date_naive().to_string();
    let page = ctx.get(&ctx.graph(page_id), &[("fields", "id,name,username,picture{url},followers_count,fan_count,link")]).await?;
    d.profile = parse_pages(std::slice::from_ref(&page)).into_iter().next();
    if let Some(n) = page["followers_count"].as_f64() {
        d.metric(None, "followers", n, &today);
    }
    if let Some(n) = page["fan_count"].as_f64() {
        d.metric(None, "page_likes", n, &today);
    }

    let posts = ctx
        .get(&ctx.graph(&format!("{page_id}/published_posts")), &[
            ("fields", "id,message,created_time,permalink_url,full_picture,shares,comments.summary(true).limit(0),reactions.summary(true).limit(0)"),
            ("limit", "100"),
        ])
        .await;
    if let Some(v) = d.soft("posts", posts)? {
        d.content.extend(v["data"].as_array().into_iter().flatten().map(parse_post));
    }

    let until = Utc::now().timestamp().to_string();
    let since = (Utc::now() - Duration::days(89)).timestamp().to_string();
    for (api_name, ours) in PAGE_METRICS {
        let r = ctx.get(&ctx.graph(&format!("{page_id}/insights")), &[("metric", api_name), ("period", "day"), ("since", &since), ("until", &until)]).await;
        if let Some(v) = d.soft(&format!("insight {api_name}"), r)? {
            for (_, day, val) in parse_insights(&v) {
                d.metric(None, ours, val, &day);
            }
        }
    }
    Ok(d)
}

pub async fn comments(ctx: &Ctx<'_>, post_id: &str) -> AppResult<Vec<Comment>> {
    let v = ctx.get(&ctx.graph(&format!("{post_id}/comments")), &[("fields", "id,message,from{name},created_time,is_hidden,like_count"), ("limit", "100"), ("filter", "stream")]).await?;
    Ok(v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| Comment {
            id: c["id"].as_str().unwrap_or_default().into(),
            author: c["from"]["name"].as_str().map(Into::into),
            text: c["message"].as_str().unwrap_or_default().into(),
            published_at: c["created_time"].as_str().map(Into::into),
            hidden: c["is_hidden"].as_bool(),
            like_count: c["like_count"].as_i64(),
        })
        .collect())
}

pub async fn moderate(ctx: &Ctx<'_>, comment_id: &str, m: &Moderation) -> AppResult<()> {
    let url = ctx.graph(comment_id);
    match m {
        Moderation::Hide => ctx.post_form(&url, &[("is_hidden", "true")]).await?,
        Moderation::Unhide => ctx.post_form(&url, &[("is_hidden", "false")]).await?,
        Moderation::Delete => crate::http::send(ctx.http.delete(&url).bearer_auth(ctx.token)).await?,
        Moderation::Reply { text } => ctx.post_form(&ctx.graph(&format!("{comment_id}/comments")), &[("message", text)]).await?,
    };
    Ok(())
}

pub async fn publish(ctx: &Ctx<'_>, page_id: &str, req: &PublishRequest) -> AppResult<PublishResult> {
    let sched_ts = req.scheduled_at.as_deref().map(|s| chrono::DateTime::parse_from_rfc3339(s).map(|d| d.timestamp().to_string())).transpose().map_err(|_| AppError::validation("invalid scheduled time"))?;
    let v = if let Some(path) = &req.media_path {
        let bytes = tokio::fs::read(path).await?;
        let name = std::path::Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or("photo.jpg").to_string();
        let mut form = reqwest::multipart::Form::new().text("caption", req.body.clone()).part("source", reqwest::multipart::Part::bytes(bytes).file_name(name));
        if let Some(ts) = &sched_ts {
            form = form.text("published", "false").text("scheduled_publish_time", ts.clone());
        }
        // Multipart bodies are not clonable, so this call is not retried automatically.
        let resp = ctx.http.post(ctx.graph(&format!("{page_id}/photos"))).bearer_auth(ctx.token).multipart(form).send().await?;
        let status = resp.status().as_u16();
        let text = resp.text().await.unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(crate::http::classify(status, None, &text));
        }
        serde_json::from_str::<Value>(&text)?
    } else {
        let mut form: Vec<(&str, &str)> = vec![("message", req.body.as_str())];
        if let Some(l) = &req.link {
            form.push(("link", l));
        }
        if let Some(ts) = &sched_ts {
            form.push(("published", "false"));
            form.push(("scheduled_publish_time", ts));
        }
        ctx.post_form(&ctx.graph(&format!("{page_id}/feed")), &form).await?
    };
    let id = v["post_id"].as_str().or(v["id"].as_str()).ok_or_else(|| AppError::Api { status: 500, message: "Meta did not return a post id".into(), retryable: false })?;
    Ok(PublishResult { external_id: id.into(), url: Some(format!("https://www.facebook.com/{id}")), scheduled: sched_ts.is_some() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insights_day_is_end_time_minus_one() {
        let v = json!({"data":[{"name":"page_impressions_unique","period":"day","values":[{"value":42,"end_time":"2026-03-02T08:00:00+0000"}]}]});
        assert_eq!(parse_insights(&v), vec![("page_impressions_unique".into(), "2026-03-01".into(), 42.0)]);
    }

    #[test]
    fn pages_keep_page_token() {
        let pages = vec![json!({"id":"1","name":"P","access_token":"PAGE_TOKEN","picture":{"data":{"url":"u"}}}), json!({"name":"no id"})];
        let p = parse_pages(&pages);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].token_override.as_deref(), Some("PAGE_TOKEN"));
        assert_eq!(p[0].avatar_url.as_deref(), Some("u"));
    }

    #[test]
    fn post_parsing() {
        let p = parse_post(&json!({"id":"1_2","message":"Hello","created_time":"2026-01-01T00:00:00+0000","reactions":{"summary":{"total_count":5}},"shares":{"count":2}}));
        assert_eq!(p.stats["reactions"], json!(5.0));
        assert_eq!(p.stats["shares"], json!(2.0));
        assert_eq!(p.title.as_deref(), Some("Hello"));
    }
}
