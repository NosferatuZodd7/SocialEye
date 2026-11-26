//! YouTube Data API v3 + YouTube Analytics API v2.
use super::*;
use crate::error::{AppError, AppResult};
use crate::oauth::OAuthProvider;
use chrono::{Duration, Utc};
use serde_json::{json, Value};

pub static OAUTH: OAuthProvider = OAuthProvider {
    auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    // offline + consent: guarantees a refresh token for background sync.
    extra_auth_params: &[("access_type", "offline"), ("prompt", "consent")],
};

pub const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/youtube.readonly",
    "https://www.googleapis.com/auth/yt-analytics.readonly",
    "https://www.googleapis.com/auth/youtube.upload",
    "https://www.googleapis.com/auth/youtube.force-ssl",
];

const API: &str = "https://www.googleapis.com/youtube/v3";
const ANALYTICS: &str = "https://youtubeanalytics.googleapis.com/v2/reports";
pub const SOURCE: &str = "YouTube Data API v3 / YouTube Analytics API";
/// YouTube allows Shorts up to 3 minutes; the API has no explicit Shorts flag.
const SHORT_MAX_SECS: i64 = 180;
const MAX_VIDEOS: usize = 250;

pub async fn discover(ctx: &Ctx<'_>) -> AppResult<Vec<DiscoveredAccount>> {
    let v = ctx.get(&format!("{API}/channels"), &[("part", "snippet"), ("mine", "true"), ("maxResults", "50")]).await?;
    let out = parse_channels(&v);
    if out.is_empty() {
        return Err(AppError::NotFound { message: "this Google account has no YouTube channel".into() });
    }
    Ok(out)
}

pub fn parse_channels(v: &Value) -> Vec<DiscoveredAccount> {
    v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|c| DiscoveredAccount {
            account_type: "channel".into(),
            external_id: c["id"].as_str().unwrap_or_default().into(),
            name: c["snippet"]["title"].as_str().unwrap_or("YouTube channel").into(),
            handle: c["snippet"]["customUrl"].as_str().map(Into::into),
            avatar_url: c["snippet"]["thumbnails"]["default"]["url"].as_str().map(Into::into),
            token_override: None,
        })
        .filter(|a| !a.external_id.is_empty())
        .collect()
}

/// ISO 8601 duration (PT#H#M#S, optionally P#D) → seconds.
pub fn parse_duration(s: &str) -> Option<i64> {
    let s = s.strip_prefix('P')?;
    let (date, time) = s.split_once('T').unwrap_or((s, ""));
    let mut total = 0i64;
    let mut take = |part: &str, units: &[(char, i64)]| -> Option<()> {
        let mut num = String::new();
        for ch in part.chars() {
            if ch.is_ascii_digit() {
                num.push(ch);
            } else {
                let mul = units.iter().find(|(u, _)| *u == ch)?.1;
                total += num.parse::<i64>().ok()? * mul;
                num.clear();
            }
        }
        num.is_empty().then_some(())
    };
    take(date, &[('D', 86_400), ('W', 604_800)])?;
    take(time, &[('H', 3600), ('M', 60), ('S', 1)])?;
    Some(total)
}

fn num(v: &Value) -> Option<f64> {
    v.as_str().and_then(|s| s.parse().ok()).or_else(|| v.as_f64())
}

pub fn parse_video(v: &Value) -> ContentRow {
    let sn = &v["snippet"];
    let st = &v["statistics"];
    let duration = v["contentDetails"]["duration"].as_str().and_then(parse_duration);
    let live = sn["liveBroadcastContent"].as_str().unwrap_or("none") != "none" || !v["liveStreamingDetails"].is_null();
    let kind = if live {
        "live"
    } else if duration.is_some_and(|d| d > 0 && d <= SHORT_MAX_SECS) {
        "short"
    } else {
        "video"
    };
    let mut stats = serde_json::Map::new();
    for (k, src) in [("views", "viewCount"), ("likes", "likeCount"), ("comments", "commentCount")] {
        if let Some(n) = num(&st[src]) {
            stats.insert(k.into(), json!(n));
        }
    }
    if kind == "short" {
        stats.insert("kind_confidence".into(), json!("heuristic"));
    }
    if let Some(p) = v["status"]["privacyStatus"].as_str() {
        stats.insert("privacy".into(), json!(p));
    }
    let id = v["id"].as_str().unwrap_or_default();
    ContentRow {
        external_id: id.into(),
        kind: kind.into(),
        title: sn["title"].as_str().map(Into::into),
        description: sn["description"].as_str().map(Into::into),
        url: Some(if kind == "short" { format!("https://www.youtube.com/shorts/{id}") } else { format!("https://www.youtube.com/watch?v={id}") }),
        thumbnail_url: sn["thumbnails"]["medium"]["url"].as_str().or(sn["thumbnails"]["default"]["url"].as_str()).map(Into::into),
        published_at: sn["publishedAt"].as_str().map(Into::into),
        duration_secs: duration,
        stats,
    }
}

