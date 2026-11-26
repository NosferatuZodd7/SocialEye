import { invoke } from "@tauri-apps/api/core";

/** Mirrors the Rust `AppError` enum (serde tag = "kind"). */
export interface AppError {
  kind:
    | "database" | "secret" | "not_configured" | "auth" | "api" | "rate_limited"
    | "network" | "unsupported" | "validation" | "not_found" | "io";
  message?: string;
  platform?: string;
  capability?: string;
  reason?: string;
  manual_alternative?: string | null;
  reconnect_required?: boolean;
  retry_after_secs?: number;
  status?: number;
}

export const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  const err = e as AppError;
  if (err?.kind === "unsupported") return `${err.reason}${err.manual_alternative ? ` — ${err.manual_alternative}` : ""}`;
  if (err?.kind === "rate_limited") return `Rate limited by the platform. Try again in ${err.retry_after_secs}s.`;
  if (err?.kind === "not_configured") return `${err.platform}: ${err.message}`;
  if (err?.message) return err.message;
  return e instanceof Error ? e.message : String(e);
}

export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw { kind: "not_configured", platform: "SocialEye", message: "the desktop backend is not available in a plain browser. Run `npm run tauri dev`." } satisfies AppError;
  }
  return invoke<T>(cmd, args);
}

// ---- types (rows come straight from SQLite) ----
export type Platform = "instagram" | "facebook" | "youtube" | "spotify";

export interface Account {
  id: number; platform: Platform; account_type: string; external_id: string; name: string;
  handle: string | null; avatar_url: string | null; parent_account_id: number | null;
  status: "connected" | "reconnect_required" | "disconnected"; token_expires_at: number | null;
  auto_sync: number; last_sync_at: string | null; last_sync_status: string | null; last_error: string | null;
  unavailable: string | null;
}
export interface ContentItem {
  id: number; account_id: number; external_id: string; kind: string; title: string | null; description: string | null;
  url: string | null; thumbnail_url: string | null; published_at: string | null; duration_secs: number | null;
  stats_json: string; platform: Platform; account_name: string;
}
export interface MetricPoint { account_id: number; metric: string; day: string; value: number; source: string }
export interface Capability { id: string; status: "available" | "restricted" | "unsupported"; requirement: string; note: string; manual_alternative: string | null }
export interface Comment { id: string; author: string | null; text: string; published_at: string | null; hidden: boolean | null; like_count: number | null }
export interface Media { id: number; path: string; name: string; kind: "image" | "video" | "audio" | "other"; size_bytes: number; public_url: string | null; missing: boolean }
export interface Draft { id: number; title: string; body: string; category: string | null; tags: string; hashtags: string; media_asset_id: number | null; media_name: string | null; media_kind: string | null; variants_json: string; updated_at: string }
export interface Variant { id: number; draft_id: number; account_id: number; title: string | null; body: string; media_asset_id: number | null; thumbnail_asset_id: number | null; link: string | null; privacy: string | null; status: string; scheduled_at: string | null; published_at: string | null; external_id: string | null; error: string | null; platform: Platform; account_name: string }
export interface Notification { id: number; kind: string; title: string; body: string; read: number; created_at: string; platform: Platform | null; account_name: string | null }
export interface Automation { id: number; name: string; enabled: number; rule_json: string; last_fired_at: string | null; run_count: number }
export interface Provider { provider: "google" | "meta" | "spotify"; configured: boolean; has_secret: boolean; app: { client_id: string; redirect_port: number; api_version: string | null; updated_at: string } | null }

export const parseJson = <T,>(s: string | null | undefined, fallback: T): T => {
  try { return s ? (JSON.parse(s) as T) : fallback; } catch { return fallback; }
};

