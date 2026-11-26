//! Tauri commands: the only surface the frontend can call. Every input is validated
//! here or in the module it delegates to; secrets never cross this boundary.
use crate::app::{load_account, normalize_time, AppState, Ui};
use crate::connectors::{self, Ctx, Moderation, Platform};
use crate::db::{self, query_json};
use crate::error::{AppError, AppResult};
use crate::{automations, csv_import, ig_export, reports, secrets, updates};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct TauriUi(pub AppHandle);

impl Ui for TauriUi {
    fn open_url(&self, url: &str) -> AppResult<()> {
        use tauri_plugin_opener::OpenerExt;
        self.0.opener().open_url(url, None::<&str>).map_err(|e| AppError::Io { message: e.to_string() })
    }
    fn desktop_notify(&self, title: &str, body: &str) {
        use tauri_plugin_notification::NotificationExt;
        let _ = self.0.notification().builder().title(title).body(body).show();
    }
    fn emit(&self, event: &str, payload: Value) {
        let _ = self.0.emit(event, payload);
    }
}

type R<T> = AppResult<T>;

fn require_confirmation(confirmed: bool, what: &str) -> R<()> {
    if confirmed {
        Ok(())
    } else {
        Err(AppError::validation(format!("{what} requires explicit confirmation")))
    }
}

/// Escape LIKE wildcards in user search input.
fn like(q: &str) -> String {
    format!("%{}%", q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"))
}

fn allow_asset(app: &AppHandle, path: &str) {
    let _ = app.asset_protocol_scope().allow_file(path);
}

// ------------------------------------------------------------------ app & settings

#[tauri::command]
pub fn app_info(s: State<'_, AppState>) -> R<Value> {
    let c = s.db.conn();
    Ok(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "schema_version": db::schema_version(&c)?,
        "data_dir": s.data_dir.to_string_lossy(),
        "telemetry": false,
    }))
}

#[tauri::command]
pub fn settings_get(s: State<'_, AppState>) -> R<Value> {
    let rows = query_json(&s.db.conn(), "SELECT key, value FROM settings", [])?;
    Ok(Value::Object(rows.into_iter().filter_map(|r| Some((r["key"].as_str()?.to_string(), r["value"].clone()))).collect()))
}

const SETTING_KEYS: &[&str] =
    &["language", "theme", "sync_interval_minutes", "desktop_notifications", "dashboard_widgets", "onboarding_done", "update_channel", "update_auto_check"];

#[tauri::command]
pub fn settings_set(s: State<'_, AppState>, key: String, value: String) -> R<()> {
    if !SETTING_KEYS.contains(&key.as_str()) {
        return Err(AppError::validation(format!("unknown setting {key}")));
    }
    if key == "sync_interval_minutes" && !value.parse::<i64>().is_ok_and(|v| v == 0 || (15..=10_080).contains(&v)) {
        return Err(AppError::validation("sync interval must be 0 (off) or 15–10080 minutes"));
    }
    if key == "update_channel" && !matches!(value.as_str(), "testing" | "stable") {
        return Err(AppError::validation("update channel must be testing or stable"));
    }
    db::set_setting(&s.db.conn(), &key, &value)
}

#[derive(Deserialize)]
pub struct ProviderInput {
    provider: String,
    client_id: String,
    /// None = keep the stored secret; Some("") = remove it.
    client_secret: Option<String>,
    redirect_port: u16,
    api_version: Option<String>,
}

#[tauri::command]
pub fn providers_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    let c = s.db.conn();
    let mut out = vec![];
    for p in ["google", "meta", "spotify"] {
        let row = query_json(&c, "SELECT client_id, redirect_port, api_version, updated_at FROM platform_apps WHERE platform = ?1", [p])?.pop();
        let has_secret = s.secrets.get(&secrets::client_secret_key(p))?.is_some();
        out.push(json!({"provider": p, "configured": row.is_some(), "has_secret": has_secret, "app": row}));
    }
    Ok(out)
}

#[tauri::command]
pub fn provider_save(s: State<'_, AppState>, input: ProviderInput) -> R<()> {
    if !["google", "meta", "spotify"].contains(&input.provider.as_str()) {
        return Err(AppError::validation("unknown provider"));
    }
    let id = input.client_id.trim();
    if id.is_empty() || id.len() > 200 || id.chars().any(char::is_whitespace) {
        return Err(AppError::validation("client ID looks invalid"));
    }
    if input.redirect_port < 1024 {
        return Err(AppError::validation("redirect port must be 1024–65535"));
    }
    if let Some(v) = &input.api_version {
        if !v.is_empty() && !regex::Regex::new(r"^v\d{1,2}\.\d$").unwrap().is_match(v) {
            return Err(AppError::validation("Graph API version must look like v24.0"));
        }
    }
    match input.client_secret.as_deref().map(str::trim) {
        Some("") => s.secrets.delete(&secrets::client_secret_key(&input.provider))?,
        Some(sec) => s.secrets.set(&secrets::client_secret_key(&input.provider), sec)?,
        None => {}
    }
    let c = s.db.conn();
    c.execute(
        "INSERT INTO platform_apps(platform, client_id, redirect_port, api_version, updated_at) VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(platform) DO UPDATE SET client_id = excluded.client_id, redirect_port = excluded.redirect_port,
           api_version = excluded.api_version, updated_at = datetime('now')",
        (&input.provider, id, input.redirect_port, input.api_version.filter(|v| !v.is_empty())),
    )?;
    db::audit(&c, "provider.save", &input.provider, "")
}

