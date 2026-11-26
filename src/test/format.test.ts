import { describe, expect, it } from "vitest";
import { fmtDuration, fmtNumber, parseDate, pctChange, previousRange } from "../lib/format";
import { summarize, metricLabel } from "../lib/platforms";
import { weekKey, publishingFrequency, summarizeSeries, buildReport } from "../lib/analytics";
import type { Account, ContentItem, MetricPoint } from "../lib/api";

describe("format", () => {
  it("treats SQLite datetimes as UTC", () => {
    expect(parseDate("2026-01-02 03:04:05")?.toISOString()).toBe("2026-01-02T03:04:05.000Z");
    expect(parseDate("nonsense")).toBeNull();
  });
  it("never renders missing numbers as zero", () => {
    expect(fmtNumber(null)).toBe("—");
    expect(fmtNumber(1234567, "en")).toBe("1,234,567");
  });
  it("durations and ranges", () => {
    expect(fmtDuration(3723)).toBe("1:02:03");
    expect(fmtDuration(65)).toBe("1:05");
    expect(previousRange("2026-01-29", "2026-02-25")).toEqual({ from: "2026-01-01", to: "2026-01-28" });
    expect(pctChange(110, 100)).toBeCloseTo(10);
    expect(pctChange(5, 0)).toBeNull();
  });
});

describe("metrics", () => {
  it("sums flows, takes the latest stock, averages durations", () => {
    expect(summarize("views", [1, 2, 3])).toBe(6);
    expect(summarize("subscribers", [10, 12, 11])).toBe(11);
    expect(summarize("avg_view_duration_secs", [10, 20])).toBe(15);
    expect(summarize("views", [])).toBeNull();
  });
  it("labels distinguish platform metrics", () => {
    expect(metricLabel("views")).not.toBe(metricLabel("plays"));
    expect(metricLabel("plays", "pt")).toBe("Reproduções");
    expect(metricLabel("custom_thing")).toBe("custom thing");
  });
});

const acc = (id: number, platform: Account["platform"]): Account => ({ id, platform, account_type: "x", external_id: String(id), name: `A${id}`, handle: null, avatar_url: null, parent_account_id: null, status: "connected", token_expires_at: null, auto_sync: 1, last_sync_at: null, last_sync_status: null, last_error: null, unavailable: platform === "spotify" ? "plays: not exposed" : null });
const pt = (account_id: number, metric: string, day: string, value: number): MetricPoint => ({ account_id, metric, day, value, source: "test" });

describe("analytics", () => {
  it("computes ISO weeks", () => {
    expect(weekKey("2026-01-01")).toBe("2026-W01");
    expect(weekKey("2027-01-01")).toBe("2026-W53");
    expect(weekKey("2026-12-28")).toBe("2026-W53");
  });
  it("counts publishing frequency per platform and respects filters", () => {
    const c = (platform: Account["platform"], published_at: string) => ({ platform, published_at } as ContentItem);
    const f = publishingFrequency([c("youtube", "2026-01-05T10:00:00Z"), c("youtube", "2026-01-06"), c("spotify", "2026-01-07"), c("facebook", "2025-01-01")], "2026-01-01", "2026-01-31", ["youtube", "spotify"]);
    expect(f).toEqual([{ name: "2026-W02", youtube: 2, spotify: 1 }]);
  });
  it("summaries never merge different accounts or metrics", () => {
    const s = summarizeSeries([acc(1, "youtube"), acc(2, "spotify")], [pt(1, "views", "2026-01-02", 5), pt(1, "views", "2026-01-03", 5), pt(2, "plays", "2026-01-02", 7)], [pt(1, "views", "2025-12-30", 5)]);
    expect(s).toHaveLength(2);
    expect(s.find((x) => x.metric === "views")).toMatchObject({ value: 10, previous: 5, change: 100 });
    expect(s.find((x) => x.metric === "plays")).toMatchObject({ value: 7, previous: null, change: null });
  });
  it("report lists unavailable metrics", () => {
    const r = buildReport({ title: "T", from: "a", to: "b", lang: "en", accounts: [acc(2, "spotify")], summary: [], daily: [], content: [] });
    expect(r.sheets.map((s) => s.name)).toEqual(["Summary", "Daily", "Content", "Not available"]);
    expect(r.sheets[3].rows[0]).toEqual(["Spotify", "A2", "plays: not exposed"]);
  });
});
