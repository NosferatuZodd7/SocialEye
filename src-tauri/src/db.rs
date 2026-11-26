//! SQLite storage: connection, versioned migrations, JSON row helpers, backup/restore.
use crate::error::{AppError, AppResult};
use rusqlite::{types::ValueRef, Connection, Params};
use serde_json::{Map, Value};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// Append-only. Never edit a shipped migration; add a new one.
const MIGRATIONS: &[&str] = &[
    // 1 — initial schema
    r#"
    CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);

    CREATE TABLE platform_apps (
        platform TEXT PRIMARY KEY,
        client_id TEXT NOT NULL,
        redirect_port INTEGER NOT NULL DEFAULT 53682,
        api_version TEXT,
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE accounts (
        id INTEGER PRIMARY KEY,
        platform TEXT NOT NULL CHECK (platform IN ('instagram','facebook','youtube','spotify')),
        account_type TEXT NOT NULL,
        external_id TEXT NOT NULL,
        name TEXT NOT NULL,
        handle TEXT,
        avatar_url TEXT,
        parent_account_id INTEGER REFERENCES accounts(id) ON DELETE CASCADE,
        status TEXT NOT NULL DEFAULT 'connected',
        scopes TEXT,
        token_expires_at INTEGER,
        auto_sync INTEGER NOT NULL DEFAULT 1,
        last_sync_at TEXT,
        last_sync_status TEXT,
        last_error TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE (platform, external_id)
    );

    CREATE TABLE content_items (
        id INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        external_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        title TEXT,
        description TEXT,
        url TEXT,
        thumbnail_url TEXT,
        published_at TEXT,
        duration_secs INTEGER,
        stats_json TEXT NOT NULL DEFAULT '{}',
        updated_at TEXT NOT NULL DEFAULT (datetime('now')),
        UNIQUE (account_id, external_id)
    );
    CREATE INDEX idx_content_account_pub ON content_items(account_id, published_at);

    CREATE TABLE metric_snapshots (
        id INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        content_id INTEGER REFERENCES content_items(id) ON DELETE CASCADE,
        metric TEXT NOT NULL,
        value REAL NOT NULL,
        day TEXT NOT NULL,
        source TEXT NOT NULL,
        captured_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    -- One value per account/content/metric/day; re-sync overwrites.
    CREATE UNIQUE INDEX idx_metric_unique ON metric_snapshots(account_id, IFNULL(content_id, 0), metric, day);
    CREATE INDEX idx_metric_lookup ON metric_snapshots(account_id, metric, day);

    -- Point-in-time dimensional data (traffic sources, demographics) for a period.
    CREATE TABLE breakdowns (
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        kind TEXT NOT NULL,
        period_start TEXT NOT NULL,
        period_end TEXT NOT NULL,
        data_json TEXT NOT NULL,
        source TEXT NOT NULL,
        captured_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (account_id, kind)
    );

    CREATE TABLE sync_runs (
        id INTEGER PRIMARY KEY,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        started_at TEXT NOT NULL DEFAULT (datetime('now')),
        finished_at TEXT,
        status TEXT NOT NULL DEFAULT 'running',
        items INTEGER NOT NULL DEFAULT 0,
        unavailable TEXT,
        error TEXT
    );
    CREATE INDEX idx_sync_account ON sync_runs(account_id, started_at);

    CREATE TABLE media_assets (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        kind TEXT NOT NULL CHECK (kind IN ('image','video','audio','other')),
        size_bytes INTEGER NOT NULL,
        public_url TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE drafts (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        category TEXT,
        tags TEXT NOT NULL DEFAULT '',
        hashtags TEXT NOT NULL DEFAULT '',
        media_asset_id INTEGER REFERENCES media_assets(id) ON DELETE SET NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        updated_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE draft_variants (
        id INTEGER PRIMARY KEY,
        draft_id INTEGER NOT NULL REFERENCES drafts(id) ON DELETE CASCADE,
        account_id INTEGER NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        title TEXT,
        body TEXT NOT NULL DEFAULT '',
        media_asset_id INTEGER REFERENCES media_assets(id) ON DELETE SET NULL,
        thumbnail_asset_id INTEGER REFERENCES media_assets(id) ON DELETE SET NULL,
        link TEXT,
        privacy TEXT,
        status TEXT NOT NULL DEFAULT 'draft'
            CHECK (status IN ('draft','scheduled','publishing','published','failed')),
        scheduled_at TEXT,
        published_at TEXT,
        external_id TEXT,
        error TEXT,
        UNIQUE (draft_id, account_id)
    );
    CREATE INDEX idx_variants_schedule ON draft_variants(status, scheduled_at);

    CREATE TABLE publish_history (
        id INTEGER PRIMARY KEY,
        variant_id INTEGER REFERENCES draft_variants(id) ON DELETE SET NULL,
        account_id INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
        action TEXT NOT NULL,
        status TEXT NOT NULL,
        message TEXT,
        external_id TEXT,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE templates (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        body TEXT NOT NULL,
        hashtags TEXT NOT NULL DEFAULT '',
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );

    CREATE TABLE automations (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        enabled INTEGER NOT NULL DEFAULT 1,
        rule_json TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        last_fired_at TEXT
    );

    CREATE TABLE automation_runs (
        id INTEGER PRIMARY KEY,
        automation_id INTEGER NOT NULL REFERENCES automations(id) ON DELETE CASCADE,
        ran_at TEXT NOT NULL DEFAULT (datetime('now')),
        status TEXT NOT NULL,
        message TEXT,
        dedupe_key TEXT
    );
    CREATE UNIQUE INDEX idx_automation_dedupe ON automation_runs(automation_id, dedupe_key);

    CREATE TABLE notifications (
        id INTEGER PRIMARY KEY,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        account_id INTEGER REFERENCES accounts(id) ON DELETE SET NULL,
        read INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL DEFAULT (datetime('now'))
    );
    CREATE INDEX idx_notifications_read ON notifications(read, created_at);

    CREATE TABLE ig_export_imports (
        id INTEGER PRIMARY KEY,
        label TEXT NOT NULL,
        imported_at TEXT NOT NULL DEFAULT (datetime('now')),
        followers_count INTEGER NOT NULL,
        following_count INTEGER NOT NULL,
        warnings TEXT
    );
    CREATE TABLE ig_export_entries (
        import_id INTEGER NOT NULL REFERENCES ig_export_imports(id) ON DELETE CASCADE,
        list TEXT NOT NULL CHECK (list IN ('followers','following')),
        username TEXT NOT NULL,
        followed_at INTEGER,
        PRIMARY KEY (import_id, list, username)
    );

    CREATE TABLE audit_log (
        id INTEGER PRIMARY KEY,
        at TEXT NOT NULL DEFAULT (datetime('now')),
        action TEXT NOT NULL,
        target TEXT,
        detail TEXT
    );
    "#,
];

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        Self::init(Connection::open(path)?)
    }
    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }
    fn init(conn: Connection) -> AppResult<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;")?;
        migrate(&conn)?;
        Ok(Db(Mutex::new(conn)))
    }
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves SQLite itself consistent; keep going.
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Consistent online backup to `dest` using SQLite's backup API.
    pub fn backup_to(&self, dest: &Path) -> AppResult<()> {
        let conn = self.conn();
        let mut out = Connection::open(dest)?;
        let b = rusqlite::backup::Backup::new(&conn, &mut out)?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
        Ok(())
    }

    /// Replace the live database with the contents of `src` (validated first).
    pub fn restore_from(&self, src: &Path) -> AppResult<()> {
        let incoming = Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let ok: String = incoming.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(AppError::validation("backup file failed integrity check"));
        }
        let has_accounts: i64 = incoming.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='accounts'",
            [],
            |r| r.get(0),
        )?;
        if has_accounts == 0 {
            return Err(AppError::validation("file is not a SocialEye backup"));
        }
        let mut conn = self.conn();
        let b = rusqlite::backup::Backup::new(&incoming, &mut conn)?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
        drop(b);
        migrate(&conn)?; // an older backup is upgraded in place
        Ok(())
    }
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current as usize > MIGRATIONS.len() {
        return Err(AppError::Database {
            message: "database was created by a newer SocialEye version".into(),
        });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        conn.execute_batch(&format!("BEGIN; {sql}; PRAGMA user_version = {}; COMMIT;", i + 1))?;
    }
    Ok(())
}