/// Analytics API returns `{columnHeaders:[{name}], rows:[[..]]}`; turn rows into maps.
pub fn report_rows(v: &Value) -> Vec<serde_json::Map<String, Value>> {
    let names: Vec<&str> = v["columnHeaders"].as_array().into_iter().flatten().filter_map(|h| h["name"].as_str()).collect();
    v["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_array())
        .map(|r| names.iter().zip(r).map(|(n, x)| (n.to_string(), x.clone())).collect())
        .collect()
}

pub async fn sync(ctx: &Ctx<'_>, channel_id: &str) -> AppResult<SyncData> {
    let mut d = SyncData { source: SOURCE, ..Default::default() };
    let today = Utc::now().date_naive();
    let today_s = today.to_string();
    let start = (today - Duration::days(90)).to_string();

    let ch = ctx.get(&format!("{API}/channels"), &[("part", "snippet,statistics,contentDetails"), ("id", channel_id)]).await?;
    let c = ch["items"].get(0).ok_or_else(|| AppError::NotFound { message: "channel not found or no longer accessible".into() })?;
    d.profile = parse_channels(&ch).into_iter().next();
    let s = &c["statistics"];
    if s["hiddenSubscriberCount"].as_bool() == Some(true) {
        d.unavailable.push("subscribers: hidden by channel settings".into());
    } else if let Some(n) = num(&s["subscriberCount"]) {
        d.metric(None, "subscribers", n, &today_s);
    }
    if let Some(n) = num(&s["viewCount"]) {
        d.metric(None, "total_views", n, &today_s);
    }
    if let Some(n) = num(&s["videoCount"]) {
        d.metric(None, "video_count", n, &today_s);
    }

    // Uploads playlist → video ids (newest first), bounded per sync.
    let uploads = c["contentDetails"]["relatedPlaylists"]["uploads"].as_str().unwrap_or_default().to_string();
    let mut ids = vec![];
    let mut page: Option<String> = None;
    while !uploads.is_empty() && ids.len() < MAX_VIDEOS {
        let mut q = vec![("part", "contentDetails"), ("playlistId", uploads.as_str()), ("maxResults", "50")];
        if let Some(p) = &page {
            q.push(("pageToken", p.as_str()));
        }
        let v = ctx.get(&format!("{API}/playlistItems"), &q).await?;
        ids.extend(v["items"].as_array().into_iter().flatten().filter_map(|i| i["contentDetails"]["videoId"].as_str().map(String::from)));
        page = v["nextPageToken"].as_str().map(String::from);
        if page.is_none() {
            break;
        }
    }
    for chunk in ids.chunks(50) {
        let joined = chunk.join(",");
        let v = ctx.get(&format!("{API}/videos"), &[("part", "snippet,statistics,contentDetails,status,liveStreamingDetails"), ("id", &joined)]).await?;
        d.content.extend(v["items"].as_array().into_iter().flatten().map(parse_video));
    }

    // Channel-level daily analytics.
    let ids_param = format!("channel=={channel_id}");
    let daily = ctx
        .get(
            ANALYTICS,
            &[
                ("ids", &ids_param),
                ("startDate", &start),
                ("endDate", &today_s),
                ("dimensions", "day"),
                ("sort", "day"),
                ("metrics", "views,estimatedMinutesWatched,averageViewDuration,subscribersGained,subscribersLost,likes,comments,shares"),
            ],
        )
        .await;
    if let Some(v) = d.soft("daily analytics", daily)? {
        for r in report_rows(&v) {
            let day = r["day"].as_str().unwrap_or_default().to_string();
            for (k, m) in [
                ("views", "views"),
                ("estimatedMinutesWatched", "watch_minutes"),
                ("averageViewDuration", "avg_view_duration_secs"),
                ("subscribersGained", "subscribers_gained"),
                ("subscribersLost", "subscribers_lost"),
                ("likes", "likes"),
                ("comments", "comments"),
                ("shares", "shares"),
            ] {
                if let Some(n) = r.get(k).and_then(Value::as_f64) {
                    d.metric(None, m, n, &day);
                }
            }
        }
    }

    // Per-video analytics for the top videos of the period.
    let per_video = ctx
        .get(
            ANALYTICS,
            &[
                ("ids", &ids_param),
                ("startDate", &start),
                ("endDate", &today_s),
                ("dimensions", "video"),
                ("sort", "-views"),
                ("maxResults", "200"),
                ("metrics", "views,estimatedMinutesWatched,averageViewDuration,averageViewPercentage"),
            ],
        )
        .await;
    if let Some(v) = d.soft("per-video analytics", per_video)? {
        for r in report_rows(&v) {
            let Some(vid) = r.get("video").and_then(Value::as_str) else { continue };
            if let Some(row) = d.content.iter_mut().find(|c| c.external_id == vid) {
                for (k, m) in [
                    ("estimatedMinutesWatched", "watch_minutes_90d"),
                    ("averageViewDuration", "avg_view_duration_secs"),
                    ("averageViewPercentage", "avg_view_pct"),
                    ("views", "views_90d"),
                ] {
                    if let Some(n) = r.get(k) {
                        row.stats.insert(m.into(), n.clone());
                    }
                }
            }
        }
    }

    for (kind, dims, metrics) in [
        ("traffic_sources", "insightTrafficSourceType", "views,estimatedMinutesWatched"),
        ("demographics", "ageGroup,gender", "viewerPercentage"),
        ("geography", "country", "views"),
    ] {
        let r = ctx.get(ANALYTICS, &[("ids", &ids_param), ("startDate", &start), ("endDate", &today_s), ("dimensions", dims), ("metrics", metrics)]).await;
        if let Some(v) = d.soft(kind, r)? {
            d.breakdowns.push(Breakdown {
                kind: kind.into(),
                period_start: start.clone(),
                period_end: today_s.clone(),
                data: Value::Array(report_rows(&v).into_iter().map(Value::Object).collect()),
            });
        }
    }
    d.unavailable.push("impressions, impressions CTR: not exposed by the YouTube Analytics API (see YouTube Studio)".into());
    Ok(d)
}

/// Audience retention curve for one video (fetched on demand, not stored).
pub async fn retention(ctx: &Ctx<'_>, channel_id: &str, video_id: &str) -> AppResult<Vec<serde_json::Map<String, Value>>> {
    let today = Utc::now().date_naive();
    let v = ctx
        .get(
            ANALYTICS,
            &[
                ("ids", &format!("channel=={channel_id}")),
                ("startDate", "2005-02-14"),
                ("endDate", &today.to_string()),
                ("dimensions", "elapsedVideoTimeRatio"),
                ("metrics", "audienceWatchRatio,relativeRetentionPerformance"),
                ("filters", &format!("video=={video_id}")),
            ],
        )
        .await?;
    Ok(report_rows(&v))
}

pub fn parse_comments(v: &Value) -> Vec<Comment> {
    v["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            let c = &t["snippet"]["topLevelComment"];
            let s = &c["snippet"];
            Comment {
                id: c["id"].as_str().unwrap_or_default().into(),
                author: s["authorDisplayName"].as_str().map(Into::into),
                text: s["textOriginal"].as_str().or(s["textDisplay"].as_str()).unwrap_or_default().into(),
                published_at: s["publishedAt"].as_str().map(Into::into),
                hidden: s["moderationStatus"].as_str().map(|m| m != "published"),
                like_count: s["likeCount"].as_i64(),
            }
        })
        .collect()
}