#[tauri::command]
pub fn capabilities(platform: String) -> R<Value> {
    Ok(serde_json::to_value(connectors::capabilities(Platform::parse(&platform)?))?)
}

// ------------------------------------------------------------------ accounts

#[tauri::command]
pub fn accounts_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT a.id, a.platform, a.account_type, a.external_id, a.name, a.handle, a.avatar_url, a.parent_account_id, a.status,
                a.scopes, a.token_expires_at, a.auto_sync, a.last_sync_at, a.last_sync_status, a.last_error, a.created_at,
                (SELECT unavailable FROM sync_runs r WHERE r.account_id = a.id AND r.status = 'ok' ORDER BY r.id DESC LIMIT 1) AS unavailable
         FROM accounts a ORDER BY a.platform, a.name",
        [],
    )
}

#[tauri::command]
pub async fn account_connect(app: AppHandle, s: State<'_, AppState>, platform: String) -> R<Vec<i64>> {
    let p = Platform::parse(&platform)?;
    let ids = s.connect(&TauriUi(app.clone()), p).await?;
    // First sync right away so the user sees data.
    for id in &ids {
        let _ = s.sync_account(&TauriUi(app.clone()), *id).await;
    }
    Ok(ids)
}

#[tauri::command]
pub fn account_disconnect(s: State<'_, AppState>, id: i64) -> R<()> {
    s.disconnect(id)
}

#[tauri::command]
pub fn account_remove(s: State<'_, AppState>, id: i64, confirmed: bool) -> R<()> {
    require_confirmation(confirmed, "removing an account and its history")?;
    s.remove_account(id)
}

#[tauri::command]
pub fn account_set_auto_sync(s: State<'_, AppState>, id: i64, enabled: bool) -> R<()> {
    s.db.conn().execute("UPDATE accounts SET auto_sync = ?1 WHERE id = ?2", (enabled, id))?;
    Ok(())
}

#[tauri::command]
pub async fn account_sync(app: AppHandle, s: State<'_, AppState>, id: i64) -> R<Value> {
    s.sync_account(&TauriUi(app), id).await
}

#[tauri::command]
pub fn sync_runs(s: State<'_, AppState>, account_id: Option<i64>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT r.*, a.name, a.platform FROM sync_runs r JOIN accounts a ON a.id = r.account_id
         WHERE (?1 IS NULL OR r.account_id = ?1) ORDER BY r.id DESC LIMIT 200",
        [account_id],
    )
}

#[tauri::command]
pub async fn spotify_add_podcast(app: AppHandle, s: State<'_, AppState>, parent_id: i64, show: String) -> R<i64> {
    let id = s.add_podcast(parent_id, &show).await?;
    let _ = s.sync_account(&TauriUi(app), id).await;
    Ok(id)
}

#[tauri::command]
pub async fn spotify_saved_shows(s: State<'_, AppState>, parent_id: i64) -> R<Value> {
    let acc = load_account(&s.db.conn(), parent_id)?;
    let token = s.access_token(&acc).await?;
    let ctx = Ctx { http: &s.http, token: &token, graph_version: "" };
    Ok(serde_json::to_value(connectors::spotify::saved_shows(&ctx).await?)?)
}

// ------------------------------------------------------------------ content, metrics, comments

#[derive(Deserialize, Default)]
pub struct ContentQuery {
    account_id: Option<i64>,
    platform: Option<String>,
    kind: Option<String>,
    from: Option<String>,
    to: Option<String>,
    search: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
}

#[tauri::command]
pub fn content_list(s: State<'_, AppState>, q: ContentQuery) -> R<Vec<Value>> {
    let order = match q.sort.as_deref() {
        Some("views") => "CAST(json_extract(c.stats_json, '$.views') AS REAL) DESC",
        Some("engagement") => "(IFNULL(json_extract(c.stats_json, '$.likes'),0) + IFNULL(json_extract(c.stats_json, '$.comments'),0) + IFNULL(json_extract(c.stats_json, '$.reactions'),0) + IFNULL(json_extract(c.stats_json, '$.shares'),0)) DESC",
        Some("oldest") => "c.published_at ASC",
        _ => "c.published_at DESC",
    };
    let sql = format!(
        "SELECT c.*, a.platform, a.name AS account_name FROM content_items c JOIN accounts a ON a.id = c.account_id
         WHERE (?1 IS NULL OR c.account_id = ?1) AND (?2 IS NULL OR a.platform = ?2) AND (?3 IS NULL OR c.kind = ?3)
           AND (?4 IS NULL OR c.published_at >= ?4) AND (?5 IS NULL OR c.published_at <= ?5 || 'T23:59:59Z')
           AND (?6 IS NULL OR c.title LIKE ?6 ESCAPE '\\' OR c.description LIKE ?6 ESCAPE '\\')
         ORDER BY {order} LIMIT ?7"
    );
    query_json(&s.db.conn(), &sql, (q.account_id, q.platform, q.kind, q.from, q.to, q.search.as_deref().map(like), q.limit.unwrap_or(500).clamp(1, 5000)))
}

