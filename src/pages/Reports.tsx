import { useState } from "react";
import { api, type Platform, type ReportData } from "../lib/api";
import { useAction } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { daysAgo, fmtNumber, isoDay, previousRange } from "../lib/format";
import { PLATFORMS, PLATFORM_META, metricLabel } from "../lib/platforms";
import { buildReport, summarizeSeries, type SummaryRow } from "../lib/analytics";
import { saveDialog } from "../lib/files";
import { useFeedback } from "../components/feedback";
import { Button, Empty, ErrorBox, Input, PageHeader, Panel, Table, Toggle } from "../components/ui";
import { RangePicker } from "./Analytics";

export default function Reports() {
  const t = useT();
  const { accounts } = useStore();
  const { toast } = useFeedback();
  const [title, setTitle] = useState(t("rep.defaultTitle"));
  const [range, setRange] = useState({ from: daysAgo(28), to: isoDay(new Date()) });
  const [platforms, setPlatforms] = useState<Platform[]>(PLATFORMS);
  const [metricFilter, setMetricFilter] = useState<string[] | null>(null);
  const [result, setResult] = useState<{ report: ReportData; summary: SummaryRow[]; metrics: string[] } | null>(null);
  const act = useAction();
  const accs = accounts.filter((a) => platforms.includes(a.platform) && a.account_type !== "user");

  const generate = () => act.run(async () => {
    const ids = accs.map((a) => a.id);
    const latest = await api.latest();
    const all = [...new Set(latest.filter((m) => ids.includes(m.account_id)).map((m) => m.metric))].sort();
    const metrics = metricFilter ? all.filter((m) => metricFilter.includes(m)) : all;
    const prev = previousRange(range.from, range.to);
    const [cur, before, content] = await Promise.all([
      api.series({ account_ids: ids, metrics, from: range.from, to: range.to }),
      api.series({ account_ids: ids, metrics, from: prev.from, to: prev.to }),
      api.content({ from: range.from, to: range.to, limit: 5000 }),
    ]);
    const summary = summarizeSeries(accs, cur, before);
    setResult({ summary, metrics: all, report: buildReport({ title, ...range, lang: t.lang, accounts: accs, summary, daily: cur, content: content.filter((c) => ids.includes(c.account_id)) }) });
  });

  const exportAs = (format: "csv" | "xlsx") => result && act.run(async () => {
    const path = await saveDialog(`${title.replace(/[^\w\- ]+/g, "")}.${format}`, format, format.toUpperCase());
    if (!path) return;
    await api.exportReport(path, format, result.report);
    toast(t("rep.exported"));
  });

  return (
    <>
      <PageHeader title={t("rep.title")} subtitle={t("rep.subtitle")} />
      <Panel className="no-print mb-4">
        <div className="grid gap-3 md:grid-cols-[1fr_auto]">
          <Input label={t("rep.reportTitle")} value={title} onChange={(e) => setTitle(e.target.value)} />
          <RangePicker from={range.from} to={range.to} onChange={(from, to) => setRange({ from, to })} />
        </div>
        <div className="mt-3 flex flex-wrap gap-4">{PLATFORMS.map((p) => <Toggle key={p} label={PLATFORM_META[p].label} checked={platforms.includes(p)} onChange={(v) => setPlatforms((x) => (v ? [...x, p] : x.filter((y) => y !== p)))} />)}</div>
        {result && (
          <div className="mt-3">
            <div className="mb-1 text-[12.5px] text-[var(--muted)]">{t("rep.metrics")}</div>
            <div className="flex flex-wrap gap-3">{result.metrics.map((m) => <Toggle key={m} label={metricLabel(m, t.lang)} checked={!metricFilter || metricFilter.includes(m)} onChange={(v) => setMetricFilter((f) => { const cur = f ?? result.metrics; return v ? [...cur, m] : cur.filter((x) => x !== m); })} />)}</div>
          </div>
        )}
        <div className="mt-4 flex flex-wrap gap-2">
          <Button variant="primary" busy={act.busy} onClick={generate} disabled={!accs.length}>{t("common.refresh")}</Button>
          <Button disabled={!result} onClick={() => window.print()}>{t("rep.pdf")}</Button>
          <Button disabled={!result} onClick={() => exportAs("csv")}>{t("rep.csv")}</Button>
          <Button disabled={!result} onClick={() => exportAs("xlsx")}>{t("rep.xlsx")}</Button>
        </div>
        <p className="mt-2 text-[12px] text-[var(--muted)]">{t("rep.pdfHint")}</p>
        {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
      </Panel>
      {!accs.length ? <Empty title={t("acc.empty")} /> : result && (
        <div className="grid gap-4">
          <div><h2 className="text-[20px] font-semibold">{title}</h2><p className="text-[var(--muted)]">{range.from} – {range.to}</p><p className="mt-1 text-[12px] text-[var(--muted)]">{t("rep.sourceNote")}</p></div>
          <Panel title={t("rep.summary")}>
            <Table head={[t("common.platform"), t("common.account"), t("common.metric"), t("common.value"), t("an.period"), t("common.source")]} empty={!result.summary.length}>
              {result.summary.filter((s) => !metricFilter || metricFilter.includes(s.metric)).map((s) => (
                <tr key={`${s.account.id}-${s.metric}`}>
                  <td>{PLATFORM_META[s.account.platform].label}</td><td>{s.account.name}</td><td>{metricLabel(s.metric, t.lang)}</td>
                  <td className="num">{fmtNumber(s.value, t.lang)}</td>
                  <td className={s.change == null ? "text-[var(--muted)]" : s.change >= 0 ? "text-ok" : "text-bad"}>{s.change == null ? "—" : `${s.change >= 0 ? "+" : ""}${s.change.toFixed(1)}%`}</td>
                  <td className="text-xs text-[var(--muted)]">{s.source}</td>
                </tr>
              ))}
            </Table>
          </Panel>
          {result.report.sheets.slice(2).map((sh) => (
            <Panel key={sh.name} title={sh.name === "Content" ? t("rep.content") : sh.name}>
              <Table head={sh.columns}>{sh.rows.slice(0, 200).map((r, i) => <tr key={i}>{r.map((c, j) => <td key={j} className={typeof c === "number" ? "num" : "max-w-xs truncate"}>{typeof c === "number" ? fmtNumber(c, t.lang) : c ?? "—"}</td>)}</tr>)}</Table>
            </Panel>
          ))}
        </div>
      )}
    </>
  );
}