pub async fn comments(ctx: &Ctx<'_>, video_id: &str) -> AppResult<Vec<Comment>> {
    let v = ctx
        .get(
            &format!("{API}/commentThreads"),
            &[("part", "snippet"), ("videoId", video_id), ("maxResults", "100"), ("order", "time"), ("textFormat", "plainText")],
        )
        .await?;
    Ok(parse_comments(&v))
}

pub async fn moderate(ctx: &Ctx<'_>, comment_id: &str, m: &Moderation) -> AppResult<()> {
    let status = match m {
        Moderation::Hide => "heldForReview",
        Moderation::Unhide => "published",
        Moderation::Delete => "rejected", // channel owners reject; only authors can hard-delete
        Moderation::Reply { text } => {
            let body = json!({"snippet": {"parentId": comment_id, "textOriginal": text}});
            crate::http::send(ctx.http.post(format!("{API}/comments")).bearer_auth(ctx.token).query(&[("part", "snippet")]).json(&body)).await?;
            return Ok(());
        }
    };
    crate::http::send(
        ctx.http.post(format!("{API}/comments/setModerationStatus")).bearer_auth(ctx.token).query(&[("id", comment_id), ("moderationStatus", status)]),
    )
    .await?;
    Ok(())
}

pub fn video_metadata(req: &PublishRequest) -> Value {
    let mut status = json!({ "privacyStatus": req.privacy.as_deref().unwrap_or("private"), "selfDeclaredMadeForKids": false });
    if let Some(at) = &req.scheduled_at {
        // Native scheduling: the video stays private until publishAt.
        status["privacyStatus"] = json!("private");
        status["publishAt"] = json!(at);
    }
    json!({
        "snippet": { "title": req.title.clone().unwrap_or_default(), "description": req.body, "tags": req.tags },
        "status": status,
    })
}

