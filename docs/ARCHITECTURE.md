# Architecture

```
┌──────────────────────── React + TypeScript (src/) ────────────────────────┐
│ pages/*  ── lib/api.ts (typed invoke wrappers) ── lib/i18n, format, ...    │
└───────────────────────────────┬────────────────────────────────────────────┘
                                │ Tauri IPC (commands only; no tokens cross)
┌───────────────────────────────▼──────── Rust (src-tauri/src/) ─────────────┐
│ commands.rs   validation, confirmation checks, thin wrappers               │
│ app.rs        AppState, token refresh, connect / sync / publish, scheduler │
│ connectors/   youtube.rs facebook.rs instagram.rs spotify.rs  (independent)│
│ oauth.rs      PKCE + loopback redirect     http.rs  retries, backoff,      │
│ secrets.rs    Windows Credential Manager                error classes      │
│ db.rs         SQLite, migrations, backup   automations.rs  rule engine     │
│ ig_export.rs  csv_import.rs  reports.rs (CSV/XLSX)  updates.rs (self-update)│
└────────────────────────────────────────────────────────────────────────────┘
```

## Key decisions

- **Local-first.** All data lives in `%APPDATA%\io.github.socialeye\socialeye.db`. The app contacts only the platform APIs you connect.
- **Tokens never reach the WebView.** OAuth, token refresh and every API call run in Rust. Tokens and client secrets are stored with `keyring` (Windows Credential Manager). Request URLs never contain tokens (Bearer header), and `reqwest` errors are stripped of URLs before display.
- **Independent connectors.** Each platform module only depends on the shared `Ctx` (HTTP client + token). A platform API change is contained in one file.
- **Structured unsupported results.** `AppError::Unsupported { capability, reason, manual_alternative }` is built from the capability matrix (`connectors::capabilities`) and shown verbatim in the UI.
- **Soft failures for optional metrics.** `SyncData::soft` records a failing optional metric as *unavailable* and keeps syncing; authorization, rate-limit and network errors still abort.
- **No fabricated data.** Daily metrics are upserted one row per account/content/metric/day; missing days stay missing in charts.
- **Explicit consent.** Publishing, deleting comments, removing accounts, deleting drafts and restoring backups require `confirmed: true` from the UI's confirmation dialog; the backend rejects the call otherwise.
- **Automations only notify.** The rule engine has no destructive actions by design.
- **Self-update from GitHub Releases** with Testing/Stable channels, signature and signed-version verification, no downgrades and explicit consent — see [UPDATES.md](UPDATES.md).

## Synchronization

`AppState::sync_account` → refresh token if it expires within 2 minutes → `connectors::sync` → `persist_sync` (one transaction) → automation events. A per-account guard prevents concurrent syncs. The scheduler (`AppState::tick`, every 60 s) publishes due local schedules, syncs accounts older than the configured interval (default 6 h) and checks token expiry.

Each platform call goes through `http::send`: up to 4 attempts with exponential backoff and jitter for 5xx/network errors, `Retry-After` for 429, Meta throttling codes (4, 17, 32, 613, 80001–80014) treated as rate limits, Meta code 190 / HTTP 401 / `invalid_grant` treated as *reconnect required*, Google `quotaExceeded` as non-retryable.

Pagination: YouTube uploads (playlistItems, 50/page, max 250), Graph `paging.next` for Pages, Spotify episode offsets (max 500).

API versions: Google and Spotify endpoints are versioned in the URL (v3 / v2 / v1). The Meta Graph API version is a setting (default `v24.0`).

## Database

SQLite with `foreign_keys = ON`, WAL journal. Migrations are an append-only array in `db.rs`; `PRAGMA user_version` records the applied version. Restoring an older backup upgrades it in place; opening a newer database is refused.

| Table | Purpose |
|---|---|
| `settings` | key/value app settings |
| `platform_apps` | client ID, redirect port, Graph version per provider (`google`, `meta`, `spotify`) |
| `accounts` | channels, Pages, IG accounts, Spotify users and podcasts (`parent_account_id`), status, token expiry, last sync |
| `content_items` | videos, posts, reels, episodes with `stats_json` (latest counters) |
| `metric_snapshots` | daily values per account (or content), unique on account/content/metric/day, with source |
| `breakdowns` | latest dimensional data (traffic sources, demographics, geography) with period |
| `sync_runs` | sync history, item counts, unavailable metrics, errors |
| `media_assets` | media library (references to local files, optional public URL) |
| `drafts`, `draft_variants` | content studio: one draft, one version per target account with status/schedule |
| `publish_history` | every publish/schedule attempt and result |
| `templates` | reusable text and hashtags |
| `automations`, `automation_runs` | rules (JSON) and de-duplicated firings |
| `notifications` | notification center |
| `ig_export_imports`, `ig_export_entries` | Instagram export snapshots (followers/following usernames) |
| `audit_log` | connects, disconnects, publishes, imports, exports, backups |

Indexes cover metric lookups (`account_id, metric, day`), content by date, schedules (`status, scheduled_at`) and unread notifications.

## Adding a platform (TikTok, Threads, X, LinkedIn, Twitch…)

1. Add the platform to `Platform`, the `accounts.platform` CHECK (new migration), and `PLATFORM_META` in `src/lib/platforms.ts`.
2. Create `connectors/<name>.rs` with `OAUTH`, `SCOPES`, `discover`, `sync`, and optionally `comments`/`moderate`/`publish`.
3. Add its capability matrix entry and dispatch arms in `connectors/mod.rs`.
4. Add parser unit tests with real (anonymised) API response fixtures.