#[derive(Deserialize)]
pub struct SeriesQuery {
    account_ids: Vec<i64>,
    metrics: Vec<String>,
    from: String,
    to: String,
    content_id: Option<i64>,
}

#[tauri::command]
pub fn metrics_series(s: State<'_, AppState>, q: SeriesQuery) -> R<Vec<Value>> {
    if q.account_ids.len() > 200 || q.metrics.len() > 50 {
        return Err(AppError::validation("too many series requested"));
    }
    let accs = serde_json::to_string(&q.account_ids)?;
    let mets = serde_json::to_string(&q.metrics)?;
    query_json(
        &s.db.conn(),
        "SELECT m.account_id, m.metric, m.day, m.value, m.source FROM metric_snapshots m
         WHERE m.account_id IN (SELECT value FROM json_each(?1)) AND m.metric IN (SELECT value FROM json_each(?2))
           AND m.day BETWEEN ?3 AND ?4 AND ((?5 IS NULL AND m.content_id IS NULL) OR m.content_id = ?5)
         ORDER BY m.day",
        (accs, mets, q.from, q.to, q.content_id),
    )
}

/// Latest value of every account-level metric, with its date and source.
#[tauri::command]
pub fn metrics_latest(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT m.account_id, m.metric, m.value, m.day, m.source FROM metric_snapshots m
         JOIN (SELECT account_id, metric, MAX(day) AS day FROM metric_snapshots WHERE content_id IS NULL GROUP BY account_id, metric) x
           ON x.account_id = m.account_id AND x.metric = m.metric AND x.day = m.day
         WHERE m.content_id IS NULL",
        [],
    )
}

#[tauri::command]
pub fn breakdowns(s: State<'_, AppState>, account_id: i64) -> R<Vec<Value>> {
    query_json(&s.db.conn(), "SELECT kind, period_start, period_end, data_json, source, captured_at FROM breakdowns WHERE account_id = ?1", [account_id])
}

#[tauri::command]
pub async fn youtube_retention(s: State<'_, AppState>, account_id: i64, video_id: String) -> R<Value> {
    let acc = load_account(&s.db.conn(), account_id)?;
    if acc.platform != Platform::Youtube {
        return Err(AppError::validation("not a YouTube account"));
    }
    let token = s.access_token(&acc).await?;
    let ctx = Ctx { http: &s.http, token: &token, graph_version: "" };
    Ok(serde_json::to_value(connectors::youtube::retention(&ctx, &acc.external_id, &video_id).await?)?)
}

#[tauri::command]
pub async fn youtube_update_video(
    s: State<'_, AppState>,
    account_id: i64,
    video_id: String,
    title: String,
    description: String,
    privacy: String,
    confirmed: bool,
) -> R<()> {
    require_confirmation(confirmed, "changing a published video")?;
    ctx_for(&s, account_id, async |ctx, p| {
        if p != Platform::Youtube {
            return Err(AppError::validation("not a YouTube account"));
        }
        connectors::youtube::update_metadata(ctx, &video_id, &title, &description, &privacy).await
    })
    .await?;
    let c = s.db.conn();
    c.execute(
        "UPDATE content_items SET title = ?1, description = ?2 WHERE account_id = ?3 AND external_id = ?4",
        (&title, &description, account_id, &video_id),
    )?;
    db::audit(&c, "youtube.update_video", &video_id, &privacy)
}

async fn ctx_for<T>(s: &AppState, account_id: i64, f: impl AsyncFnOnce(&Ctx<'_>, Platform) -> R<T>) -> R<T> {
    let (acc, gv) = {
        let c = s.db.conn();
        (load_account(&c, account_id)?, crate::app::graph_version(&c))
    };
    let token = s.access_token(&acc).await?;
    let ctx = Ctx { http: &s.http, token: &token, graph_version: &gv };
    f(&ctx, acc.platform).await
}

#[tauri::command]
pub async fn comments_list(s: State<'_, AppState>, account_id: i64, content_external_id: String) -> R<Value> {
    let r = ctx_for(&s, account_id, async |ctx, p| connectors::comments(p, ctx, &content_external_id).await).await?;
    Ok(serde_json::to_value(r)?)
}

#[tauri::command]
pub async fn comment_moderate(s: State<'_, AppState>, account_id: i64, comment_id: String, moderation: Moderation, confirmed: bool) -> R<()> {
    if matches!(moderation, Moderation::Delete) {
        require_confirmation(confirmed, "deleting a comment")?;
    }
    if let Moderation::Reply { text } = &moderation {
        if text.trim().is_empty() || text.len() > 8000 {
            return Err(AppError::validation("reply must be 1–8000 characters"));
        }
    }
    ctx_for(&s, account_id, async |ctx, p| connectors::moderate(p, ctx, &comment_id, &moderation).await).await?;
    db::audit(&s.db.conn(), "comment.moderate", &comment_id, &format!("{moderation:?}").chars().take(40).collect::<String>())
}

// ------------------------------------------------------------------ media library

fn media_kind(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("jpg" | "jpeg" | "png" | "webp" | "gif") => "image",
        Some("mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi") => "video",
        Some("mp3" | "m4a" | "wav" | "aac" | "ogg" | "flac") => "audio",
        _ => "other",
    }
}

