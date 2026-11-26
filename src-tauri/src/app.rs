//! Application core: shared state, token lifecycle, connect / sync / publish engines
//! and the background scheduler. Tauri commands (commands.rs) are thin wrappers.
use crate::automations::{self, Event, Fired};
use crate::connectors::{self, facebook, instagram, youtube, Ctx, Platform, PublishRequest, SyncData};
use crate::db::{self, Db};
use crate::error::{AppError, AppResult};
use crate::oauth;
use crate::secrets::{self, SecretStore, TokenSet};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct AppState {
    pub db: Db,
    pub secrets: Box<dyn SecretStore>,
    pub http: reqwest::Client,
    pub data_dir: PathBuf,
    syncing: Mutex<HashSet<i64>>,
}

/// Something that can surface notifications/progress (the Tauri app, or nothing in tests).
pub trait Ui: Send + Sync {
    fn open_url(&self, url: &str) -> AppResult<()>;
    fn desktop_notify(&self, title: &str, body: &str);
    fn emit(&self, event: &str, payload: Value);
}

pub struct NoUi;
impl Ui for NoUi {
    fn open_url(&self, _: &str) -> AppResult<()> {
        Err(AppError::validation("no browser available"))
    }
    fn desktop_notify(&self, _: &str, _: &str) {}
    fn emit(&self, _: &str, _: Value) {}
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: i64,
    pub platform: Platform,
    pub account_type: String,
    pub external_id: String,
    pub name: String,
    pub parent_account_id: Option<i64>,
    pub had_synced: bool,
}

pub fn load_account(c: &Connection, id: i64) -> AppResult<Account> {
    let (platform, account_type, external_id, name, parent_account_id, last_sync): (String, String, String, String, Option<i64>, Option<String>) = c.query_row(
        "SELECT platform, account_type, external_id, name, parent_account_id, last_sync_at FROM accounts WHERE id = ?1",
        [id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
    )?;
    Ok(Account { id, platform: Platform::parse(&platform)?, account_type, external_id, name, parent_account_id, had_synced: last_sync.is_some() })
}

pub struct ProviderApp {
    pub client_id: String,
    pub port: u16,
    pub api_version: Option<String>,
}

pub fn provider_app(c: &Connection, provider: &str) -> AppResult<ProviderApp> {
    c.query_row("SELECT client_id, redirect_port, api_version FROM platform_apps WHERE platform = ?1", [provider], |r| {
        Ok(ProviderApp { client_id: r.get(0)?, port: r.get::<_, i64>(1)? as u16, api_version: r.get(2)? })
    })
    .optional()?
    .filter(|p| !p.client_id.trim().is_empty())
    .ok_or_else(|| AppError::NotConfigured {
        platform: provider.into(),
        message: "add your developer app's Client ID in Settings → Platform apps (see docs/API_SETUP.md)".into(),
    })
}

pub fn graph_version(c: &Connection) -> String {
    provider_app(c, "meta").ok().and_then(|p| p.api_version).filter(|v| !v.is_empty()).unwrap_or_else(|| facebook::DEFAULT_GRAPH_VERSION.into())
}

pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// Normalise any RFC 3339 timestamp to UTC `YYYY-MM-DDTHH:MM:SSZ` so SQL string comparison is valid.
pub fn normalize_time(s: &str) -> AppResult<String> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .map_err(|_| AppError::validation("invalid date/time"))
}

struct SyncGuard<'a>(&'a Mutex<HashSet<i64>>, i64);
impl Drop for SyncGuard<'_> {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(&self.1);
    }
}

/// A local Instagram schedule more than this late is not published automatically.
const MISSED_WINDOW_MINUTES: i64 = 15;

impl AppState {
    pub fn new(db: Db, secrets: Box<dyn SecretStore>, data_dir: PathBuf) -> AppResult<Self> {
        automations::seed_defaults(&db.conn())?;
        Ok(AppState { db, secrets, http: crate::http::client(), data_dir, syncing: Mutex::new(HashSet::new()) })
    }

    fn client_secret(&self, provider: &str) -> AppResult<Option<String>> {
        self.secrets.get(&secrets::client_secret_key(provider))
    }

