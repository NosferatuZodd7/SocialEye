import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { ExternalLink } from "lucide-react";
import { api, parseJson, type Account, type Comment, type ContentItem, type Platform } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { daysAgo, fmtDate, fmtDuration, fmtNumber, isoDay } from "../lib/format";
import { KIND_LABELS, PLATFORM_META, STOCK_METRICS, metricLabel, summarize } from "../lib/platforms";
import { openExternal } from "../lib/files";
import { useFeedback } from "../components/feedback";
import { Bars, MetricTrend, PLATFORM_HEX, TrendChart } from "../components/charts";
import { Badge, Button, Empty, ErrorBox, Input, Modal, PageHeader, Panel, Select, Spinner, Stat, Table, Tabs, Textarea } from "../components/ui";
import { CapabilityList } from "./Accounts";
import { InstagramFollowers } from "./InstagramFollowers";
import { SpotifyImport } from "./SpotifyImport";

export const STAT_COLUMNS: Record<Platform, string[]> = {
  youtube: ["views", "likes", "comments", "avg_view_pct"],
  instagram: ["likes", "comments"],
  facebook: ["reactions", "comments", "shares"],
  spotify: [],
};

export const stats = (c: ContentItem) => parseJson<Record<string, number | string>>(c.stats_json, {});

/** The platform's own performance measure for a content item (never mixed across platforms). */
export function performance(c: ContentItem): number | null {
  const s = stats(c);
  if (c.platform === "youtube") return typeof s.views === "number" ? s.views : null;
  const parts = ["likes", "comments", "reactions", "shares"].map((k) => s[k]).filter((v): v is number => typeof v === "number");
  return parts.length ? parts.reduce((a, b) => a + b, 0) : null;
}

const RANGES = [7, 28, 90, 365] as const;

