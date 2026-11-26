import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { api, type Account, type MetricPoint } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { daysAgo, fmtDate, isoDay } from "../lib/format";
import { PLATFORM_META, metricLabel } from "../lib/platforms";
import { Badge, Button, Empty, ErrorBox, Modal, PageHeader, Panel, PlatformBadge, PlatformDot, Spinner, Stat, Toggle } from "../components/ui";
import { MetricTrend } from "../components/charts";
import { openExternal } from "../lib/files";

const WIDGETS = ["audience", "activity", "upcoming", "recent", "unread"] as const;
type Widget = (typeof WIDGETS)[number];

export function audienceFor(acc: Account, latest: MetricPoint[]) {
  const metric = PLATFORM_META[acc.platform].audience;
  const p = latest.find((m) => m.account_id === acc.id && m.metric === metric);
  const unavailableLine = acc.unavailable?.split("\n").find((l) => l.startsWith(metric) || l.includes(metric));
  return { metric, point: p ?? null, reason: unavailableLine ?? undefined };
}

function Onboarding() {
  const t = useT();
  const steps = [
    { title: t("dash.onboard.step1"), body: t("dash.onboard.step1b"), to: "/settings" },
    { title: t("dash.onboard.step2"), body: t("dash.onboard.step2b"), to: "/accounts" },
    { title: t("dash.onboard.step3"), body: t("dash.onboard.step3b"), to: "/instagram?tab=followers" },
  ];
  return (
    <Panel title={t("dash.onboard.title")}>
      <p className="mb-4 max-w-[70ch] text-[var(--muted)]">{t("dash.onboard.body")}</p>
      <ol className="grid gap-3 md:grid-cols-3">
        {steps.map((s, i) => (
          <li key={s.to}>
            <Link to={s.to} className="block h-full rounded-xl border border-[var(--line)] p-4 hover:border-iris">
              <span className="num mb-2 inline-flex size-7 items-center justify-center rounded-full bg-iris/15 text-iris-soft">{i + 1}</span>
              <div className="font-medium">{s.title}</div>
              <div className="mt-1 text-[13px] text-[var(--muted)]">{s.body}</div>
            </Link>
          </li>
        ))}
      </ol>
    </Panel>
  );
}