    /// Valid access token for an account, refreshing it when it is about to expire.
    pub async fn access_token(&self, acc: &Account) -> AppResult<String> {
        let owner = acc.parent_account_id.unwrap_or(acc.id);
        let reconnect = || AppError::Auth { message: format!("{} must be reconnected", acc.name), reconnect_required: true };
        let t = secrets::load_token(self.secrets.as_ref(), owner)?.ok_or_else(reconnect)?;
        let now = chrono::Utc::now().timestamp();
        let Some(exp) = t.expires_at else { return Ok(t.access_token) };
        if exp > now + 120 {
            return Ok(t.access_token);
        }
        let Some(rt) = t.refresh_token.clone() else {
            return if exp > now { Ok(t.access_token) } else { Err(reconnect()) };
        };
        let provider = acc.platform.provider();
        let app = provider_app(&self.db.conn(), provider)?;
        let secret = self.client_secret(provider)?;
        let fresh = oauth::refresh(&self.http, acc.platform.oauth(), &app.client_id, secret.as_deref(), &rt).await?;
        secrets::save_token(self.secrets.as_ref(), owner, &fresh)?;
        self.db.conn().execute("UPDATE accounts SET token_expires_at = ?1 WHERE id = ?2 OR parent_account_id = ?2", (fresh.expires_at, owner))?;
        Ok(fresh.access_token)
    }

    /// Full OAuth connection: browser consent → token exchange → account discovery.
    pub async fn connect(&self, ui: &dyn Ui, platform: Platform) -> AppResult<Vec<i64>> {
        let provider = platform.provider();
        let (app, gv) = {
            let c = self.db.conn();
            (provider_app(&c, provider)?, graph_version(&c))
        };
        let secret = self.client_secret(provider)?;
        let pk = oauth::pkce();
        let listener = oauth::bind(app.port).await?;
        let url = oauth::authorize_url(platform.oauth(), &app.client_id, app.port, platform.scopes(), &pk);
        ui.open_url(&url)?;
        let code = oauth::wait_for_code(listener, &pk.state).await?;
        let mut tokens = oauth::exchange_code(&self.http, platform.oauth(), &app.client_id, secret.as_deref(), app.port, &code, &pk.verifier).await?;

        let mut long_lived = false;
        if provider == "meta" {
            if let Some(s) = &secret {
                // Long-lived user token (~60 days) → Page tokens that do not expire.
                let v = crate::http::send(self.http.post(format!("https://graph.facebook.com/{gv}/oauth/access_token")).form(&[
                    ("grant_type", "fb_exchange_token"),
                    ("client_id", app.client_id.as_str()),
                    ("client_secret", s.as_str()),
                    ("fb_exchange_token", tokens.access_token.as_str()),
                ]))
                .await?;
                tokens = oauth::parse_token_response(&v, chrono::Utc::now().timestamp())?;
                long_lived = true;
            }
        }
        let ctx = Ctx { http: &self.http, token: &tokens.access_token, graph_version: &gv };
        let found = connectors::discover(platform, &ctx).await?;
        let scopes = platform.scopes().join(" ");
        let mut ids = vec![];
        for d in found {
            let t = match &d.token_override {
                Some(page_token) => TokenSet { access_token: page_token.clone(), refresh_token: None, expires_at: if long_lived { None } else { tokens.expires_at } },
                None => tokens.clone(),
            };
            let id = self.upsert_account(platform, &d, None, &scopes, t.expires_at)?;
            secrets::save_token(self.secrets.as_ref(), id, &t)?;
            ids.push(id);
        }
        db::audit(&self.db.conn(), "account.connect", platform.as_str(), &format!("{} account(s)", ids.len()))?;
        Ok(ids)
    }