#[tauri::command]
pub fn media_add(app: AppHandle, s: State<'_, AppState>, paths: Vec<String>) -> R<Vec<i64>> {
    let c = s.db.conn();
    let mut ids = vec![];
    for p in paths {
        let path = PathBuf::from(&p);
        let meta = std::fs::metadata(&path).map_err(|_| AppError::validation(format!("cannot read {p}")))?;
        if !meta.is_file() {
            return Err(AppError::validation(format!("{p} is not a file")));
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
        let id: i64 = c.query_row(
            "INSERT INTO media_assets(path, name, kind, size_bytes) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET size_bytes = excluded.size_bytes RETURNING id",
            (&p, &name, media_kind(&path), meta.len() as i64),
            |r| r.get(0),
        )?;
        allow_asset(&app, &p);
        ids.push(id);
    }
    Ok(ids)
}

#[tauri::command]
pub fn media_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    let rows = query_json(&s.db.conn(), "SELECT * FROM media_assets ORDER BY id DESC", [])?;
    Ok(rows
        .into_iter()
        .map(|mut r| {
            let exists = r["path"].as_str().is_some_and(|p| Path::new(p).is_file());
            r["missing"] = json!(!exists);
            r
        })
        .collect())
}

#[tauri::command]
pub fn media_set_public_url(s: State<'_, AppState>, id: i64, url: Option<String>) -> R<()> {
    let url = url.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    if let Some(u) = &url {
        let parsed = url::Url::parse(u).map_err(|_| AppError::validation("invalid URL"))?;
        if parsed.scheme() != "https" {
            return Err(AppError::validation("public media URL must be HTTPS"));
        }
    }
    s.db.conn().execute("UPDATE media_assets SET public_url = ?1 WHERE id = ?2", (url, id))?;
    Ok(())
}

#[tauri::command]
pub fn media_remove(s: State<'_, AppState>, id: i64) -> R<()> {
    // Only the library reference is removed; the user's file is never deleted.
    s.db.conn().execute("DELETE FROM media_assets WHERE id = ?1", [id])?;
    Ok(())
}

// ------------------------------------------------------------------ drafts, variants, templates

#[derive(Deserialize)]
pub struct DraftInput {
    id: Option<i64>,
    title: String,
    body: String,
    category: Option<String>,
    tags: String,
    hashtags: String,
    media_asset_id: Option<i64>,
}

#[tauri::command]
pub fn draft_save(s: State<'_, AppState>, d: DraftInput) -> R<i64> {
    if d.title.trim().is_empty() || d.title.len() > 300 {
        return Err(AppError::validation("title must be 1–300 characters"));
    }
    let c = s.db.conn();
    match d.id {
        Some(id) => {
            c.execute(
                "UPDATE drafts SET title=?1, body=?2, category=?3, tags=?4, hashtags=?5, media_asset_id=?6, updated_at=datetime('now') WHERE id=?7",
                (&d.title, &d.body, &d.category, &d.tags, &d.hashtags, d.media_asset_id, id),
            )?;
            Ok(id)
        }
        None => Ok(c.query_row(
            "INSERT INTO drafts(title, body, category, tags, hashtags, media_asset_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
            (&d.title, &d.body, &d.category, &d.tags, &d.hashtags, d.media_asset_id),
            |r| r.get(0),
        )?),
    }
}

#[tauri::command]
pub fn drafts_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT d.*, m.name AS media_name, m.kind AS media_kind, m.path AS media_path,
           (SELECT json_group_array(json_object('id', v.id, 'account_id', v.account_id, 'platform', a.platform, 'status', v.status, 'scheduled_at', v.scheduled_at))
              FROM draft_variants v JOIN accounts a ON a.id = v.account_id WHERE v.draft_id = d.id) AS variants_json
         FROM drafts d LEFT JOIN media_assets m ON m.id = d.media_asset_id ORDER BY d.updated_at DESC",
        [],
    )
}

#[tauri::command]
pub fn draft_delete(s: State<'_, AppState>, id: i64, confirmed: bool) -> R<()> {
    require_confirmation(confirmed, "deleting a draft")?;
    s.db.conn().execute("DELETE FROM drafts WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn variants_list(s: State<'_, AppState>, draft_id: i64) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT v.*, a.platform, a.name AS account_name FROM draft_variants v JOIN accounts a ON a.id = v.account_id WHERE v.draft_id = ?1 ORDER BY a.platform",
        [draft_id],
    )
}