function Overview({ account }: { account: Account }) {
  const t = useT();
  const [range, setRange] = useState<number>(28);
  const from = daysAgo(range), to = isoDay(new Date());
  const d = useLoad(async () => {
    const latest = (await api.latest()).filter((m) => m.account_id === account.id);
    const flows = [...new Set([...PLATFORM_META[account.platform].primary, ...latest.map((m) => m.metric).filter((m) => !STOCK_METRICS.has(m))])];
    const [series, content, bd] = await Promise.all([
      api.series({ account_ids: [account.id], metrics: [...flows, PLATFORM_META[account.platform].audience], from, to }),
      api.content({ account_id: account.id, limit: 5000 }),
      account.platform === "youtube" ? api.breakdowns(account.id) : Promise.resolve([]),
    ]);
    return { latest, series, content, bd, flows };
  }, [account.id, from]);

  if (d.loading && !d.data) return <Spinner />;
  if (d.error) return <ErrorBox message={d.error} onRetry={d.reload} />;
  const { latest, series, content, bd, flows } = d.data!;
  const unavailable = account.unavailable?.split("\n") ?? [];
  const audience = PLATFORM_META[account.platform].audience;
  const audienceRow = latest.find((m) => m.metric === audience);
  const stocks = latest.filter((m) => STOCK_METRICS.has(m.metric) && m.metric !== audience);
  const presentFlows = flows.filter((m) => series.some((s) => s.metric === m));
  const ranked = content.map((c) => ({ c, p: performance(c) })).filter((x) => x.p != null).sort((a, b) => b.p! - a.p!);
  const byKind = Object.entries(content.reduce<Record<string, number>>((acc, c) => ({ ...acc, [c.kind]: (acc[c.kind] ?? 0) + 1 }), {}));
  const byMonth = account.platform === "spotify"
    ? Object.entries(content.reduce<Record<string, number>>((acc, c) => { const k = (c.published_at ?? "").slice(0, 7); if (k) acc[k] = (acc[k] ?? 0) + 1; return acc; }, {})).sort().slice(-18)
    : [];

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap items-center gap-1" role="group" aria-label="Range">
        {RANGES.map((r) => <Button key={r} variant={r === range ? "primary" : "ghost"} onClick={() => setRange(r)}>{t(`common.range.${r}` as const)}</Button>)}
      </div>
      <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-3">
        <Stat label={metricLabel(audience, t.lang)} value={audienceRow?.value ?? null} source={audienceRow?.source} day={audienceRow?.day} reason={unavailable.find((u) => u.startsWith(audience)) ?? t("common.unavailable")} />
        {stocks.map((m) => <Stat key={m.metric} label={metricLabel(m.metric, t.lang)} value={m.value} source={m.source} day={m.day} />)}
        {presentFlows.map((m) => <Stat key={m} label={`${metricLabel(m, t.lang)} (${t(`common.range.${range}` as "common.range.7")})`} value={summarize(m, series.filter((x) => x.metric === m).map((x) => x.value))} source={series.find((x) => x.metric === m)?.source} />)}
      </div>
      <Panel title={t("pf.trend")}>
        {presentFlows.length ? <MetricTrend rows={series} metrics={[...presentFlows, ...(series.some((s) => s.metric === audience) ? [audience] : [])]} label={(m) => metricLabel(m, t.lang)} color={PLATFORM_HEX[account.platform]} />
          : <div className="hatched flex h-[160px] items-center justify-center rounded-lg px-6 text-center text-[var(--muted)]">{account.platform === "spotify" ? t("sp.csvBody") : t("an.noData")}</div>}
      </Panel>
      {unavailable.length > 0 && (
        <div className="hatched rounded-xl border border-[var(--line)] p-3 text-[12.5px]">
          <div className="mb-1 font-medium">{t("acc.unavailableTitle")}</div>
          <ul className="list-inside list-disc text-[var(--muted)]">{unavailable.map((u) => <li key={u}>{u}</li>)}</ul>
        </div>
      )}
      <div className="grid gap-4 lg:grid-cols-2">
        {byKind.length > 0 && (
          <Panel title={t("pf.kind")}>
            <Bars data={byKind.map(([k, n]) => ({ name: KIND_LABELS[k]?.[t.lang] ?? k, items: n }))} series={[{ key: "items", label: t("pf.content"), color: PLATFORM_HEX[account.platform] }]} height={200} />
            {account.platform === "youtube" && <p className="mt-2 text-[12px] text-[var(--muted)]">{t("pf.shortsNote")}</p>}
          </Panel>
        )}
        {byMonth.length > 0 && (
          <Panel title={t("sp.releaseCalendar")}>
            <Bars data={byMonth.map(([m, n]) => ({ name: m, items: n }))} series={[{ key: "items", label: t("sp.episodes"), color: PLATFORM_HEX.spotify }]} height={200} />
          </Panel>
        )}
        {ranked.length > 1 && (
          <Panel title={`${t("pf.best")} / ${t("pf.worst")}`}>
            <RankList title={t("pf.best")} items={ranked.slice(0, 5)} />
            <div className="h-3" />
            <RankList title={t("pf.worst")} items={ranked.slice(-5).reverse()} />
          </Panel>
        )}
        {bd.map((b) => <Breakdown key={b.kind} kind={b.kind} rows={parseJson<Record<string, string | number>[]>(b.data_json, [])} source={`${b.source}, ${b.period_start} – ${b.period_end}`} />)}
      </div>
    </div>
  );
}

function RankList({ title, items }: { title: string; items: { c: ContentItem; p: number | null }[] }) {
  const t = useT();
  return (
    <div>
      <div className="mb-1 text-[12.5px] text-[var(--muted)]">{title}</div>
      <ol className="space-y-1">{items.map(({ c, p }) => (
        <li key={c.id} className="flex items-center gap-2 text-[13px]"><span className="flex-1 truncate">{c.title ?? c.kind}</span><span className="num">{fmtNumber(p, t.lang)}</span></li>
      ))}</ol>
    </div>
  );
}