const CHUNK: usize = 8 * 1024 * 1024; // multiple of 256 KiB as required

/// Resumable upload. `progress(sent, total)` is called after each chunk.
pub async fn upload(ctx: &Ctx<'_>, req: &PublishRequest, progress: &(dyn Fn(u64, u64) + Send + Sync)) -> AppResult<PublishResult> {
    use tokio::io::AsyncReadExt;
    let path = req.media_path.as_deref().ok_or_else(|| AppError::validation("no video file"))?;
    let mut file = tokio::fs::File::open(path).await?;
    let total = file.metadata().await?.len();
    let init = ctx
        .http
        .post("https://www.googleapis.com/upload/youtube/v3/videos")
        .bearer_auth(ctx.token)
        .query(&[("uploadType", "resumable"), ("part", "snippet,status")])
        .header("X-Upload-Content-Length", total.to_string())
        .header("X-Upload-Content-Type", "video/*")
        .json(&video_metadata(req))
        .send()
        .await?;
    if !init.status().is_success() {
        let s = init.status().as_u16();
        return Err(crate::http::classify(s, None, &init.text().await.unwrap_or_default()));
    }
    let session = init.headers().get("location").and_then(|h| h.to_str().ok()).map(String::from).ok_or_else(|| AppError::Api {
        status: 500,
        message: "no upload session returned".into(),
        retryable: false,
    })?;

    let mut sent = 0u64;
    let mut buf = vec![0u8; CHUNK];
    loop {
        let mut n = 0;
        while n < CHUNK {
            let r = file.read(&mut buf[n..]).await?;
            if r == 0 {
                break;
            }
            n += r;
        }
        let end = sent + n as u64;
        let range = if total == 0 { "bytes */0".to_string() } else { format!("bytes {}-{}/{}", sent, end - 1, total) };
        let mut attempt = 0;
        let resp = loop {
            let r = ctx.http.put(&session).header("Content-Range", &range).body(buf[..n].to_vec()).send().await;
            match r {
                Ok(r) if r.status().as_u16() >= 500 && attempt < 3 => {}
                Ok(r) => break r,
                Err(e) if attempt >= 3 => return Err(e.into()),
                Err(_) => {}
            }
            attempt += 1;
            tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt))).await;
        };
        sent = end;
        progress(sent, total);
        match resp.status().as_u16() {
            308 => continue, // Resume Incomplete: send the next chunk
            200 | 201 => {
                let v: Value = resp.json().await?;
                let id = v["id"]
                    .as_str()
                    .ok_or_else(|| AppError::Api { status: 500, message: "upload finished without a video id".into(), retryable: false })?
                    .to_string();
                return Ok(PublishResult {
                    url: Some(format!("https://www.youtube.com/watch?v={id}")),
                    external_id: id,
                    scheduled: req.scheduled_at.is_some(),
                });
            }
            s => return Err(crate::http::classify(s, None, &resp.text().await.unwrap_or_default())),
        }
    }
}