#[derive(Deserialize)]
pub struct VariantInput {
    id: Option<i64>,
    draft_id: i64,
    account_id: i64,
    title: Option<String>,
    body: String,
    media_asset_id: Option<i64>,
    thumbnail_asset_id: Option<i64>,
    link: Option<String>,
    privacy: Option<String>,
    scheduled_at: Option<String>,
}

#[tauri::command]
pub fn variant_save(s: State<'_, AppState>, v: VariantInput) -> R<i64> {
    let sched = v.scheduled_at.as_deref().filter(|x| !x.is_empty()).map(normalize_time).transpose()?;
    if let Some(l) = &v.link {
        if !l.is_empty() && !matches!(url::Url::parse(l).map(|u| u.scheme().to_string()).as_deref(), Ok("https" | "http")) {
            return Err(AppError::validation("link must be an http(s) URL"));
        }
    }
    let link = v.link.filter(|l| !l.is_empty());
    let c = s.db.conn();
    if let Some(id) = v.id {
        let st: String = c.query_row("SELECT status FROM draft_variants WHERE id = ?1", [id], |r| r.get(0))?;
        if matches!(st.as_str(), "published" | "publishing")
            || (st == "scheduled" && c.query_row("SELECT external_id IS NOT NULL FROM draft_variants WHERE id = ?1", [id], |r| r.get::<_, bool>(0))?)
        {
            return Err(AppError::validation("this version is already on the platform; edit it there"));
        }
        c.execute(
            "UPDATE draft_variants SET account_id=?1, title=?2, body=?3, media_asset_id=?4, thumbnail_asset_id=?5, link=?6, privacy=?7, scheduled_at=?8, status='draft', error=NULL WHERE id=?9",
            (v.account_id, &v.title, &v.body, v.media_asset_id, v.thumbnail_asset_id, &link, &v.privacy, &sched, id),
        )?;
        Ok(id)
    } else {
        Ok(c.query_row(
            "INSERT INTO draft_variants(draft_id, account_id, title, body, media_asset_id, thumbnail_asset_id, link, privacy, scheduled_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) RETURNING id",
            (v.draft_id, v.account_id, &v.title, &v.body, v.media_asset_id, v.thumbnail_asset_id, &link, &v.privacy, &sched),
            |r| r.get(0),
        )?)
    }
}

#[tauri::command]
pub fn variant_delete(s: State<'_, AppState>, id: i64) -> R<()> {
    s.db.conn().execute("DELETE FROM draft_variants WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn variant_validate(s: State<'_, AppState>, id: i64) -> R<Vec<String>> {
    s.validate_variant(id)
}

#[tauri::command]
pub async fn variant_publish(app: AppHandle, s: State<'_, AppState>, id: i64, confirmed: bool) -> R<Value> {
    require_confirmation(confirmed, "publishing")?;
    s.publish_variant(&TauriUi(app), id, false).await
}

#[tauri::command]
pub fn variant_unschedule(s: State<'_, AppState>, id: i64) -> R<()> {
    let n = s.db.conn().execute("UPDATE draft_variants SET status = 'draft' WHERE id = ?1 AND status = 'scheduled' AND external_id IS NULL", [id])?;
    if n == 0 {
        return Err(AppError::validation("only locally scheduled items can be unscheduled here; platform-scheduled posts are managed on the platform"));
    }
    Ok(())
}

#[tauri::command]
pub fn publish_history(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT h.*, a.name AS account_name, a.platform, d.title AS draft_title FROM publish_history h
         LEFT JOIN accounts a ON a.id = h.account_id LEFT JOIN draft_variants v ON v.id = h.variant_id LEFT JOIN drafts d ON d.id = v.draft_id
         ORDER BY h.id DESC LIMIT 500",
        [],
    )
}

#[tauri::command]
pub fn templates_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(&s.db.conn(), "SELECT * FROM templates ORDER BY name", [])
}

#[tauri::command]
pub fn template_save(s: State<'_, AppState>, id: Option<i64>, name: String, body: String, hashtags: String) -> R<i64> {
    if name.trim().is_empty() {
        return Err(AppError::validation("template name is required"));
    }
    let c = s.db.conn();
    match id {
        Some(id) => {
            c.execute("UPDATE templates SET name=?1, body=?2, hashtags=?3 WHERE id=?4", (&name, &body, &hashtags, id))?;
            Ok(id)
        }
        None => Ok(c.query_row("INSERT INTO templates(name, body, hashtags) VALUES (?1, ?2, ?3) RETURNING id", (&name, &body, &hashtags), |r| r.get(0))?),
    }
}

