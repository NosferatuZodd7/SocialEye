import { useState } from "react";
import { Link } from "react-router-dom";
import { RefreshCw } from "lucide-react";
import { api, errorText, type Account, type Capability, type Platform } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { fmtDate, fmtRelative } from "../lib/format";
import { PLATFORMS, PLATFORM_META } from "../lib/platforms";
import { useFeedback } from "../components/feedback";
import { Badge, Button, Empty, ErrorBox, Input, Modal, PageHeader, Panel, PlatformDot, Table, Toggle } from "../components/ui";

const PROVIDER_LABEL = { google: "Google", meta: "Meta", spotify: "Spotify" } as const;

export function CapabilityList({ caps }: { caps: Capability[] }) {
  const t = useT();
  const tone = { available: "ok", restricted: "warn", unsupported: "bad" } as const;
  return (
    <ul className="divide-y divide-[var(--line)]">
      {caps.map((c) => (
        <li key={c.id} className="flex gap-3 py-2.5">
          <div className="w-28 shrink-0"><Badge tone={tone[c.status]}>{t(`acc.cap.${c.status}`)}</Badge></div>
          <div className="min-w-0 text-[13px]">
            <div className="font-medium">{c.id.replace(/_/g, " ")}</div>
            <div className="text-[var(--muted)]">{c.note}</div>
            {c.requirement !== "—" && <div className="text-[11.5px] text-[var(--muted)]">{c.requirement}</div>}
            {c.manual_alternative && <div className="mt-1 text-[12.5px]"><span className="text-iris-soft">{t("acc.cap.manual")}:</span> {c.manual_alternative}</div>}
          </div>
        </li>
      ))}
    </ul>
  );
}

function statusTone(s: Account["status"]) {
  return s === "connected" ? "ok" : s === "reconnect_required" ? "warn" : "neutral";
}