function Breakdown({ kind, rows, source }: { kind: string; rows: Record<string, string | number>[]; source: string }) {
  const t = useT();
  const title = { traffic_sources: t("pf.traffic"), demographics: t("pf.demographics"), geography: t("pf.geography") }[kind] ?? kind;
  const data = kind === "traffic_sources" ? rows.map((r) => ({ name: String(r.insightTrafficSourceType).replace(/_/g, " ").toLowerCase(), views: Number(r.views) })).sort((a, b) => b.views - a.views).slice(0, 10)
    : kind === "geography" ? rows.map((r) => ({ name: String(r.country), views: Number(r.views) })).sort((a, b) => b.views - a.views).slice(0, 10)
    : rows.map((r) => ({ name: `${String(r.ageGroup).replace("age", "")} ${r.gender}`, percent: Number(r.viewerPercentage) }));
  return (
    <Panel title={title}>
      {data.length ? <Bars data={data} series={[kind === "demographics" ? { key: "percent", label: "% viewers" } : { key: "views", label: metricLabel("views", t.lang) }]} horizontal height={Math.max(160, data.length * 26)} />
        : <div className="hatched rounded-lg p-6 text-center text-[var(--muted)]">{t("common.unavailable")}</div>}
      <p className="mt-2 text-[11.5px] text-[var(--muted)]">{t("common.source")}: {source}</p>
    </Panel>
  );
}

function ContentTab({ account }: { account: Account }) {
  const t = useT();
  const [f, setF] = useState({ kind: "", from: "", to: "", search: "", sort: "newest" });
  const [selected, setSelected] = useState<number[]>([]);
  const [detail, setDetail] = useState<ContentItem | null>(null);
  const [compare, setCompare] = useState(false);
  const list = useLoad(() => api.content({ account_id: account.id, kind: f.kind || undefined, from: f.from || undefined, to: f.to || undefined, search: f.search || undefined, sort: f.sort }), [account.id, f.kind, f.from, f.to, f.search, f.sort]);
  const cols = STAT_COLUMNS[account.platform];
  const kinds = useMemo(() => Object.keys(KIND_LABELS), []);
  const set = (k: keyof typeof f) => (e: { target: { value: string } }) => setF((x) => ({ ...x, [k]: e.target.value }));
  const toggle = (id: number) => setSelected((s) => (s.includes(id) ? s.filter((x) => x !== id) : s.length < 4 ? [...s, id] : s));
  const chosen = list.data?.filter((c) => selected.includes(c.id)) ?? [];

  return (
    <Panel>
      <div className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-6">
        <Select label={t("pf.kind")} value={f.kind} onChange={set("kind")}><option value="">{t("common.all")}</option>{kinds.map((k) => <option key={k} value={k}>{KIND_LABELS[k][t.lang]}</option>)}</Select>
        <Input label={t("common.from")} type="date" value={f.from} onChange={set("from")} />
        <Input label={t("common.to")} type="date" value={f.to} onChange={set("to")} />
        <div className="col-span-2"><Input label={t("common.search")} value={f.search} onChange={set("search")} /></div>
        <Select label={t("common.sort")} value={f.sort} onChange={set("sort")}>
          <option value="newest">{t("common.newest")}</option><option value="oldest">{t("common.oldest")}</option>
          {account.platform === "youtube" && <option value="views">{t("common.mostViews")}</option>}<option value="engagement">{t("common.mostEngagement")}</option>
        </Select>
      </div>
      <div className="mb-2 flex items-center gap-2 text-[12.5px] text-[var(--muted)]">
        {t("pf.compareHint")} <Button disabled={selected.length < 2} onClick={() => setCompare(true)}>{t("pf.compare")} ({selected.length})</Button>
      </div>
      {list.error ? <ErrorBox message={list.error} onRetry={list.reload} /> : list.loading && !list.data ? <Spinner /> : !list.data?.length ? <Empty title={t("pf.noContent")} /> : (
        <Table head={["", "", t("st.draftTitle"), t("common.date"), t("common.type"), ...cols.map((c) => c === "avg_view_pct" ? "Avg. % viewed" : metricLabel(c, t.lang)), account.platform === "spotify" || account.platform === "youtube" ? "Duration" : ""]}>
          {list.data.map((c) => {
            const s = stats(c);
            return (
              <tr key={c.id} className="hover:bg-[var(--panel-2)]/50">
                <td><input type="checkbox" aria-label={`Select ${c.title}`} checked={selected.includes(c.id)} onChange={() => toggle(c.id)} /></td>
                <td>{c.thumbnail_url ? <img src={c.thumbnail_url} alt="" loading="lazy" className="h-9 w-14 rounded object-cover" /> : <div className="h-9 w-14 rounded bg-[var(--panel-2)]" />}</td>
                <td className="max-w-[360px]"><button type="button" onClick={() => setDetail(c)} className="line-clamp-2 text-left hover:text-iris-soft cursor-pointer">{c.title ?? c.kind}</button></td>
                <td className="whitespace-nowrap">{fmtDate(c.published_at, t.lang)}</td>
                <td><Badge tone={c.kind === "short" ? "warn" : "neutral"}>{KIND_LABELS[c.kind]?.[t.lang] ?? c.kind}</Badge></td>
                {cols.map((k) => <td key={k} className="num">{typeof s[k] === "number" ? (k === "avg_view_pct" ? `${(s[k] as number).toFixed(1)}%` : fmtNumber(s[k] as number, t.lang)) : <span className="text-[var(--muted)]">—</span>}</td>)}
                <td className="num">{c.duration_secs != null ? fmtDuration(c.duration_secs) : ""}</td>
              </tr>
            );
          })}
        </Table>
      )}
      {detail && <ContentDetail item={detail} account={account} onClose={() => setDetail(null)} />}
      <Modal open={compare} onClose={() => setCompare(false)} title={t("pf.compare")} wide>
        {cols.length ? <Bars data={cols.map((k) => Object.fromEntries([["name", metricLabel(k, t.lang)], ...chosen.map((c) => [String(c.id), Number(stats(c)[k] ?? 0)])]))}
          series={chosen.map((c) => ({ key: String(c.id), label: (c.title ?? c.kind).slice(0, 40) }))} height={320} />
          : <Table head={[t("st.draftTitle"), t("common.date"), "Duration"]}>{chosen.map((c) => <tr key={c.id}><td>{c.title}</td><td>{fmtDate(c.published_at, t.lang)}</td><td>{fmtDuration(c.duration_secs)}</td></tr>)}</Table>}
      </Modal>
    </Panel>
  );
}