    pub fn upsert_account(&self, p: Platform, d: &connectors::DiscoveredAccount, parent: Option<i64>, scopes: &str, expires: Option<i64>) -> AppResult<i64> {
        let c = self.db.conn();
        Ok(c.query_row(
            "INSERT INTO accounts(platform, account_type, external_id, name, handle, avatar_url, parent_account_id, scopes, token_expires_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'connected')
             ON CONFLICT(platform, external_id) DO UPDATE SET name = excluded.name, handle = excluded.handle,
               avatar_url = excluded.avatar_url, scopes = excluded.scopes, token_expires_at = excluded.token_expires_at,
               parent_account_id = excluded.parent_account_id, status = 'connected', last_error = NULL
             RETURNING id",
            (p.as_str(), &d.account_type, &d.external_id, &d.name, &d.handle, &d.avatar_url, parent, scopes, expires),
            |r| r.get(0),
        )?)
    }

    /// Track a Spotify podcast under a connected Spotify user account.
    pub async fn add_podcast(&self, parent_id: i64, show: &str) -> AppResult<i64> {
        let id = connectors::spotify::parse_show_id(show)?;
        let parent = load_account(&self.db.conn(), parent_id)?;
        if parent.platform != Platform::Spotify || parent.account_type != "user" {
            return Err(AppError::validation("podcasts are added under a connected Spotify account"));
        }
        let token = self.access_token(&parent).await?;
        let ctx = Ctx { http: &self.http, token: &token, graph_version: "" };
        let s = connectors::spotify::show(&ctx, &id).await?;
        let d = connectors::spotify::parse_show(&s).ok_or_else(|| AppError::NotFound { message: "podcast not found".into() })?;
        let new_id = self.upsert_account(Platform::Spotify, &d, Some(parent_id), "", None)?;
        db::audit(&self.db.conn(), "podcast.add", &id, "")?;
        Ok(new_id)
    }

    pub fn disconnect(&self, id: i64) -> AppResult<()> {
        let c = self.db.conn();
        let children: Vec<i64> = c.prepare("SELECT id FROM accounts WHERE parent_account_id = ?1")?.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        for a in children.iter().chain([id].iter()) {
            self.secrets.delete(&secrets::token_key(*a))?;
        }
        c.execute("UPDATE accounts SET status = 'disconnected', token_expires_at = NULL WHERE id = ?1 OR parent_account_id = ?1", [id])?;
        db::audit(&c, "account.disconnect", &id.to_string(), "token removed from credential store; history kept")?;
        Ok(())
    }

    pub fn remove_account(&self, id: i64) -> AppResult<()> {
        self.disconnect(id)?;
        let c = self.db.conn();
        c.execute("DELETE FROM accounts WHERE id = ?1", [id])?;
        db::audit(&c, "account.remove", &id.to_string(), "account and its history deleted")?;
        Ok(())
    }

    fn deliver(&self, ui: &dyn Ui, fired: Vec<Fired>) {
        let desktop_on = db::get_setting(&self.db.conn(), "desktop_notifications").ok().flatten().as_deref() != Some("0");
        for f in &fired {
            if f.desktop && desktop_on {
                ui.desktop_notify(&f.title, &f.body);
            }
        }
        if !fired.is_empty() {
            ui.emit("notifications-changed", json!(fired.len()));
        }
    }

    pub fn run_automations(&self, ui: &dyn Ui, account_id: i64, events: &[Event]) {
        let r = automations::evaluate(&self.db.conn(), account_id, events);
        match r {
            Ok(f) => self.deliver(ui, f),
            Err(e) => log::error!("automation evaluation failed: {e}"),
        }
    }