#[tauri::command]
pub fn template_delete(s: State<'_, AppState>, id: i64) -> R<()> {
    s.db.conn().execute("DELETE FROM templates WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn calendar_items(s: State<'_, AppState>, from: String, to: String) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT 'variant' AS source, v.id, d.id AS draft_id, COALESCE(v.title, d.title) AS title, a.platform, a.name AS account_name,
                v.status, COALESCE(v.scheduled_at, v.published_at) AS at, NULL AS url
         FROM draft_variants v JOIN drafts d ON d.id = v.draft_id JOIN accounts a ON a.id = v.account_id
         WHERE COALESCE(v.scheduled_at, v.published_at) BETWEEN ?1 AND ?2 || 'T23:59:59Z'
         UNION ALL
         SELECT 'content', c.id, NULL, COALESCE(c.title, c.kind), a.platform, a.name, 'published', c.published_at, c.url
         FROM content_items c JOIN accounts a ON a.id = c.account_id
         WHERE c.published_at BETWEEN ?1 AND ?2 || 'T23:59:59Z'
         ORDER BY at",
        (from, to),
    )
}

// ------------------------------------------------------------------ automations & notifications

#[tauri::command]
pub fn automations_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT a.*, (SELECT count(*) FROM automation_runs r WHERE r.automation_id = a.id) AS run_count FROM automations a ORDER BY a.id",
        [],
    )
}

#[tauri::command]
pub fn automation_save(s: State<'_, AppState>, id: Option<i64>, name: String, enabled: bool, rule: Value) -> R<i64> {
    let rule = automations::parse_rule(&rule)?;
    if name.trim().is_empty() {
        return Err(AppError::validation("name is required"));
    }
    let json = serde_json::to_string(&rule)?;
    let c = s.db.conn();
    match id {
        Some(id) => {
            c.execute("UPDATE automations SET name=?1, enabled=?2, rule_json=?3 WHERE id=?4", (&name, enabled, &json, id))?;
            Ok(id)
        }
        None => Ok(c.query_row("INSERT INTO automations(name, enabled, rule_json) VALUES (?1, ?2, ?3) RETURNING id", (&name, enabled, &json), |r| r.get(0))?),
    }
}

#[tauri::command]
pub fn automation_set_enabled(s: State<'_, AppState>, id: i64, enabled: bool) -> R<()> {
    s.db.conn().execute("UPDATE automations SET enabled = ?1 WHERE id = ?2", (enabled, id))?;
    Ok(())
}

#[tauri::command]
pub fn automation_delete(s: State<'_, AppState>, id: i64) -> R<()> {
    s.db.conn().execute("DELETE FROM automations WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn automation_runs(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(&s.db.conn(), "SELECT r.*, a.name FROM automation_runs r JOIN automations a ON a.id = r.automation_id ORDER BY r.id DESC LIMIT 300", [])
}

#[tauri::command]
pub fn notifications_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(
        &s.db.conn(),
        "SELECT n.*, a.platform, a.name AS account_name FROM notifications n LEFT JOIN accounts a ON a.id = n.account_id ORDER BY n.id DESC LIMIT 500",
        [],
    )
}

#[tauri::command]
pub fn notifications_mark_read(s: State<'_, AppState>, ids: Option<Vec<i64>>) -> R<()> {
    let c = s.db.conn();
    match ids {
        Some(ids) => c.execute("UPDATE notifications SET read = 1 WHERE id IN (SELECT value FROM json_each(?1))", [serde_json::to_string(&ids)?])?,
        None => c.execute("UPDATE notifications SET read = 1", [])?,
    };
    Ok(())
}

#[tauri::command]
pub fn notifications_clear(s: State<'_, AppState>) -> R<()> {
    s.db.conn().execute("DELETE FROM notifications WHERE read = 1", [])?;
    Ok(())
}

// ------------------------------------------------------------------ imports

#[tauri::command]
pub fn ig_import(s: State<'_, AppState>, label: String, files: Vec<ig_export::ImportFile>) -> R<Value> {
    let p = ig_export::parse(&files)?;
    let label = if label.trim().is_empty() { chrono::Utc::now().format("Export %Y-%m-%d").to_string() } else { label.chars().take(100).collect() };
    let mut c = s.db.conn();
    let tx = c.transaction()?;
    let id: i64 = tx.query_row(
        "INSERT INTO ig_export_imports(label, followers_count, following_count, warnings) VALUES (?1, ?2, ?3, ?4) RETURNING id",
        (&label, p.followers.len() as i64, p.following.len() as i64, p.warnings.join("\n")),
        |r| r.get(0),
    )?;
    {
        let mut st = tx.prepare("INSERT OR IGNORE INTO ig_export_entries(import_id, list, username, followed_at) VALUES (?1, ?2, ?3, ?4)")?;
        for (list, m) in [("followers", &p.followers), ("following", &p.following)] {
            for (u, ts) in m {
                st.execute((id, list, u, ts))?;
            }
        }
    }
    db::audit(&tx, "instagram.import", &id.to_string(), &p.used_files.join(", "))?;
    tx.commit()?;
    Ok(json!({"id": id, "followers": p.followers.len(), "following": p.following.len(), "used_files": p.used_files, "warnings": p.warnings}))
}

#[tauri::command]
pub fn ig_imports_list(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(&s.db.conn(), "SELECT * FROM ig_export_imports ORDER BY id DESC", [])
}

