import type { Account, ContentItem, MetricPoint, Platform, ReportData } from "./api";
import { parseJson } from "./api";
import { PLATFORM_META, STOCK_METRICS, metricLabel, summarize } from "./platforms";
import { pctChange } from "./format";

/** ISO week key, e.g. "2026-W05". */
export function weekKey(iso: string): string {
  const d = new Date(iso.slice(0, 10) + "T00:00:00Z");
  const day = (d.getUTCDay() + 6) % 7;
  d.setUTCDate(d.getUTCDate() - day + 3);
  const firstThursday = new Date(Date.UTC(d.getUTCFullYear(), 0, 4));
  const week = 1 + Math.round(((d.getTime() - firstThursday.getTime()) / 86400000 - 3 + ((firstThursday.getUTCDay() + 6) % 7)) / 7);
  return `${d.getUTCFullYear()}-W${String(week).padStart(2, "0")}`;
}

/** Items published per week per platform (counts are comparable across platforms). */
export function publishingFrequency(content: ContentItem[], from: string, to: string, platforms: Platform[]) {
  const rows = new Map<string, Record<string, number | string>>();
  for (const c of content) {
    const day = c.published_at?.slice(0, 10);
    if (!day || day < from || day > to || !platforms.includes(c.platform)) continue;
    const k = weekKey(day);
    const r = rows.get(k) ?? { name: k };
    r[c.platform] = ((r[c.platform] as number) ?? 0) + 1;
    rows.set(k, r);
  }
  return [...rows.values()].sort((a, b) => String(a.name).localeCompare(String(b.name)));
}

export interface SummaryRow { account: Account; metric: string; value: number | null; previous: number | null; change: number | null; source: string }

/** Per account and metric: range value (sum for flows, latest for stocks) and previous period. */
export function summarizeSeries(accounts: Account[], current: MetricPoint[], previous: MetricPoint[]): SummaryRow[] {
  const out: SummaryRow[] = [];
  for (const a of accounts) {
    const metrics = [...new Set(current.filter((p) => p.account_id === a.id).map((p) => p.metric))].sort();
    for (const m of metrics) {
      const cur = current.filter((p) => p.account_id === a.id && p.metric === m);
      const prev = previous.filter((p) => p.account_id === a.id && p.metric === m);
      const value = summarize(m, cur.map((p) => p.value));
      const pv = summarize(m, prev.map((p) => p.value));
      out.push({ account: a, metric: m, value, previous: pv, change: value != null && pv != null ? pctChange(value, pv) : null, source: cur[cur.length - 1]?.source ?? "" });
    }
  }
  return out;
}

export function buildReport(opts: {
  title: string; from: string; to: string; lang: "en" | "pt"; accounts: Account[];
  summary: SummaryRow[]; daily: MetricPoint[]; content: ContentItem[];
}): ReportData {
  const { lang } = opts;
  const byId = new Map(opts.accounts.map((a) => [a.id, a]));
  const name = (id: number) => byId.get(id)?.name ?? String(id);
  const plat = (id: number) => { const p = byId.get(id)?.platform; return p ? PLATFORM_META[p].label : ""; };
  const unavailable = opts.accounts.flatMap((a) => (a.unavailable ?? "").split("\n").filter(Boolean).map((u) => [PLATFORM_META[a.platform].label, a.name, u]));
  return {
    title: opts.title,
    sheets: [
      {
        name: "Summary",
        columns: ["Platform", "Account", "Metric", "Aggregation", `Value ${opts.from}..${opts.to}`, "Previous period", "Change %", "Source"],
        rows: opts.summary.map((s) => [PLATFORM_META[s.account.platform].label, s.account.name, metricLabel(s.metric, lang), STOCK_METRICS.has(s.metric) ? "latest" : s.metric === "avg_view_duration_secs" ? "average" : "sum", s.value, s.previous, s.change == null ? null : Math.round(s.change * 10) / 10, s.source]),
      },
      { name: "Daily", columns: ["Day", "Platform", "Account", "Metric", "Value", "Source"], rows: opts.daily.map((p) => [p.day, plat(p.account_id), name(p.account_id), metricLabel(p.metric, lang), p.value, p.source]) },
      {
        name: "Content",
        columns: ["Platform", "Account", "Title", "Published", "Type", "Views", "Likes", "Comments", "Reactions", "Shares", "URL"],
        rows: opts.content.map((c) => {
          const s = parseJson<Record<string, number>>(c.stats_json, {});
          return [PLATFORM_META[c.platform].label, c.account_name, c.title, c.published_at?.slice(0, 10) ?? null, c.kind, s.views ?? null, s.likes ?? null, s.comments ?? null, s.reactions ?? null, s.shares ?? null, c.url];
        }),
      },
      { name: "Not available", columns: ["Platform", "Account", "Detail"], rows: unavailable.length ? unavailable : [["", "", "All requested metrics were available."]] },
    ],
  };
}