export default function Dashboard() {
  const t = useT();
  const { accounts, settings, setSetting, backend } = useStore();
  const [customize, setCustomize] = useState(false);
  const active = useMemo<Widget[]>(() => {
    try { const v = JSON.parse(settings.dashboard_widgets ?? "null"); if (Array.isArray(v)) return v; } catch { /* default */ }
    return [...WIDGETS];
  }, [settings.dashboard_widgets]);
  const connected = accounts.filter((a) => a.status !== "disconnected" && a.account_type !== "user");
  const from = daysAgo(28), to = isoDay(new Date());
  const ids = connected.map((a) => a.id);

  const data = useLoad(async () => {
    if (!backend || !connected.length) return null;
    const metrics = [...new Set(connected.flatMap((a) => PLATFORM_META[a.platform].primary))];
    const [latest, series, recent, cal, notes] = await Promise.all([
      api.latest(), api.series({ account_ids: ids, metrics, from, to }), api.content({ limit: 6 }),
      api.calendar(isoDay(new Date()), daysAgo(-45)), api.notifications(),
    ]);
    return { latest, series, recent, upcoming: cal.filter((c) => c.source === "variant" && c.status === "scheduled"), unread: notes.filter((n) => !n.read).slice(0, 5) };
  }, [backend, ids.join(",")]);

  const toggle = (w: Widget, on: boolean) => setSetting("dashboard_widgets", JSON.stringify(on ? [...active, w] : active.filter((x) => x !== w)));
  const show = (w: Widget) => active.includes(w);
  const widgetLabel: Record<Widget, string> = { audience: t("dash.audience"), activity: t("dash.activity"), upcoming: t("dash.upcoming"), recent: t("dash.recent"), unread: t("dash.unread") };

  return (
    <>
      <PageHeader title={t("dash.title")} subtitle={t("dash.subtitle")} actions={connected.length > 0 && <Button onClick={() => setCustomize(true)}>{t("dash.customize")}</Button>} />
      {!connected.length ? <Onboarding /> : data.loading && !data.data ? <Spinner /> : data.error ? <ErrorBox message={data.error} onRetry={data.reload} /> : data.data && (
        <div className="grid gap-4">
          {show("audience") && (
            <Panel title={t("dash.audience")}>
              <p className="mb-3 text-[12.5px] text-[var(--muted)]">{t("dash.audienceNote")}</p>
              <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-3">
                {connected.map((a) => {
                  const { metric, point, reason } = audienceFor(a, data.data!.latest);
                  return (
                    <div key={a.id}>
                      <div className="mb-1 flex items-center gap-1.5 truncate text-[12.5px]"><PlatformDot platform={a.platform} />{a.name}</div>
                      <Stat label={metricLabel(metric, t.lang)} value={point?.value ?? null} source={point?.source} day={point?.day} reason={reason ?? t("common.unavailable")} />
                    </div>
                  );
                })}
              </div>
            </Panel>
          )}
          {show("activity") && (
            <div className="grid gap-4 lg:grid-cols-2">
              {connected.slice(0, 6).map((a) => {
                const rows = data.data!.series.filter((s) => s.account_id === a.id);
                const metrics = PLATFORM_META[a.platform].primary.filter((m) => rows.some((r) => r.metric === m));
                return (
                  <Panel key={a.id} title={<span className="flex items-center gap-2"><PlatformDot platform={a.platform} />{a.name}</span>} actions={<span className="text-xs text-[var(--muted)]">{t("dash.activity")}</span>}>
                    {metrics.length ? <MetricTrend rows={rows} metrics={metrics} label={(m) => metricLabel(m, t.lang)} height={190} />
                      : <div className="hatched flex h-[120px] items-center justify-center rounded-lg text-[var(--muted)]">{t("common.unavailable")}</div>}
                  </Panel>
                );
              })}
            </div>
          )}
          <div className="grid gap-4 lg:grid-cols-3">
            {show("upcoming") && (
              <Panel title={t("dash.upcoming")}>
                {data.data.upcoming.length ? <ul className="space-y-2">{data.data.upcoming.slice(0, 6).map((u) => (
                  <li key={u.id} className="flex items-center gap-2"><PlatformDot platform={u.platform} /><span className="flex-1 truncate">{u.title}</span><span className="text-xs text-[var(--muted)]">{fmtDate(u.at, t.lang, true)}</span></li>
                ))}</ul> : <Empty title={t("dash.nothingScheduled")} action={<Link className="text-iris-soft" to="/studio">{t("nav.studio")}</Link>} />}
              </Panel>
            )}
            {show("recent") && (
              <Panel title={t("dash.recent")}>
                <ul className="space-y-2">{data.data.recent.map((c) => (
                  <li key={c.id}><button type="button" onClick={() => c.url && openExternal(c.url)} className="flex w-full items-center gap-2 text-left hover:text-iris-soft cursor-pointer">
                    <PlatformDot platform={c.platform} /><span className="flex-1 truncate">{c.title ?? c.kind}</span><span className="text-xs text-[var(--muted)]">{fmtDate(c.published_at, t.lang)}</span>
                  </button></li>
                ))}</ul>
                {!data.data.recent.length && <p className="text-[var(--muted)]">{t("pf.noContent")}</p>}
              </Panel>
            )}
            {show("unread") && (
              <Panel title={t("dash.unread")} actions={<Link to="/notifications" className="text-xs text-iris-soft">{t("nav.notifications")}</Link>}>
                {data.data.unread.length ? <ul className="space-y-2">{data.data.unread.map((n) => (
                  <li key={n.id} className="text-[13px]"><div className="flex items-center gap-2">{n.platform && <PlatformDot platform={n.platform} />}<span className="font-medium">{n.title}</span></div>{n.body && <div className="truncate text-[var(--muted)]">{n.body}</div>}</li>
                ))}</ul> : <p className="text-[var(--muted)]">{t("no.empty")}</p>}
              </Panel>
            )}
          </div>
          <div className="flex flex-wrap gap-2">{[...new Set(connected.map((a) => a.platform))].map((p) => <PlatformBadge key={p} platform={p} />)}<Badge>{connected.length} {t("nav.accounts").toLowerCase()}</Badge></div>
        </div>
      )}
      <Modal open={customize} onClose={() => setCustomize(false)} title={t("dash.widgets")}>
        <div className="flex flex-col gap-3">{WIDGETS.map((w) => <Toggle key={w} checked={show(w)} onChange={(v) => toggle(w, v)} label={widgetLabel[w]} />)}</div>
      </Modal>
    </>
  );
}