#[tauri::command]
pub fn ig_import_delete(s: State<'_, AppState>, id: i64) -> R<()> {
    s.db.conn().execute("DELETE FROM ig_export_imports WHERE id = ?1", [id])?;
    Ok(())
}

#[tauri::command]
pub fn ig_compare(s: State<'_, AppState>, import_id: i64, previous_id: Option<i64>) -> R<Value> {
    let c = s.db.conn();
    let set = |id: i64, list: &str| -> R<BTreeSet<String>> {
        Ok(c.prepare("SELECT username FROM ig_export_entries WHERE import_id = ?1 AND list = ?2")?
            .query_map((id, list), |r| r.get(0))?
            .collect::<Result<_, _>>()?)
    };
    let (fs, fg) = (set(import_id, "followers")?, set(import_id, "following")?);
    let prev = previous_id.map(|p| Ok::<_, AppError>((set(p, "followers")?, set(p, "following")?))).transpose()?;
    Ok(serde_json::to_value(ig_export::compare(&fs, &fg, prev.as_ref().map(|p| &p.0), prev.as_ref().map(|p| &p.1)))?)
}

/// Import metrics from a CSV export into an account (e.g. Spotify for Creators).
#[tauri::command]
pub fn csv_import_metrics(s: State<'_, AppState>, account_id: i64, file_name: String, content: String) -> R<Value> {
    let p = csv_import::parse(&content)?;
    let source = format!("CSV import: {}", file_name.chars().take(120).collect::<String>());
    let today = chrono::Utc::now().date_naive().to_string();
    let mut c = s.db.conn();
    load_account(&c, account_id)?;
    let tx = c.transaction()?;
    let (mut stored, mut unmatched) = (0, BTreeSet::new());
    for r in &p.rows {
        let cid: Option<i64> = match &r.episode {
            Some(t) => {
                let id = tx
                    .query_row("SELECT id FROM content_items WHERE account_id = ?1 AND lower(trim(title)) = lower(trim(?2))", (account_id, t), |x| x.get(0))
                    .ok();
                if id.is_none() {
                    unmatched.insert(t.clone());
                    continue;
                }
                id
            }
            None => None,
        };
        db::put_metric(&tx, account_id, cid, &r.metric, r.value, r.day.as_deref().unwrap_or(&today), &source)?;
        stored += 1;
    }
    db::audit(&tx, "csv.import", &account_id.to_string(), &source)?;
    tx.commit()?;
    Ok(json!({"stored": stored, "metrics": p.metrics, "ignored_columns": p.ignored_columns, "skipped_rows": p.skipped_rows, "unmatched_episodes": unmatched}))
}

// ------------------------------------------------------------------ reports, backup, search, audit

#[tauri::command]
pub fn export_report(s: State<'_, AppState>, path: String, format: String, report: reports::Report) -> R<()> {
    reports::export(&report, Path::new(&path), &format)?;
    db::audit(&s.db.conn(), "report.export", &format, &report.title)
}

#[tauri::command]
pub fn backup_create(s: State<'_, AppState>, path: String) -> R<()> {
    s.db.backup_to(Path::new(&path))?;
    db::audit(&s.db.conn(), "backup.create", "", "")
}

#[tauri::command]
pub fn backup_restore(s: State<'_, AppState>, path: String, confirmed: bool) -> R<()> {
    require_confirmation(confirmed, "restoring a backup (replaces all current data)")?;
    // Safety net: keep the pre-restore state next to the database.
    s.db.backup_to(&s.data_dir.join("pre-restore.db"))?;
    s.db.restore_from(Path::new(&path))?;
    db::audit(&s.db.conn(), "backup.restore", "", "")
}

const EXPORT_TABLES: &[&str] = &[
    "accounts",
    "content_items",
    "metric_snapshots",
    "breakdowns",
    "drafts",
    "draft_variants",
    "templates",
    "media_assets",
    "automations",
    "notifications",
    "publish_history",
    "ig_export_imports",
    "ig_export_entries",
    "settings",
];

/// Portable JSON export of all data. Tokens and client secrets live in the OS
/// credential store and are therefore never included.
#[tauri::command]
pub fn data_export_json(s: State<'_, AppState>, path: String) -> R<()> {
    let c = s.db.conn();
    let mut out = serde_json::Map::new();
    out.insert("format".into(), json!("socialeye-export"));
    out.insert("schema_version".into(), json!(db::schema_version(&c)?));
    out.insert("exported_at".into(), json!(crate::app::now_rfc3339()));
    for t in EXPORT_TABLES {
        out.insert((*t).into(), Value::Array(query_json(&c, &format!("SELECT * FROM {t}"), [])?));
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&Value::Object(out))?)?;
    db::audit(&c, "data.export", "", "")
}