function ContentDetail({ item, account, onClose }: { item: ContentItem; account: Account; onClose: () => void }) {
  const t = useT();
  const s = stats(item);
  const metrics = Object.keys(s).filter((k) => typeof s[k] === "number");
  const hist = useLoad(() => api.series({ account_ids: [account.id], metrics: [...metrics, "plays", "listeners", "starts"], from: "2000-01-01", to: isoDay(new Date()), content_id: item.id }), [item.id]);
  const [retention, setRetention] = useState<{ x: number; y: number }[] | null>(null);
  const ret = useAction();
  return (
    <Modal open onClose={onClose} title={item.title ?? item.kind} wide>
      <div className="grid gap-4 md:grid-cols-[220px_1fr]">
        <div>
          {item.thumbnail_url && <img src={item.thumbnail_url} alt="" className="mb-2 w-full rounded-lg" />}
          <div className="text-[12.5px] text-[var(--muted)]">{fmtDate(item.published_at, t.lang, true)}</div>
          {item.url && <Button className="mt-2" onClick={() => openExternal(item.url!)}><ExternalLink className="size-3.5" />{t("common.open")}</Button>}
        </div>
        <div className="min-w-0">
          <div className="mb-3 grid grid-cols-2 gap-2 md:grid-cols-4">
            {metrics.map((k) => <Stat key={k} label={k === "avg_view_pct" ? "Avg. % viewed" : metricLabel(k, t.lang)} value={s[k] as number} />)}
            {item.duration_secs != null && <Stat label="Duration (s)" value={item.duration_secs} />}
          </div>
          {item.description && <p className="mb-3 max-h-32 overflow-y-auto text-[13px] whitespace-pre-line text-[var(--muted)]">{item.description}</p>}
          <h3 className="mb-1 text-[13px] font-medium">{t("pf.history")}</h3>
          {hist.data?.length ? <MetricTrend rows={hist.data} metrics={[...new Set(hist.data.map((r) => r.metric))]} label={(m) => metricLabel(m, t.lang)} height={180} />
            : <div className="hatched rounded-lg p-4 text-center text-[12.5px] text-[var(--muted)]">{t("common.unavailable")}</div>}
          {account.platform === "youtube" && (
            <div className="mt-3">
              <h3 className="mb-1 text-[13px] font-medium">{t("pf.retention")}</h3>
              {retention ? <TrendChart data={retention.map((r) => ({ day: `${Math.round(r.x * 100)}%`, ratio: Math.round(r.y * 1000) / 10 }))} series={[{ key: "ratio", label: "% of viewers still watching" }]} height={180} />
                : <Button busy={ret.busy} onClick={() => ret.run(async () => setRetention((await api.retention(account.id, item.external_id)).map((r) => ({ x: r.elapsedVideoTimeRatio, y: r.audienceWatchRatio }))))}>{t("pf.loadRetention")}</Button>}
              {ret.error && <ErrorBox message={ret.error} />}
            </div>
          )}
          {account.platform === "youtube" && <EditVideo account={account} item={item} />}
          {account.platform !== "spotify" && <Comments account={account} item={item} />}
        </div>
      </div>
    </Modal>
  );
}

