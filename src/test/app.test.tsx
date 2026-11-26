/**
 * Integration test: renders every route of the real App against a mocked Tauri backend.
 * The data below is illustrative sample data used only by tests.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

const day = (n: number) => new Date(Date.now() - n * 86400000).toISOString().slice(0, 10);
const accounts = [
  { id: 1, platform: "youtube", account_type: "channel", external_id: "UC1", name: "Sample Channel", handle: "@sample", avatar_url: null, parent_account_id: null, status: "connected", token_expires_at: null, auto_sync: 1, last_sync_at: "2026-11-26 00:10:00", last_sync_status: "ok", last_error: null, unavailable: "impressions, impressions CTR: not exposed" },
  { id: 5, platform: "spotify", account_type: "podcast", external_id: "s1", name: "Sample Podcast", handle: "Sample Media", avatar_url: null, parent_account_id: null, status: "reconnect_required", token_expires_at: null, auto_sync: 1, last_sync_at: null, last_sync_status: "failed", last_error: "authorization error: token expired", unavailable: "followers, plays: not exposed by the Spotify Web API" },
];
const responses: Record<string, unknown> = {
  settings_get: { language: "en" },
  accounts_list: accounts,
  metrics_latest: [{ account_id: 1, metric: "subscribers", value: 1234, day: day(0), source: "YouTube Data API v3" }],
  metrics_series: [{ account_id: 1, metric: "views", day: day(1), value: 10, source: "YouTube Analytics API" }],
  content_list: [{ id: 1, account_id: 1, external_id: "v1", kind: "short", title: "Sample video", description: "", url: null, thumbnail_url: null, published_at: day(2) + "T10:00:00Z", duration_secs: 40, stats_json: '{"views":100}', platform: "youtube", account_name: "Sample Channel" }],
  notifications_list: [{ id: 1, kind: "sync_failed", title: "Sample Podcast: synchronization failed", body: "token expired", read: 0, created_at: "2026-11-25 10:00:00", platform: "spotify", account_name: "Sample Podcast" }],
  providers_list: [{ provider: "google", configured: true, has_secret: true, app: { client_id: "x", redirect_port: 53682, api_version: null, updated_at: "" } }, { provider: "meta", configured: false, has_secret: false, app: null }, { provider: "spotify", configured: false, has_secret: false, app: null }],
  capabilities: [{ id: "publish", status: "unsupported", requirement: "—", note: "No public publishing API.", manual_alternative: "Use Spotify for Creators." }],
  updates_status: { enabled: true, current: "0.1.0-dev.3", channel: "testing", repository: "o/r", auto_check: true },
  updates_check: { version: "0.1.0-dev.7", tag: "v0.1.0-dev.7", prerelease: true, notes: "Fixes", published_at: null, html_url: "https://github.com/o/r/releases/tag/v0.1.0-dev.7" },
  app_info: { version: "0.1.0", schema_version: 1, data_dir: "C:\\data", telemetry: false },
};
const invoke = vi.fn(async (...a: [string, unknown?]) => { const cmd = a[0]; return (cmd.startsWith("plugin:") ? 1 : responses[cmd] ?? []); });

vi.mock("@tauri-apps/api/core", () => ({ invoke: (cmd: string, args: unknown) => invoke(cmd, args), convertFileSrc: (p: string) => p }));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));

beforeAll(() => { (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {}; });
afterEach(() => { cleanup(); window.location.hash = ""; });

const { default: App } = await import("../App");

const ROUTES: [string, string][] = [
  ["/", "Dashboard"], ["/accounts", "Accounts"], ["/instagram", "Instagram"], ["/facebook", "Facebook"], ["/youtube", "YouTube"],
  ["/spotify", "Spotify"], ["/studio", "Content studio"], ["/calendar", "Calendar"], ["/analytics", "Unified analytics"],
  ["/automations", "Automations"], ["/notifications", "Notifications"], ["/reports", "Reports"], ["/settings", "Settings"],
];

describe("App routes", () => {
  it.each(ROUTES)("renders %s", async (route, heading) => {
    window.location.hash = "#" + route;
    render(<App />);
    expect(await screen.findByRole("heading", { level: 1, name: heading })).toBeInTheDocument();
  });

  it("dashboard shows provenance and unavailable audience", async () => {
    window.location.hash = "#/";
    render(<App />);
    const panel = (await screen.findByText("Audience by platform")).closest("section")!;
    expect(within(panel).getByText("1,234")).toBeInTheDocument();
    expect(within(panel).getByText("YouTube Data API v3", { exact: false })).toBeInTheDocument();
    expect(within(panel).getByText("Not available")).toBeInTheDocument();
  });

  it("settings shows the update channel and an available signed update", async () => {
    window.location.hash = "#/settings";
    render(<App />);
    expect(await screen.findByText("0.1.0-dev.3")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "Channel" })).toHaveValue("testing");
    (await screen.findByRole("button", { name: "Check now" })).click();
    expect(await screen.findByText("Version 0.1.0-dev.7 is available")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install and restart" })).toBeInTheDocument();
  });

  it("accounts page surfaces reconnection and last error", async () => {
    window.location.hash = "#/accounts";
    render(<App />);
    expect(await screen.findByText("Needs reconnection")).toBeInTheDocument();
    expect(screen.getByText("authorization error: token expired")).toBeInTheDocument();
    expect(screen.getAllByText(/Add a Meta developer app/)).toHaveLength(2); // Instagram + Facebook share the Meta app
  });
});