#[tauri::command]
pub fn search(s: State<'_, AppState>, q: String) -> R<Value> {
    let q = q.trim();
    if q.len() < 2 {
        return Ok(json!({"accounts": [], "content": [], "drafts": []}));
    }
    let p = like(q);
    let c = s.db.conn();
    Ok(json!({
        "accounts": query_json(&c, "SELECT id, platform, name, handle FROM accounts WHERE name LIKE ?1 ESCAPE '\\' OR handle LIKE ?1 ESCAPE '\\' LIMIT 10", [&p])?,
        "content": query_json(&c, "SELECT c.id, c.title, c.kind, c.url, a.platform FROM content_items c JOIN accounts a ON a.id = c.account_id WHERE c.title LIKE ?1 ESCAPE '\\' ORDER BY c.published_at DESC LIMIT 15", [&p])?,
        "drafts": query_json(&c, "SELECT id, title FROM drafts WHERE title LIKE ?1 ESCAPE '\\' OR body LIKE ?1 ESCAPE '\\' LIMIT 10", [&p])?,
    }))
}

#[tauri::command]
pub fn audit_log(s: State<'_, AppState>) -> R<Vec<Value>> {
    query_json(&s.db.conn(), "SELECT * FROM audit_log ORDER BY id DESC LIMIT 500", [])
}

// ------------------------------------------------------------------ updates

fn update_channel(s: &AppState) -> String {
    db::get_setting(&s.db.conn(), "update_channel").ok().flatten().unwrap_or_else(|| updates::DEFAULT_CHANNEL.into())
}

#[tauri::command]
pub fn updates_status(app: AppHandle, s: State<'_, AppState>) -> R<Value> {
    Ok(json!({
        "enabled": updates::enabled(),
        "current": app.package_info().version.to_string(),
        "channel": update_channel(&s),
        "repository": updates::REPOSITORY,
        "auto_check": db::get_setting(&s.db.conn(), "update_auto_check")?.as_deref() != Some("0"),
    }))
}

#[tauri::command]
pub async fn updates_check(app: AppHandle, s: State<'_, AppState>) -> R<Option<updates::Available>> {
    let channel = update_channel(&s);
    updates::check(&s.http, &channel, &app.package_info().version).await
}

/// Download, verify (signature + signed version) and run the installer, then restart.
#[tauri::command]
pub async fn updates_install(app: AppHandle, s: State<'_, AppState>, confirmed: bool) -> R<()> {
    use tauri_plugin_updater::UpdaterExt;
    require_confirmation(confirmed, "installing an update")?;
    let channel = update_channel(&s);
    let avail = updates::check(&s.http, &channel, &app.package_info().version).await?.ok_or_else(|| AppError::validation("no newer version is available"))?;
    let upd_err = |e: tauri_plugin_updater::Error| AppError::Api { status: 0, message: format!("update failed: {e}"), retryable: false };
    let url = url::Url::parse(&avail.manifest_url).map_err(|_| AppError::validation("invalid update URL"))?;
    let update = app
        .updater_builder()
        .endpoints(vec![url])
        .map_err(upd_err)?
        .build()
        .map_err(upd_err)?
        .check()
        .await
        .map_err(upd_err)?
        .ok_or_else(|| AppError::validation("the signed update manifest offers no newer version"))?;
    db::audit(&s.db.conn(), "update.install", &avail.tag, &channel)?;
    let ui = TauriUi(app.clone());
    let mut received = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                received += chunk as u64;
                ui.emit("update-progress", json!({"received": received, "total": total}));
            },
            || {},
        )
        .await
        .map_err(upd_err)?;
    app.restart()
}

/// Background check (startup and every 6 hours): notifies once per new version, never installs.
pub async fn auto_check_updates(app: &AppHandle) {
    let s = app.state::<AppState>();
    let on = db::get_setting(&s.db.conn(), "update_auto_check").ok().flatten().as_deref() != Some("0");
    if !on || !updates::enabled() {
        return;
    }
    let channel = update_channel(&s);
    let Ok(Some(a)) = updates::check(&s.http, &channel, &app.package_info().version).await else { return };
    let seen = db::get_setting(&s.db.conn(), "update_notified").ok().flatten();
    if seen.as_deref() == Some(a.version.as_str()) {
        return;
    }
    let title = format!("SocialEye {} is available", a.version);
    {
        let c = s.db.conn();
        let _ = c.execute("INSERT INTO notifications(kind, title, body) VALUES ('update', ?1, 'Settings → Updates')", [&title]);
        let _ = db::set_setting(&c, "update_notified", &a.version);
    }
    let ui = TauriUi(app.clone());
    ui.desktop_notify(&title, "Settings → Updates");
    ui.emit("notifications-changed", json!(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_escapes_wildcards() {
        assert_eq!(like("50%_off"), "%50\\%\\_off%");
    }

    #[test]
    fn media_kinds() {
        assert_eq!(media_kind(Path::new("a.MP4")), "video");
        assert_eq!(media_kind(Path::new("a.jpeg")), "image");
        assert_eq!(media_kind(Path::new("a.exe")), "other");
    }

    #[test]
    fn confirmation_is_required() {
        assert!(require_confirmation(false, "x").is_err());
        assert!(require_confirmation(true, "x").is_ok());
    }
}