function EditVideo({ account, item }: { account: Account; item: ContentItem }) {
  const t = useT();
  const { confirm, toast } = useFeedback();
  const [f, setF] = useState<{ title: string; description: string; privacy: string } | null>(null);
  const act = useAction();
  const save = () => f && act.run(async () => {
    if (!(await confirm(t("pf.editConfirm")))) return;
    await api.updateVideo(account.id, item.external_id, f.title, f.description, f.privacy);
    toast(t("common.saved")); setF(null);
  });
  if (!f) return <Button className="mt-3" onClick={() => setF({ title: item.title ?? "", description: item.description ?? "", privacy: String(stats(item).privacy ?? "public") })}>{t("pf.editVideo")}</Button>;
  return (
    <div className="mt-3 grid gap-2 rounded-lg border border-[var(--line)] p-3">
      <Input label={`${t("st.draftTitle")} (${f.title.length}/100)`} value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} />
      <Textarea label={t("st.body")} value={f.description} onChange={(e) => setF({ ...f, description: e.target.value })} />
      <Select label={t("st.privacy")} value={f.privacy} onChange={(e) => setF({ ...f, privacy: e.target.value })}>
        <option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option>
      </Select>
      {act.error && <ErrorBox message={act.error} />}
      <div className="flex gap-2"><Button variant="primary" busy={act.busy} onClick={save}>{t("common.save")}</Button><Button variant="ghost" onClick={() => setF(null)}>{t("common.cancel")}</Button></div>
    </div>
  );
}