function AccountRow({ a, podcasts }: { a: Account; podcasts: Account[] }) {
  const t = useT();
  const { reloadAccounts, syncing } = useStore();
  const { confirm, toast } = useFeedback();
  const act = useAction();
  const [showPodcast, setShowPodcast] = useState(false);
  const isSyncing = syncing.has(a.id);

  const sync = () => act.run(async () => { const r = await api.sync(a.id); toast(t("acc.synced", { n: r.content })); await reloadAccounts(); });
  const disconnect = async () => { if (await confirm(t("acc.disconnectConfirm", { name: a.name }))) await act.run(async () => { await api.disconnect(a.id); await reloadAccounts(); }); };
  const remove = async () => { if (await confirm(t("acc.removeConfirm", { name: a.name }), { danger: true, confirmLabel: t("acc.remove") })) await act.run(async () => { await api.removeAccount(a.id); await reloadAccounts(); }); };
  const reconnect = () => act.run(async () => { await api.connect(a.platform); await reloadAccounts(); });

  return (
    <div className="rounded-xl border border-[var(--line)] p-3.5">
      <div className="flex flex-wrap items-center gap-3">
        {a.avatar_url ? <img src={a.avatar_url} alt="" className="size-10 rounded-full object-cover" /> : <div className="flex size-10 items-center justify-center rounded-full bg-[var(--panel-2)]"><PlatformDot platform={a.platform} /></div>}
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2 font-medium">{a.name}<Badge tone={statusTone(a.status)}>{t(`acc.status.${a.status}`)}</Badge><Badge>{a.account_type}</Badge></div>
          <div className="text-[12.5px] text-[var(--muted)]">
            {a.handle && <>{a.account_type === "podcast" ? a.handle : `@${a.handle.replace(/^@/, "")}`} — </>}{t("acc.lastSync")}: {a.last_sync_at ? fmtRelative(a.last_sync_at, t.lang) : t("common.never")}
            {a.token_expires_at && <> — {t("acc.tokenExpires")}: {fmtDate(new Date(a.token_expires_at * 1000).toISOString(), t.lang, true)}</>}
          </div>
        </div>
        <Toggle checked={!!a.auto_sync} onChange={(v) => act.run(async () => { await api.setAutoSync(a.id, v); await reloadAccounts(); })} label={t("acc.autoSync")} />
        {a.status === "reconnect_required" || a.status === "disconnected"
          ? !a.parent_account_id && <Button variant="primary" busy={act.busy} onClick={reconnect}>{t("acc.reconnect")}</Button>
          : <Button busy={isSyncing || act.busy} onClick={sync}><RefreshCw className="size-3.5" />{isSyncing ? t("acc.syncing") : t("acc.sync")}</Button>}
        {a.platform === "spotify" && a.account_type === "user" && <Button onClick={() => setShowPodcast(true)}>{t("acc.addPodcast")}</Button>}
        {a.status !== "disconnected" && !a.parent_account_id && <Button variant="ghost" onClick={disconnect}>{t("acc.disconnect")}</Button>}
        <Button variant="ghost" onClick={remove}>{t("acc.remove")}</Button>
      </div>
      {a.last_error && <div className="mt-2"><ErrorBox message={a.last_error} /></div>}
      {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
      {a.unavailable && (
        <details className="mt-2 text-[12.5px]">
          <summary className="cursor-pointer text-[var(--muted)]">{t("acc.unavailableTitle")} ({a.unavailable.split("\n").length})</summary>
          <ul className="hatched mt-1 list-inside list-disc rounded-lg p-2">{a.unavailable.split("\n").map((l) => <li key={l}>{l}</li>)}</ul>
        </details>
      )}
      {podcasts.length > 0 && <div className="mt-3 ml-12 space-y-2">{podcasts.map((p) => <AccountRow key={p.id} a={p} podcasts={[]} />)}</div>}
      {showPodcast && <PodcastModal parent={a} onClose={() => setShowPodcast(false)} />}
    </div>
  );
}

function PodcastModal({ parent, onClose }: { parent: Account; onClose: () => void }) {
  const t = useT();
  const { reloadAccounts } = useStore();
  const [url, setUrl] = useState("");
  const act = useAction();
  const saved = useLoad(() => api.savedShows(parent.id), [parent.id]);
  const add = (show: string) => act.run(async () => { await api.addPodcast(parent.id, show); await reloadAccounts(); onClose(); });
  return (
    <Modal open onClose={onClose} title={t("acc.addPodcast")}>
      <form className="flex items-end gap-2" onSubmit={(e) => { e.preventDefault(); void add(url); }}>
        <div className="flex-1"><Input label={t("acc.podcastUrl")} value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://open.spotify.com/show/…" /></div>
        <Button variant="primary" type="submit" busy={act.busy} disabled={!url}>{t("common.add")}</Button>
      </form>
      {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
      <h3 className="mt-4 mb-2 text-[13px] font-medium">{t("acc.savedShows")}</h3>
      {saved.error ? <ErrorBox message={saved.error} /> : (
        <ul className="max-h-64 space-y-1 overflow-y-auto">{saved.data?.map((s) => (
          <li key={s.external_id}><button type="button" onClick={() => add(s.external_id)} className="flex w-full items-center gap-2 rounded p-1.5 text-left hover:bg-[var(--panel-2)] cursor-pointer">
            {s.avatar_url && <img src={s.avatar_url} alt="" className="size-8 rounded" />}<span className="flex-1">{s.name}</span><span className="text-xs text-[var(--muted)]">{s.handle}</span>
          </button></li>
        ))}</ul>
      )}
    </Modal>
  );
}

export default function Accounts() {
  const t = useT();
  const { accounts, reloadAccounts, backend } = useStore();
  const providers = useLoad(() => (backend ? api.providers() : Promise.resolve([])), [backend]);
  const connect = useAction();
  const [caps, setCaps] = useState<{ platform: Platform; list: Capability[] } | null>(null);
  const history = useLoad(() => (backend ? api.syncRuns() : Promise.resolve([])), [backend, accounts]);

  const configured = (p: Platform) => providers.data?.find((x) => x.provider === PLATFORM_META[p].provider)?.configured;
  const doConnect = (p: Platform) => connect.run(async () => { await api.connect(p); await reloadAccounts(); });
  const showCaps = async (p: Platform) => { try { setCaps({ platform: p, list: await api.capabilities(p) }); } catch (e) { connect.setError(errorText(e)); } };
  const roots = accounts.filter((a) => !a.parent_account_id);

  return (
    <>
      <PageHeader title={t("acc.title")} subtitle={t("acc.subtitle")} />
      <div className="mb-4 grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        {PLATFORMS.map((p) => (
          <div key={p} className="rounded-xl border border-[var(--line)] bg-[var(--panel)] p-3.5">
            <div className="mb-2 flex items-center gap-2 font-medium"><PlatformDot platform={p} />{PLATFORM_META[p].label}</div>
            {configured(p) ? <Button variant="primary" busy={connect.busy} onClick={() => doConnect(p)}>{t("acc.connect", { platform: PLATFORM_META[p].label })}</Button>
              : <p className="text-[12.5px] text-[var(--muted)]">{t("acc.notConfigured", { provider: PROVIDER_LABEL[PLATFORM_META[p].provider] })} <Link to="/settings" className="text-iris-soft">{t("nav.settings")}</Link></p>}
            <button type="button" onClick={() => showCaps(p)} className="mt-2 block text-[12.5px] text-[var(--muted)] underline-offset-2 hover:underline cursor-pointer">{t("acc.capabilities")}</button>
          </div>
        ))}
      </div>
      {connect.busy && <p className="mb-3 text-[13px] text-iris-soft" role="status">{t("acc.connecting")}</p>}
      {connect.error && <div className="mb-3"><ErrorBox message={connect.error} /></div>}
      <Panel title={t("nav.accounts")}>
        {roots.length ? <div className="space-y-3">{roots.map((a) => <AccountRow key={a.id} a={a} podcasts={accounts.filter((c) => c.parent_account_id === a.id)} />)}</div> : <Empty title={t("acc.empty")} />}
      </Panel>
      <Panel title={t("acc.history")} className="mt-4">
        <Table head={[t("common.account"), t("common.date"), t("common.status"), "Items", ""]} empty={!history.data?.length}>
          {history.data?.slice(0, 30).map((r) => (
            <tr key={String(r.id)}>
              <td>{String(r.name)}</td><td>{fmtDate(String(r.started_at), t.lang, true)}</td>
              <td><Badge tone={r.status === "ok" ? "ok" : r.status === "failed" ? "bad" : "neutral"}>{String(r.status)}</Badge></td>
              <td className="num">{String(r.items)}</td><td className="max-w-md truncate text-[var(--muted)]" title={String(r.error ?? "")}>{String(r.error ?? "")}</td>
            </tr>
          ))}
        </Table>
      </Panel>
      <Modal open={!!caps} onClose={() => setCaps(null)} title={caps ? `${PLATFORM_META[caps.platform].label}: ${t("acc.capabilities")}` : ""} wide>
        {caps && <CapabilityList caps={caps.list} />}
      </Modal>
    </>
  );
}