/// Update title/description/visibility of an existing video. videos.update replaces the
/// whole snippet (categoryId is mandatory), so the current snippet is fetched and modified.
pub async fn update_metadata(ctx: &Ctx<'_>, video_id: &str, title: &str, description: &str, privacy: &str) -> AppResult<()> {
    let probe = PublishRequest {
        title: Some(title.into()),
        body: description.into(),
        privacy: Some(privacy.into()),
        media_kind: Some("video".into()),
        media_path: Some("-".into()),
        ..Default::default()
    };
    let errs = validate_publish(Platform::Youtube, &probe, Utc::now());
    if !errs.is_empty() {
        return Err(AppError::validation(errs.join("; ")));
    }
    let v = ctx.get(&format!("{API}/videos"), &[("part", "snippet,status"), ("id", video_id)]).await?;
    let item = v["items"].get(0).ok_or_else(|| AppError::NotFound { message: "video not found or not owned by this channel".into() })?;
    let mut snippet = item["snippet"].clone();
    snippet["title"] = json!(title);
    snippet["description"] = json!(description);
    if let Some(o) = snippet.as_object_mut() {
        // Read-only fields that the API rejects on update.
        for k in ["thumbnails", "channelTitle", "localized", "publishedAt", "channelId", "liveBroadcastContent"] {
            o.remove(k);
        }
    }
    let mut status = json!({ "privacyStatus": privacy });
    if let Some(m) = item["status"]["selfDeclaredMadeForKids"].as_bool() {
        status["selfDeclaredMadeForKids"] = json!(m);
    }
    let body = json!({ "id": video_id, "snippet": snippet, "status": status });
    crate::http::send(ctx.http.put(format!("{API}/videos")).bearer_auth(ctx.token).query(&[("part", "snippet,status")]).json(&body)).await?;
    Ok(())
}

pub async fn set_thumbnail(ctx: &Ctx<'_>, video_id: &str, image_path: &str) -> AppResult<()> {
    let bytes = tokio::fs::read(image_path).await?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(AppError::validation("thumbnails must be 2 MB or smaller"));
    }
    let mime = if image_path.to_lowercase().ends_with(".png") { "image/png" } else { "image/jpeg" };
    crate::http::send(
        ctx.http
            .post("https://www.googleapis.com/upload/youtube/v3/thumbnails/set")
            .bearer_auth(ctx.token)
            .query(&[("videoId", video_id), ("uploadType", "media")])
            .header("Content-Type", mime)
            .body(bytes),
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("PT1H2M3S"), Some(3723));
        assert_eq!(parse_duration("PT59S"), Some(59));
        assert_eq!(parse_duration("P1DT1M"), Some(86_460));
        assert_eq!(parse_duration("P0D"), Some(0));
        assert_eq!(parse_duration("garbage"), None);
    }

    #[test]
    fn video_parsing_detects_shorts_and_stats() {
        let v = json!({"id":"abc","snippet":{"title":"T","publishedAt":"2026-01-01T00:00:00Z","liveBroadcastContent":"none","thumbnails":{"medium":{"url":"https://i/x.jpg"}}},
            "statistics":{"viewCount":"1500","likeCount":"20","commentCount":"3"},"contentDetails":{"duration":"PT45S"},"status":{"privacyStatus":"public"}});
        let r = parse_video(&v);
        assert_eq!(r.kind, "short");
        assert_eq!(r.stats["views"], json!(1500.0));
        assert_eq!(r.stats["kind_confidence"], json!("heuristic"));
        assert_eq!(r.url.as_deref(), Some("https://www.youtube.com/shorts/abc"));
        let long = json!({"id":"d","snippet":{},"statistics":{},"contentDetails":{"duration":"PT10M"}});
        assert_eq!(parse_video(&long).kind, "video");
    }

    #[test]
    fn analytics_rows() {
        let v = json!({"columnHeaders":[{"name":"day"},{"name":"views"}],"rows":[["2026-01-01",10],["2026-01-02",12]]});
        let rows = report_rows(&v);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1]["views"], json!(12));
        assert!(report_rows(&json!({})).is_empty());
    }

    #[test]
    fn metadata_scheduling_forces_private() {
        let r =
            PublishRequest { title: Some("x".into()), privacy: Some("public".into()), scheduled_at: Some("2027-01-01T00:00:00Z".into()), ..Default::default() };
        let m = video_metadata(&r);
        assert_eq!(m["status"]["privacyStatus"], "private");
        assert_eq!(m["status"]["publishAt"], "2027-01-01T00:00:00Z");
    }

    #[test]
    fn comment_parsing() {
        let v = json!({"items":[{"snippet":{"topLevelComment":{"id":"c1","snippet":{"authorDisplayName":"A","textOriginal":"hi","likeCount":2,"moderationStatus":"published"}}}}]});
        let c = parse_comments(&v);
        assert_eq!(c[0].id, "c1");
        assert_eq!(c[0].hidden, Some(false));
    }
}