function Comments({ account, item }: { account: Account; item: ContentItem }) {
  const t = useT();
  const { confirm } = useFeedback();
  const [list, setList] = useState<Comment[] | null>(null);
  const [reply, setReply] = useState<{ id: string; text: string } | null>(null);
  const act = useAction();
  const load = () => act.run(async () => setList(await api.comments(account.id, item.external_id)));
  const mod = (c: Comment, action: "hide" | "unhide" | "delete") => act.run(async () => {
    if (action === "delete" && !(await confirm(t("pf.deleteComment"), { danger: true, confirmLabel: t("common.delete") }))) return;
    await api.moderate(account.id, c.id, { action }, action === "delete");
    await load();
  });
  const send = () => reply && act.run(async () => { await api.moderate(account.id, reply.id, { action: "reply", text: reply.text }); setReply(null); await load(); });
  return (
    <div className="mt-4">
      <div className="mb-1 flex items-center justify-between"><h3 className="text-[13px] font-medium">{t("pf.comments")}</h3><Button busy={act.busy} onClick={load}>{list ? t("common.refresh") : t("pf.loadComments")}</Button></div>
      {act.error && <ErrorBox message={act.error} />}
      {list && !list.length && <p className="text-[var(--muted)]">{t("pf.noComments")}</p>}
      <ul className="divide-y divide-[var(--line)]">
        {list?.map((c) => (
          <li key={c.id} className="py-2 text-[13px]">
            <div className="flex items-center gap-2"><span className="font-medium">{c.author ?? "—"}</span><span className="text-xs text-[var(--muted)]">{fmtDate(c.published_at, t.lang, true)}</span>{c.hidden && <Badge tone="warn">hidden</Badge>}</div>
            <p className="whitespace-pre-line">{c.text}</p>
            <div className="mt-1 flex gap-1">
              <Button variant="ghost" onClick={() => mod(c, c.hidden ? "unhide" : "hide")}>{c.hidden ? t("pf.unhide") : t("pf.hide")}</Button>
              <Button variant="ghost" onClick={() => setReply({ id: c.id, text: "" })}>{t("pf.reply")}</Button>
              {account.platform !== "youtube" && <Button variant="ghost" onClick={() => mod(c, "delete")}>{t("common.delete")}</Button>}
            </div>
            {reply?.id === c.id && (
              <div className="mt-1 flex items-end gap-2">
                <div className="flex-1"><Input label={t("pf.reply")} placeholder={t("pf.replyPlaceholder")} value={reply.text} onChange={(e) => setReply({ id: c.id, text: e.target.value })} /></div>
                <Button variant="primary" disabled={!reply.text.trim()} onClick={send}>{t("pf.send")}</Button>
              </div>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

export default function PlatformPage({ platform }: { platform: Platform }) {
  const t = useT();
  const { accounts } = useStore();
  const [params, setParams] = useSearchParams();
  const list = accounts.filter((a) => a.platform === platform && (platform !== "spotify" || a.account_type === "podcast"));
  const id = Number(params.get("account")) || list[0]?.id;
  const account = list.find((a) => a.id === id) ?? list[0];
  type TabId = "overview" | "content" | "followers" | "import" | "caps";
  const tab = (params.get("tab") as TabId) || (platform === "instagram" && !list.length ? "followers" : "overview");
  const setTab = (v: TabId) => { params.set("tab", v); setParams(params, { replace: true }); };
  const caps = useLoad(() => api.capabilities(platform), [platform]);
  useEffect(() => { if (account && params.get("account") !== String(account.id) && params.get("account")) setParams({ account: String(account.id) }); }, [account, params, setParams]);

  const tabs: { id: TabId; label: string }[] = [
    ...(account ? [{ id: "overview" as const, label: t("pf.overview") }, { id: "content" as const, label: platform === "spotify" ? t("sp.episodes") : t("pf.content") }] : []),
    ...(platform === "instagram" ? [{ id: "followers" as const, label: t("ig.followersTab") }] : []),
    ...(platform === "spotify" && account ? [{ id: "import" as const, label: t("sp.csvImport") }] : []),
    { id: "caps", label: t("acc.capabilities") },
  ];

  return (
    <>
      <PageHeader title={PLATFORM_META[platform].label}
        subtitle={platform === "facebook" ? t("pf.personalProfile") : undefined}
        actions={<>
          {list.length > 1 && <div className="w-64"><Select label={t("common.account")} value={account?.id} onChange={(e) => setParams({ account: e.target.value, tab })}>{list.map((a) => <option key={a.id} value={a.id}>{a.name}</option>)}</Select></div>}
          {platform === "spotify" && <Button onClick={() => openExternal("https://creators.spotify.com")}><ExternalLink className="size-3.5" />{t("sp.publish")}</Button>}
        </>} />
      <Tabs value={tabs.some((x) => x.id === tab) ? tab : tabs[0].id} onChange={setTab} items={tabs} />
      {tab === "caps" ? <Panel>{caps.data ? <CapabilityList caps={caps.data} /> : caps.error ? <ErrorBox message={caps.error} /> : <Spinner />}</Panel>
        : tab === "followers" && platform === "instagram" ? <InstagramFollowers />
        : !account ? <Empty title={t("pf.noAccounts", { platform: PLATFORM_META[platform].label })} action={<Link to="/accounts" className="text-iris-soft">{t("nav.accounts")}</Link>}>{t("pf.connectHint")}</Empty>
        : tab === "content" ? <ContentTab account={account} />
        : tab === "import" ? <SpotifyImport account={account} />
        : <Overview account={account} />}
    </>
  );
}