    pub async fn sync_account(&self, ui: &dyn Ui, id: i64) -> AppResult<Value> {
        if !self.syncing.lock().unwrap_or_else(|p| p.into_inner()).insert(id) {
            return Err(AppError::validation("this account is already synchronizing"));
        }
        let _guard = SyncGuard(&self.syncing, id);
        let (acc, gv, run_id) = {
            let c = self.db.conn();
            let acc = load_account(&c, id)?;
            c.execute("INSERT INTO sync_runs(account_id) VALUES (?1)", [id])?;
            (acc, graph_version(&c), c.last_insert_rowid())
        };
        ui.emit("sync-state", json!({"account_id": id, "running": true}));
        let result = async {
            let token = self.access_token(&acc).await?;
            let ctx = Ctx { http: &self.http, token: &token, graph_version: &gv };
            connectors::sync(acc.platform, &ctx, &acc.account_type, &acc.external_id).await
        }
        .await;
        let outcome = match result {
            Ok(data) => {
                let r = persist_sync(&mut self.db.conn(), &acc, run_id, data);
                match r {
                    Ok((summary, events)) => {
                        self.run_automations(ui, id, &events);
                        Ok(summary)
                    }
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        };
        if let Err(e) = &outcome {
            let msg = e.to_string();
            let reconnect = matches!(e, AppError::Auth { reconnect_required: true, .. });
            {
                let c = self.db.conn();
                c.execute("UPDATE sync_runs SET finished_at = datetime('now'), status = 'failed', error = ?1 WHERE id = ?2", (&msg, run_id))?;
                c.execute(
                    "UPDATE accounts SET last_sync_at = datetime('now'), last_sync_status = 'failed', last_error = ?1, status = CASE WHEN ?2 THEN 'reconnect_required' ELSE status END WHERE id = ?3",
                    (&msg, reconnect, id),
                )?;
            }
            let mut ev = vec![Event::SyncFailed { run_id, error: msg.clone() }];
            if reconnect {
                ev.push(Event::ReconnectRequired { error: msg });
            }
            self.run_automations(ui, id, &ev);
        }
        ui.emit("sync-state", json!({"account_id": id, "running": false}));
        outcome
    }

    /// Build the request for a variant (title/body fall back to the draft).
    pub fn publish_request(&self, variant_id: i64) -> AppResult<(Account, PublishRequest, Option<String>)> {
        let c = self.db.conn();
        let v = db::query_json(
            &c,
            "SELECT v.*, d.title AS d_title, d.body AS d_body, d.hashtags, d.tags, d.media_asset_id AS d_media,
                    m.path AS m_path, m.kind AS m_kind, m.public_url AS m_url, t.path AS t_path
             FROM draft_variants v JOIN drafts d ON d.id = v.draft_id
             LEFT JOIN media_assets m ON m.id = COALESCE(v.media_asset_id, d.media_asset_id)
             LEFT JOIN media_assets t ON t.id = v.thumbnail_asset_id
             WHERE v.id = ?1",
            [variant_id],
        )?
        .pop()
        .ok_or_else(|| AppError::NotFound { message: "variant not found".into() })?;
        let s = |k: &str| v[k].as_str().map(String::from).filter(|x| !x.is_empty());
        let acc = load_account(&c, v["account_id"].as_i64().unwrap_or_default())?;
        let mut body = s("body").or_else(|| s("d_body")).unwrap_or_default();
        if let Some(h) = s("hashtags") {
            if !body.contains(&h) {
                body = format!("{body}\n\n{h}").trim().to_string();
            }
        }
        let req = PublishRequest {
            title: s("title").or_else(|| s("d_title")),
            body,
            link: s("link"),
            privacy: s("privacy"),
            media_path: s("m_path"),
            media_kind: s("m_kind"),
            media_public_url: s("m_url"),
            thumbnail_path: s("t_path"),
            tags: s("tags").map(|t| t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default(),
            scheduled_at: s("scheduled_at"),
        };
        Ok((acc, req, s("status")))
    }

    pub fn validate_variant(&self, variant_id: i64) -> AppResult<Vec<String>> {
        let (acc, req, _) = self.publish_request(variant_id)?;
        let mut errs = connectors::validate_publish(acc.platform, &req, chrono::Utc::now());
        if let Some(p) = &req.media_path {
            if !std::path::Path::new(p).is_file() {
                errs.push(format!("media file not found: {p}"));
            }
        }
        Ok(errs)
    }

    /// Publish (or schedule) a variant. `now` forces immediate publishing (used by the
    /// local scheduler when an Instagram schedule falls due).
    pub async fn publish_variant(&self, ui: &dyn Ui, variant_id: i64, now: bool) -> AppResult<Value> {
        let (acc, mut req, status) = self.publish_request(variant_id)?;
        if matches!(status.as_deref(), Some("published" | "publishing")) {
            return Err(AppError::validation("this variant was already published"));
        }
        if now {
            req.scheduled_at = None;
        }
        let errs = connectors::validate_publish(acc.platform, &req, chrono::Utc::now());
        if !errs.is_empty() {
            return Err(AppError::validation(errs.join("; ")));
        }
        if acc.platform == Platform::Instagram && req.scheduled_at.is_some() {
            // No native IG scheduling: SocialEye's scheduler publishes when due.
            let c = self.db.conn();
            c.execute("UPDATE draft_variants SET status = 'scheduled', error = NULL WHERE id = ?1", [variant_id])?;
            c.execute("INSERT INTO publish_history(variant_id, account_id, action, status, message) VALUES (?1, ?2, 'schedule', 'ok', 'scheduled locally — SocialEye must be running at that time')", (variant_id, acc.id))?;
            return Ok(json!({"status": "scheduled", "local": true}));
        }
        self.db.conn().execute("UPDATE draft_variants SET status = 'publishing', error = NULL WHERE id = ?1", [variant_id])?;
        let gv = graph_version(&self.db.conn());
        let result = async {
            let token = self.access_token(&acc).await?;
            let ctx = Ctx { http: &self.http, token: &token, graph_version: &gv };
            match acc.platform {
                Platform::Youtube => {
                    let progress = |sent: u64, total: u64| ui.emit("upload-progress", json!({"variant_id": variant_id, "sent": sent, "total": total}));
                    let r = youtube::upload(&ctx, &req, &progress).await?;
                    if let Some(t) = &req.thumbnail_path {
                        if let Err(e) = youtube::set_thumbnail(&ctx, &r.external_id, t).await {
                            // The video exists; report the thumbnail problem without failing the publish.
                            self.db.conn().execute("INSERT INTO publish_history(variant_id, account_id, action, status, message, external_id) VALUES (?1, ?2, 'thumbnail', 'failed', ?3, ?4)", (variant_id, acc.id, e.to_string(), &r.external_id))?;
                        }
                    }
                    Ok(r)
                }
                Platform::Facebook => facebook::publish(&ctx, &acc.external_id, &req).await,
                Platform::Instagram => instagram::publish(&ctx, &acc.external_id, &req).await,
                Platform::Spotify => Err(connectors::unsupported(Platform::Spotify, "publish")),
            }
        }
        .await;
        let c = self.db.conn();
        match result {
            Ok(r) => {
                let st = if r.scheduled { "scheduled" } else { "published" };
                c.execute(
                    "UPDATE draft_variants SET status = ?1, external_id = ?2, published_at = CASE WHEN ?1 = 'published' THEN ?3 ELSE NULL END WHERE id = ?4",
                    (st, &r.external_id, now_rfc3339(), variant_id),
                )?;
                c.execute("INSERT INTO publish_history(variant_id, account_id, action, status, message, external_id) VALUES (?1, ?2, ?3, 'ok', ?4, ?5)", (variant_id, acc.id, if r.scheduled { "schedule" } else { "publish" }, r.url.clone().unwrap_or_default(), &r.external_id))?;
                db::audit(&c, "content.publish", acc.platform.as_str(), &r.external_id)?;
                Ok(json!({"status": st, "external_id": r.external_id, "url": r.url}))
            }
            Err(e) => {
                let msg = e.to_string();
                c.execute("UPDATE draft_variants SET status = 'failed', error = ?1 WHERE id = ?2", (&msg, variant_id))?;
                c.execute("INSERT INTO publish_history(variant_id, account_id, action, status, message) VALUES (?1, ?2, 'publish', 'failed', ?3)", (variant_id, acc.id, &msg))?;
                drop(c);
                self.run_automations(ui, acc.id, &[Event::PublishFailed { variant_id, error: msg }]);
                Err(e)
            }
        }
    }

    /// One scheduler pass: due local schedules, automatic sync, expiry checks.
    pub async fn tick(&self, ui: &dyn Ui) {
        let now = now_rfc3339();
        let missed = (chrono::Utc::now() - chrono::Duration::minutes(MISSED_WINDOW_MINUTES)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let due: Vec<(i64, String)> = {
            let c = self.db.conn();
            let r = c
                .prepare("SELECT id, scheduled_at FROM draft_variants WHERE status = 'scheduled' AND external_id IS NULL AND scheduled_at <= ?1")
                .and_then(|mut s| s.query_map([&now], |r| Ok((r.get(0)?, r.get(1)?)))?.collect());
            r.unwrap_or_default()
        };
        for (vid, at) in due {
            if at < missed {
                let msg = "missed schedule: SocialEye was not running at the scheduled time. Review and publish manually.";
                let acc = {
                    let c = self.db.conn();
                    let _ = c.execute("UPDATE draft_variants SET status = 'failed', error = ?1 WHERE id = ?2", (msg, vid));
                    c.query_row("SELECT account_id FROM draft_variants WHERE id = ?1", [vid], |r| r.get::<_, i64>(0)).ok()
                };
                if let Some(a) = acc {
                    self.run_automations(ui, a, &[Event::PublishFailed { variant_id: vid, error: msg.into() }]);
                }
            } else if let Err(e) = self.publish_variant(ui, vid, true).await {
                log::warn!("scheduled publish {vid} failed: {e}");
            }
        }

        let interval: i64 = db::get_setting(&self.db.conn(), "sync_interval_minutes").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(360);
        if interval > 0 {
            let ids: Vec<i64> = {
                let c = self.db.conn();
                let r = c
                    .prepare("SELECT id FROM accounts WHERE auto_sync = 1 AND status = 'connected' AND (last_sync_at IS NULL OR last_sync_at <= datetime('now', ?1))")
                    .and_then(|mut s| s.query_map([format!("-{interval} minutes")], |r| r.get(0))?.collect());
                r.unwrap_or_default()
            };
            for id in ids {
                if let Err(e) = self.sync_account(ui, id).await {
                    log::warn!("auto-sync {id} failed: {e}");
                }
            }
        }
        let expiring: Vec<i64> = {
            let c = self.db.conn();
            let r = c.prepare("SELECT id FROM accounts WHERE token_expires_at IS NOT NULL AND status = 'connected'").and_then(|mut s| s.query_map([], |r| r.get(0))?.collect());
            r.unwrap_or_default()
        };
        for id in expiring {
            self.run_automations(ui, id, &[]);
        }
    }
}

/// Write a successful sync in one transaction and derive events for automations.
pub fn persist_sync(conn: &mut Connection, acc: &Account, run_id: i64, d: SyncData) -> AppResult<(Value, Vec<Event>)> {
    let tx = conn.transaction()?;
    let today = chrono::Utc::now().date_naive().to_string();
    let mut events = vec![];
    if let Some(p) = &d.profile {
        tx.execute("UPDATE accounts SET name = ?1, handle = COALESCE(?2, handle), avatar_url = COALESCE(?3, avatar_url) WHERE id = ?4", (&p.name, &p.handle, &p.avatar_url, acc.id))?;
    }
    for row in &d.content {
        if row.external_id.is_empty() {
            continue;
        }
        let prev: Option<(i64, String)> = tx
            .query_row("SELECT id, stats_json FROM content_items WHERE account_id = ?1 AND external_id = ?2", (acc.id, &row.external_id), |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let stats = Value::Object(row.stats.clone());
        let cid: i64 = tx.query_row(
            "INSERT INTO content_items(account_id, external_id, kind, title, description, url, thumbnail_url, published_at, duration_secs, stats_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, datetime('now'))
             ON CONFLICT(account_id, external_id) DO UPDATE SET kind = excluded.kind, title = excluded.title, description = excluded.description,
               url = excluded.url, thumbnail_url = excluded.thumbnail_url, published_at = excluded.published_at,
               duration_secs = excluded.duration_secs, stats_json = excluded.stats_json, updated_at = datetime('now')
             RETURNING id",
            (acc.id, &row.external_id, &row.kind, &row.title, &row.description, &row.url, &row.thumbnail_url, &row.published_at, row.duration_secs, stats.to_string()),
            |r| r.get(0),
        )?;
        let title = row.title.clone().unwrap_or_else(|| row.kind.clone());
        match prev {
            None if acc.had_synced => events.push(Event::NewContent { content_id: cid, title: title.clone() }),
            Some((_, old)) => {
                let old: Value = serde_json::from_str(&old).unwrap_or(Value::Null);
                let (o, n) = (old["comments"].as_f64().unwrap_or(0.0), row.stats.get("comments").and_then(Value::as_f64).unwrap_or(0.0));
                if n > o {
                    events.push(Event::NewComments { content_id: cid, title: title.clone(), delta: (n - o) as i64, total: n as i64 });
                }
            }
            None => {}
        }
        // Per-content daily history for numeric counters.
        for (k, v) in &row.stats {
            if let (Some(x), true) = (v.as_f64(), ["views", "likes", "comments", "reactions", "shares"].contains(&k.as_str())) {
                db::put_metric(&tx, acc.id, Some(cid), k, x, &today, d.source)?;
            }
        }
    }
    for m in &d.metrics {
        let cid: Option<i64> = match &m.content_external_id {
            Some(ext) => tx.query_row("SELECT id FROM content_items WHERE account_id = ?1 AND external_id = ?2", (acc.id, ext), |r| r.get(0)).optional()?,
            None => None,
        };
        db::put_metric(&tx, acc.id, cid, &m.metric, m.value, &m.day, d.source)?;
    }
    for b in &d.breakdowns {
        tx.execute(
            "INSERT INTO breakdowns(account_id, kind, period_start, period_end, data_json, source) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(account_id, kind) DO UPDATE SET period_start = excluded.period_start, period_end = excluded.period_end,
               data_json = excluded.data_json, source = excluded.source, captured_at = datetime('now')",
            (acc.id, &b.kind, &b.period_start, &b.period_end, b.data.to_string(), d.source),
        )?;
    }
    // Natively scheduled posts that now appear as published content.
    tx.execute(
        "UPDATE draft_variants SET status = 'published', published_at = COALESCE(published_at, ?2)
         WHERE account_id = ?1 AND status = 'scheduled' AND external_id IN (
           SELECT external_id FROM content_items WHERE account_id = ?1
           AND IFNULL(json_extract(stats_json, '$.privacy'), 'public') = 'public')",
        (acc.id, now_rfc3339()),
    )?;
    let unavailable = d.unavailable.join("\n");
    tx.execute("UPDATE sync_runs SET finished_at = datetime('now'), status = 'ok', items = ?1, unavailable = ?2 WHERE id = ?3", (d.content.len() as i64, &unavailable, run_id))?;
    tx.execute("UPDATE accounts SET last_sync_at = datetime('now'), last_sync_status = 'ok', last_error = NULL, status = 'connected' WHERE id = ?1", [acc.id])?;
    tx.commit()?;
    Ok((json!({"content": d.content.len(), "metrics": d.metrics.len(), "unavailable": d.unavailable}), events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::{ContentRow, DiscoveredAccount};
    use crate::secrets::MemoryStore;

    fn state() -> AppState {
        AppState::new(Db::open_in_memory().unwrap(), Box::new(MemoryStore::default()), std::env::temp_dir()).unwrap()
    }

    fn yt(s: &AppState) -> i64 {
        let d = DiscoveredAccount { account_type: "channel".into(), external_id: "UC1".into(), name: "Chan".into(), handle: None, avatar_url: None, token_override: None };
        s.upsert_account(Platform::Youtube, &d, None, "", None).unwrap()
    }

    fn video(id: &str, comments: f64) -> ContentRow {
        let mut stats = serde_json::Map::new();
        stats.insert("views".into(), json!(10.0));
        stats.insert("comments".into(), json!(comments));
        ContentRow { external_id: id.into(), kind: "video".into(), title: Some(id.into()), stats, ..Default::default() }
    }

    #[test]
    fn persist_detects_new_content_and_comments_only_after_first_sync() {
        let s = state();
        let id = yt(&s);
        let run = |s: &AppState, rows: Vec<ContentRow>| {
            let acc = load_account(&s.db.conn(), id).unwrap();
            s.db.conn().execute("INSERT INTO sync_runs(account_id) VALUES (?1)", [id]).unwrap();
            let rid = s.db.conn().last_insert_rowid();
            persist_sync(&mut s.db.conn(), &acc, rid, SyncData { content: rows, source: "test", ..Default::default() }).unwrap().1
        };
        assert!(run(&s, vec![video("a", 1.0)]).is_empty(), "first sync must not flood 'new content'");
        let ev = run(&s, vec![video("a", 3.0), video("b", 0.0)]);
        assert!(ev.iter().any(|e| matches!(e, Event::NewContent { title, .. } if title == "b")));
        assert!(ev.iter().any(|e| matches!(e, Event::NewComments { delta: 2, .. })));
        let n: i64 = s.db.conn().query_row("SELECT count(*) FROM metric_snapshots WHERE content_id IS NOT NULL AND metric = 'views'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn token_without_refresh_and_expired_requires_reconnect() {
        let s = state();
        let id = yt(&s);
        secrets::save_token(s.secrets.as_ref(), id, &TokenSet { access_token: "x".into(), refresh_token: None, expires_at: Some(1) }).unwrap();
        let acc = load_account(&s.db.conn(), id).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert!(matches!(rt.block_on(s.access_token(&acc)), Err(AppError::Auth { reconnect_required: true, .. })));
        secrets::save_token(s.secrets.as_ref(), id, &TokenSet { access_token: "ok".into(), refresh_token: None, expires_at: None }).unwrap();
        assert_eq!(rt.block_on(s.access_token(&acc)).unwrap(), "ok");
    }

    #[test]
    fn sync_failure_marks_reconnect_and_notifies() {
        let s = state();
        let id = yt(&s);
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        assert!(rt.block_on(s.sync_account(&NoUi, id)).is_err()); // no token stored
        let c = s.db.conn();
        let (status, err): (String, String) = c.query_row("SELECT status, last_error FROM accounts WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        assert_eq!(status, "reconnect_required");
        assert!(err.contains("reconnected"));
        let n: i64 = c.query_row("SELECT count(*) FROM notifications", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "sync failed + reconnect required");
    }

    #[test]
    fn disconnect_removes_tokens_keeps_history() {
        let s = state();
        let id = yt(&s);
        secrets::save_token(s.secrets.as_ref(), id, &TokenSet { access_token: "x".into(), refresh_token: None, expires_at: None }).unwrap();
        s.disconnect(id).unwrap();
        assert!(secrets::load_token(s.secrets.as_ref(), id).unwrap().is_none());
        assert_eq!(load_account(&s.db.conn(), id).unwrap().name, "Chan");
    }

    #[test]
    fn publish_validation_and_local_instagram_schedule() {
        let s = state();
        let d = DiscoveredAccount { account_type: "professional".into(), external_id: "ig1".into(), name: "IG".into(), handle: None, avatar_url: None, token_override: None };
        let ig = s.upsert_account(Platform::Instagram, &d, None, "", None).unwrap();
        let vid = {
            let c = s.db.conn();
            c.execute("INSERT INTO media_assets(path, name, kind, size_bytes, public_url) VALUES ('/x.jpg','x.jpg','image',1,'https://cdn.example/x.jpg')", []).unwrap();
            let m = c.last_insert_rowid();
            c.execute("INSERT INTO drafts(title, body, media_asset_id) VALUES ('T', 'caption', ?1)", [m]).unwrap();
            let dr = c.last_insert_rowid();
            c.execute("INSERT INTO draft_variants(draft_id, account_id, scheduled_at) VALUES (?1, ?2, '2099-01-01T00:00:00Z')", (dr, ig)).unwrap();
            c.last_insert_rowid()
        };
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let r = rt.block_on(s.publish_variant(&NoUi, vid, false)).unwrap();
        assert_eq!(r["local"], true);
        // A due-but-missed local schedule is failed, never silently published late.
        s.db.conn().execute("UPDATE draft_variants SET scheduled_at = '2000-01-01T00:00:00Z' WHERE id = ?1", [vid]).unwrap();
        rt.block_on(s.tick(&NoUi));
        let st: String = s.db.conn().query_row("SELECT status FROM draft_variants WHERE id = ?1", [vid], |r| r.get(0)).unwrap();
        assert_eq!(st, "failed");
    }

    #[test]
    fn time_normalisation() {
        assert_eq!(normalize_time("2026-01-01T10:00:00+01:00").unwrap(), "2026-01-01T09:00:00Z");
        assert!(normalize_time("tomorrow").is_err());
    }
}