export const api = {
  appInfo: () => call<{ version: string; schema_version: number; data_dir: string; telemetry: boolean }>("app_info"),
  settings: () => call<Record<string, string>>("settings_get"),
  setSetting: (key: string, value: string) => call<void>("settings_set", { key, value }),
  providers: () => call<Provider[]>("providers_list"),
  saveProvider: (input: { provider: string; client_id: string; client_secret: string | null; redirect_port: number; api_version: string | null }) => call<void>("provider_save", { input }),
  capabilities: (platform: Platform) => call<Capability[]>("capabilities", { platform }),
  accounts: () => call<Account[]>("accounts_list"),
  connect: (platform: Platform) => call<number[]>("account_connect", { platform }),
  disconnect: (id: number) => call<void>("account_disconnect", { id }),
  removeAccount: (id: number) => call<void>("account_remove", { id, confirmed: true }),
  setAutoSync: (id: number, enabled: boolean) => call<void>("account_set_auto_sync", { id, enabled }),
  sync: (id: number) => call<{ content: number; metrics: number; unavailable: string[] }>("account_sync", { id }),
  syncRuns: (accountId?: number) => call<Record<string, unknown>[]>("sync_runs", { accountId: accountId ?? null }),
  addPodcast: (parentId: number, show: string) => call<number>("spotify_add_podcast", { parentId, show }),
  savedShows: (parentId: number) => call<{ external_id: string; name: string; handle: string | null; avatar_url: string | null }[]>("spotify_saved_shows", { parentId }),
  content: (q: { account_id?: number; platform?: string; kind?: string; from?: string; to?: string; search?: string; sort?: string; limit?: number }) => call<ContentItem[]>("content_list", { q }),
  series: (q: { account_ids: number[]; metrics: string[]; from: string; to: string; content_id?: number }) => call<MetricPoint[]>("metrics_series", { q }),
  latest: () => call<MetricPoint[]>("metrics_latest"),
  breakdowns: (accountId: number) => call<{ kind: string; period_start: string; period_end: string; data_json: string; source: string }[]>("breakdowns", { accountId }),
  retention: (accountId: number, videoId: string) => call<{ elapsedVideoTimeRatio: number; audienceWatchRatio: number }[]>("youtube_retention", { accountId, videoId }),
  updateVideo: (accountId: number, videoId: string, title: string, description: string, privacy: string) => call<void>("youtube_update_video", { accountId, videoId, title, description, privacy, confirmed: true }),
  comments: (accountId: number, contentExternalId: string) => call<Comment[]>("comments_list", { accountId, contentExternalId }),
  moderate: (accountId: number, commentId: string, moderation: { action: "hide" | "unhide" | "delete" } | { action: "reply"; text: string }, confirmed = false) =>
    call<void>("comment_moderate", { accountId, commentId, moderation, confirmed }),
  media: () => call<Media[]>("media_list"),
  addMedia: (paths: string[]) => call<number[]>("media_add", { paths }),
  setMediaUrl: (id: number, url: string | null) => call<void>("media_set_public_url", { id, url }),
  removeMedia: (id: number) => call<void>("media_remove", { id }),
  drafts: () => call<Draft[]>("drafts_list"),
  saveDraft: (d: { id?: number | null; title: string; body: string; category: string | null; tags: string; hashtags: string; media_asset_id: number | null }) => call<number>("draft_save", { d }),
  deleteDraft: (id: number) => call<void>("draft_delete", { id, confirmed: true }),
  variants: (draftId: number) => call<Variant[]>("variants_list", { draftId }),
  saveVariant: (v: { id?: number | null; draft_id: number; account_id: number; title: string | null; body: string; media_asset_id: number | null; thumbnail_asset_id: number | null; link: string | null; privacy: string | null; scheduled_at: string | null }) => call<number>("variant_save", { v }),
  deleteVariant: (id: number) => call<void>("variant_delete", { id }),
  validateVariant: (id: number) => call<string[]>("variant_validate", { id }),
  publishVariant: (id: number) => call<{ status: string; url?: string; local?: boolean }>("variant_publish", { id, confirmed: true }),
  unschedule: (id: number) => call<void>("variant_unschedule", { id }),
  publishHistory: () => call<Record<string, string | number | null>[]>("publish_history"),
  templates: () => call<{ id: number; name: string; body: string; hashtags: string }[]>("templates_list"),
  saveTemplate: (id: number | null, name: string, body: string, hashtags: string) => call<number>("template_save", { id, name, body, hashtags }),
  deleteTemplate: (id: number) => call<void>("template_delete", { id }),
  calendar: (from: string, to: string) => call<{ source: string; id: number; draft_id: number | null; title: string; platform: Platform; account_name: string; status: string; at: string; url: string | null }[]>("calendar_items", { from, to }),
  automations: () => call<Automation[]>("automations_list"),
  saveAutomation: (id: number | null, name: string, enabled: boolean, rule: unknown) => call<number>("automation_save", { id, name, enabled, rule }),
  setAutomationEnabled: (id: number, enabled: boolean) => call<void>("automation_set_enabled", { id, enabled }),
  deleteAutomation: (id: number) => call<void>("automation_delete", { id }),
  automationRuns: () => call<{ id: number; name: string; ran_at: string; status: string; message: string | null }[]>("automation_runs"),
  notifications: () => call<Notification[]>("notifications_list"),
  markRead: (ids?: number[]) => call<void>("notifications_mark_read", { ids: ids ?? null }),
  clearRead: () => call<void>("notifications_clear"),
  igImport: (label: string, files: { name: string; content: string }[]) => call<{ id: number; followers: number; following: number; used_files: string[]; warnings: string[] }>("ig_import", { label, files }),
  igImports: () => call<{ id: number; label: string; imported_at: string; followers_count: number; following_count: number; warnings: string | null }[]>("ig_imports_list"),
  igDeleteImport: (id: number) => call<void>("ig_import_delete", { id }),
  igCompare: (importId: number, previousId: number | null) => call<{ not_following_back: string[]; fans: string[]; mutual: string[]; new_followers: string[] | null; lost_followers: string[] | null; new_following: string[] | null }>("ig_compare", { importId, previousId }),
  csvImport: (accountId: number, fileName: string, content: string) => call<{ stored: number; metrics: string[]; ignored_columns: string[]; skipped_rows: number; unmatched_episodes: string[] }>("csv_import_metrics", { accountId, fileName, content }),
  exportReport: (path: string, format: "csv" | "xlsx", report: ReportData) => call<void>("export_report", { path, format, report }),
  backup: (path: string) => call<void>("backup_create", { path }),
  restore: (path: string) => call<void>("backup_restore", { path, confirmed: true }),
  exportJson: (path: string) => call<void>("data_export_json", { path }),
  search: (q: string) => call<{ accounts: { id: number; platform: Platform; name: string; handle: string | null }[]; content: { id: number; title: string; kind: string; url: string | null; platform: Platform }[]; drafts: { id: number; title: string }[] }>("search", { q }),
  updatesStatus: () => call<{ enabled: boolean; current: string; channel: "testing" | "stable"; repository: string; auto_check: boolean }>("updates_status"),
  updatesCheck: () => call<UpdateInfo | null>("updates_check"),
  updatesInstall: () => call<void>("updates_install", { confirmed: true }),
  auditLog: () => call<{ id: number; at: string; action: string; target: string | null; detail: string | null }[]>("audit_log"),
};

export interface UpdateInfo { version: string; tag: string; prerelease: boolean; notes: string; published_at: string | null; html_url: string }

export interface ReportData { title: string; sheets: { name: string; columns: string[]; rows: (string | number | null)[][] }[] }
