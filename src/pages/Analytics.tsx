import { useState } from "react";
import { api, type Platform } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { daysAgo, fmtNumber, isoDay, previousRange } from "../lib/format";
import { PLATFORMS, PLATFORM_META, STOCK_METRICS, metricLabel } from "../lib/platforms";
import { publishingFrequency, summarizeSeries } from "../lib/analytics";
import { performance } from "./PlatformPage";
import { Bars, MetricTrend, PLATFORM_HEX } from "../components/charts";
import { Empty, ErrorBox, Input, PageHeader, Panel, PlatformDot, Spinner, Stat, Toggle } from "../components/ui";

export function RangePicker({ from, to, onChange }: { from: string; to: string; onChange: (f: string, t: string) => void }) {
  const t = useT();
  return (
    <div className="flex items-end gap-2">
      <Input label={t("common.from")} type="date" value={from} max={to} onChange={(e) => onChange(e.target.value, to)} />
      <Input label={t("common.to")} type="date" value={to} min={from} onChange={(e) => onChange(from, e.target.value)} />
    </div>
  );
}

export default function Analytics() {
  const t = useT();
  const { accounts } = useStore();
  const [range, setRange] = useState({ from: daysAgo(28), to: isoDay(new Date()) });
  const [platforms, setPlatforms] = useState<Platform[]>(PLATFORMS);
  const accs = accounts.filter((a) => platforms.includes(a.platform) && a.account_type !== "user");
  const prev = previousRange(range.from, range.to);

  const d = useLoad(async () => {
    if (!accs.length) return null;
    const ids = accs.map((a) => a.id);
    const latest = await api.latest();
    const metrics = [...new Set(latest.filter((m) => ids.includes(m.account_id)).map((m) => m.metric))];
    const [cur, before, content] = await Promise.all([
      api.series({ account_ids: ids, metrics, from: range.from, to: range.to }),
      api.series({ account_ids: ids, metrics, from: prev.from, to: prev.to }),
      api.content({ from: range.from, to: range.to, limit: 5000 }),
    ]);
    return { cur, before, content: content.filter((c) => ids.includes(c.account_id)) };
  }, [accs.map((a) => a.id).join(","), range.from, range.to]);

  return (
    <>
      <PageHeader title={t("an.title")} subtitle={t("an.subtitle")} actions={<RangePicker from={range.from} to={range.to} onChange={(from, to) => setRange({ from, to })} />} />
      <div className="mb-4 flex flex-wrap items-center gap-4">
        <span className="text-[13px] text-[var(--muted)]">{t("an.platforms")}</span>
        {PLATFORMS.map((p) => <Toggle key={p} label={PLATFORM_META[p].label} checked={platforms.includes(p)} onChange={(v) => setPlatforms((x) => (v ? [...x, p] : x.filter((y) => y !== p)))} />)}
      </div>
      <p className="mb-4 text-[12.5px] text-[var(--muted)]">{t("an.metricNote")}</p>
      {!accs.length ? <Empty title={t("acc.empty")} /> : d.loading && !d.data ? <Spinner /> : d.error ? <ErrorBox message={d.error} onRetry={d.reload} /> : d.data && (() => {
        const summary = summarizeSeries(accs, d.data.cur, d.data.before);
        const audience = accs.map((a) => ({ a, m: PLATFORM_META[a.platform].audience, row: summary.find((s) => s.account.id === a.id && s.metric === PLATFORM_META[a.platform].audience) }));
        const growthRows = d.data.cur.filter((p) => { const a = accs.find((x) => x.id === p.account_id); return a && p.metric === PLATFORM_META[a.platform].audience; });
        const freq = publishingFrequency(d.data.content, range.from, range.to, platforms);
        if (!d.data.cur.length && !d.data.content.length) return <Empty title={t("an.noData")} />;
        return (
          <div className="grid gap-4">
            <Panel title={t("an.audience")}>
              <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
                {audience.map(({ a, m, row }) => (
                  <div key={a.id}><div className="mb-1 flex items-center gap-1.5 truncate text-[12.5px]"><PlatformDot platform={a.platform} />{a.name}</div>
                    <Stat label={metricLabel(m, t.lang)} value={row?.value ?? null} delta={row?.change} source={row?.source} reason={t("an.noSnapshot")} /></div>
                ))}
              </div>
            </Panel>
            {growthRows.length > 0 && (
              <Panel title={t("an.growth")}>
                <MetricTrend zeroBased={false} rows={growthRows.map((r) => ({ ...r, metric: String(r.account_id) }))} metrics={[...new Set(growthRows.map((r) => String(r.account_id)))]}
                  label={(id) => { const a = accs.find((x) => String(x.id) === id)!; return `${a.name} (${metricLabel(PLATFORM_META[a.platform].audience, t.lang)})`; }}
                  color={(id) => PLATFORM_HEX[accs.find((x) => String(x.id) === id)!.platform]} />
              </Panel>
            )}
            <h2 className="mt-2 text-[17px] font-semibold">{t("an.flows")}</h2>
            <div className="grid gap-4 lg:grid-cols-2">
              {platforms.map((p) => {
                const ids = accs.filter((a) => a.platform === p).map((a) => a.id);
                const rows = summary.filter((s) => ids.includes(s.account.id) && !STOCK_METRICS.has(s.metric));
                if (!ids.length) return null;
                const series = d.data!.cur.filter((x) => ids.includes(x.account_id) && PLATFORM_META[p].primary.includes(x.metric));
                // Same metric across accounts of the same platform is additive (e.g. views of two channels).
                const merged = Object.values(series.reduce<Record<string, { day: string; metric: string; value: number }>>((acc, x) => { const k = `${x.day}|${x.metric}`; acc[k] = { day: x.day, metric: x.metric, value: (acc[k]?.value ?? 0) + x.value }; return acc; }, {}));
                return (
                  <Panel key={p} title={<span className="flex items-center gap-2"><PlatformDot platform={p} />{PLATFORM_META[p].label}</span>}>
                    {merged.length ? <MetricTrend rows={merged} metrics={[...new Set(merged.map((m) => m.metric))]} label={(m) => metricLabel(m, t.lang)} color={PLATFORM_HEX[p]} height={200} />
                      : <div className="hatched rounded-lg p-6 text-center text-[var(--muted)]">{t("common.unavailable")}</div>}
                    <table className="mt-3 w-full text-[13px]"><tbody>
                      {rows.map((r) => (
                        <tr key={`${r.account.id}${r.metric}`} className="border-t border-[var(--line)]/60">
                          <td className="py-1.5">{metricLabel(r.metric, t.lang)}{ids.length > 1 && <span className="text-[var(--muted)]"> — {r.account.name}</span>}</td>
                          <td className="num text-right">{fmtNumber(r.value, t.lang)}</td>
                          <td className={`w-28 text-right text-xs ${r.change == null ? "text-[var(--muted)]" : r.change >= 0 ? "text-ok" : "text-bad"}`} title={t("an.period")}>{r.change == null ? "—" : `${r.change >= 0 ? "+" : ""}${r.change.toFixed(1)}%`}</td>
                        </tr>
                      ))}
                    </tbody></table>
                  </Panel>
                );
              })}
            </div>
            {freq.length > 0 && (
              <Panel title={t("an.frequency")}>
                <Bars data={freq} series={platforms.filter((p) => freq.some((f) => f[p])).map((p) => ({ key: p, label: PLATFORM_META[p].label, color: PLATFORM_HEX[p] }))} />
              </Panel>
            )}
            <Panel title={t("an.topContent")}>
              <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
                {platforms.map((p) => {
                  const top = d.data!.content.filter((c) => c.platform === p).map((c) => ({ c, v: performance(c) })).filter((x) => x.v != null).sort((a, b) => b.v! - a.v!).slice(0, 5);
                  return (
                    <div key={p}>
                      <div className="mb-1 flex items-center gap-1.5 text-[12.5px] font-medium"><PlatformDot platform={p} />{PLATFORM_META[p].label} <span className="font-normal text-[var(--muted)]">({p === "youtube" ? metricLabel("views", t.lang) : t("common.mostEngagement").toLowerCase()})</span></div>
                      {top.length ? <ol className="space-y-1 text-[13px]">{top.map(({ c, v }) => <li key={c.id} className="flex gap-2"><span className="flex-1 truncate">{c.title ?? c.kind}</span><span className="num">{fmtNumber(v, t.lang)}</span></li>)}</ol>
                        : <div className="hatched rounded p-3 text-xs text-[var(--muted)]">{t("common.unavailable")}</div>}
                    </div>
                  );
                })}
              </div>
            </Panel>
          </div>
        );
      })()}
    </>
  );
}