pub fn schema_version(conn: &Connection) -> AppResult<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

/// Run a query and return every row as a JSON object keyed by column name.
pub fn query_json<P: Params>(conn: &Connection, sql: &str, params: P) -> AppResult<Vec<Value>> {
    let mut stmt = conn.prepare(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let rows = stmt.query_map(params, |row| {
        let mut obj = Map::new();
        for (i, n) in names.iter().enumerate() {
            let v = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(x) => Value::from(x),
                ValueRef::Real(x) => Value::from(x),
                ValueRef::Text(t) => Value::from(String::from_utf8_lossy(t).into_owned()),
                ValueRef::Blob(_) => Value::Null,
            };
            obj.insert(n.clone(), v);
        }
        Ok(Value::Object(obj))
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn audit(conn: &Connection, action: &str, target: &str, detail: &str) -> AppResult<()> {
    conn.execute("INSERT INTO audit_log(action, target, detail) VALUES (?1, ?2, ?3)", (action, target, detail))?;
    Ok(())
}

pub fn get_setting(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    match conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )?;
    Ok(())
}

/// Upsert a daily metric value. Returns the previous value for the same day (if any).
pub fn put_metric(
    conn: &Connection,
    account_id: i64,
    content_id: Option<i64>,
    metric: &str,
    value: f64,
    day: &str,
    source: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO metric_snapshots(account_id, content_id, metric, value, day, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(account_id, IFNULL(content_id, 0), metric, day)
         DO UPDATE SET value = excluded.value, source = excluded.source, captured_at = datetime('now')",
        (account_id, content_id, metric, value, day, source),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(conn: &Connection) -> i64 {
        conn.execute(
            "INSERT INTO accounts(platform, account_type, external_id, name) VALUES ('youtube','channel','UC1','Ch')",
            [],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let db = Db::open_in_memory().unwrap();
        let c = db.conn();
        assert_eq!(schema_version(&c).unwrap(), MIGRATIONS.len() as i64);
        migrate(&c).unwrap();
        assert_eq!(schema_version(&c).unwrap(), MIGRATIONS.len() as i64);
    }

    #[test]
    fn metric_upsert_keeps_one_row_per_day() {
        let db = Db::open_in_memory().unwrap();
        let c = db.conn();
        let a = account(&c);
        put_metric(&c, a, None, "subscribers", 10.0, "2026-01-01", "test").unwrap();
        put_metric(&c, a, None, "subscribers", 12.0, "2026-01-01", "test").unwrap();
        put_metric(&c, a, None, "subscribers", 15.0, "2026-01-02", "test").unwrap();
        let rows = query_json(&c, "SELECT value FROM metric_snapshots ORDER BY day", []).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["value"], 12.0);
    }

    #[test]
    fn foreign_keys_cascade() {
        let db = Db::open_in_memory().unwrap();
        let c = db.conn();
        let a = account(&c);
        put_metric(&c, a, None, "views", 1.0, "2026-01-01", "test").unwrap();
        c.execute("DELETE FROM accounts WHERE id = ?1", [a]).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM metric_snapshots", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn backup_and_restore_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("live.db")).unwrap();
        set_setting(&db.conn(), "theme", "dark").unwrap();
        let bak = dir.path().join("backup.db");
        db.backup_to(&bak).unwrap();
        set_setting(&db.conn(), "theme", "light").unwrap();
        db.restore_from(&bak).unwrap();
        assert_eq!(get_setting(&db.conn(), "theme").unwrap().as_deref(), Some("dark"));
    }

    #[test]
    fn restore_rejects_foreign_database() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other.db");
        Connection::open(&other).unwrap().execute_batch("CREATE TABLE x(a);").unwrap();
        let db = Db::open_in_memory().unwrap();
        assert!(matches!(db.restore_from(&other), Err(AppError::Validation { .. })));
    }
}
