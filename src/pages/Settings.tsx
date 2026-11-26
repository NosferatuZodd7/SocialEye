import { useState } from "react";
import { ExternalLink } from "lucide-react";
import { api, type Provider, type UpdateInfo } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { fmtDate } from "../lib/format";
import { openDialog, openExternal, saveDialog } from "../lib/files";
import { useFeedback } from "../components/feedback";
import { Badge, Button, ErrorBox, Input, PageHeader, Panel, Select, Table, Toggle } from "../components/ui";

const GUIDE = "https://github.com/NosferatuZodd7/SocialEye/blob/main/docs/API_SETUP.md";
const PROVIDERS: Record<Provider["provider"], { label: string; platforms: string; console: string; secretHint: string }> = {
  google: { label: "Google", platforms: "YouTube", console: "https://console.cloud.google.com/apis/credentials", secretHint: "Required by Google for Desktop app clients." },
  meta: { label: "Meta", platforms: "Instagram, Facebook", console: "https://developers.facebook.com/apps/", secretHint: "Needed for long-lived tokens. Without it Pages must be reconnected about every hour." },
  spotify: { label: "Spotify", platforms: "Spotify", console: "https://developer.spotify.com/dashboard", secretHint: "Not needed: Spotify uses PKCE for desktop apps." },
};

function ProviderForm({ p, onSaved }: { p: Provider; onSaved: () => void }) {
  const t = useT();
  const { toast } = useFeedback();
  const meta = PROVIDERS[p.provider];
  const [f, setF] = useState({ client_id: p.app?.client_id ?? "", secret: "", clear: false, port: p.app?.redirect_port ?? 53682, version: p.app?.api_version ?? "" });
  const act = useAction();
  const save = () => act.run(async () => {
    await api.saveProvider({ provider: p.provider, client_id: f.client_id, client_secret: f.clear ? "" : f.secret || null, redirect_port: f.port, api_version: p.provider === "meta" ? f.version || null : null });
    setF({ ...f, secret: "", clear: false }); toast(t("common.saved")); onSaved();
  });
  return (
    <div className="rounded-xl border border-[var(--line)] p-3.5">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <span className="font-medium">{meta.label}</span><span className="text-[12.5px] text-[var(--muted)]">{meta.platforms}</span>
        {p.configured ? <Badge tone="ok">{t("common.saved")}</Badge> : <Badge>{t("common.none")}</Badge>}
        <div className="flex-1" />
        <Button variant="ghost" onClick={() => openExternal(meta.console)}><ExternalLink className="size-3.5" />Console</Button>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <Input label={t("set.clientId")} value={f.client_id} onChange={(e) => setF({ ...f, client_id: e.target.value })} autoComplete="off" spellCheck={false} />
        <Input label={t("set.clientSecret")} type="password" value={f.secret} autoComplete="off" onChange={(e) => setF({ ...f, secret: e.target.value })} placeholder={p.has_secret ? t("set.secretStored") : ""} hint={meta.secretHint} disabled={f.clear} />
        <Input label={t("set.port")} type="number" min={1024} max={65535} value={f.port} onChange={(e) => setF({ ...f, port: Number(e.target.value) })} hint={<>{t("set.redirect")}: <code className="select-all">http://127.0.0.1:{f.port}/callback</code></>} />
        {p.provider === "meta" && <Input label={t("set.graphVersion")} value={f.version} onChange={(e) => setF({ ...f, version: e.target.value })} placeholder="v24.0" />}
      </div>
      {p.has_secret && <div className="mt-2"><Toggle label={t("set.secretClear")} checked={f.clear} onChange={(v) => setF({ ...f, clear: v })} /></div>}
      {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
      <Button className="mt-3" variant="primary" busy={act.busy} disabled={!f.client_id.trim()} onClick={save}>{t("common.save")}</Button>
    </div>
  );
}

function Updates() {
  const t = useT();
  const { backend, setSetting } = useStore();
  const { confirm } = useFeedback();
  const status = useLoad(() => (backend ? api.updatesStatus() : Promise.resolve(null)), [backend]);
  const [found, setFound] = useState<UpdateInfo | null | undefined>(undefined);
  const [pct, setPct] = useState<number | null>(null);
  const act = useAction();
  const st = status.data;
  const check = () => act.run(async () => setFound(await api.updatesCheck()));
  const install = (u: UpdateInfo) => act.run(async () => {
    if (!(await confirm(t("set.installConfirm", { version: u.version }), { confirmLabel: t("set.install") }))) return;
    const { listen } = await import("@tauri-apps/api/event");
    const off = await listen<{ received: number; total: number | null }>("update-progress", (e) => setPct(e.payload.total ? Math.round((e.payload.received / e.payload.total) * 100) : null));
    try { await api.updatesInstall(); } finally { off(); }
  });
  return (
    <Panel title={t("set.updates")}>
      <p className="mb-3 max-w-[75ch] text-[var(--muted)]">{t("set.updatesBody")}</p>
      {status.error && <ErrorBox message={status.error} />}
      {st && (
        <>
          <dl className="mb-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">
            <dt className="text-[var(--muted)]">{t("set.currentVersion")}</dt>
            <dd className="num">{st.current}{st.current.includes("-") && <span className="ml-2"><Badge tone="warn">{t("set.testBuild")}</Badge></span>}</dd>
            <dt className="text-[var(--muted)]">GitHub</dt><dd>{st.repository}</dd>
          </dl>
          {!st.enabled ? <p className="hatched rounded-lg p-3 text-[13px]">{t("set.updatesDisabled")}</p> : (
            <>
              <div className="grid gap-3 md:grid-cols-2">
                <Select label={t("set.channel")} value={st.channel} hint={t("set.channelHint")} onChange={(e) => act.run(async () => { await setSetting("update_channel", e.target.value); setFound(undefined); await status.reload(); })}>
                  <option value="testing">{t("set.channel.testing")}</option><option value="stable">{t("set.channel.stable")}</option>
                </Select>
                <div className="self-center"><Toggle label={t("set.autoCheck")} checked={st.auto_check} onChange={(v) => act.run(async () => { await setSetting("update_auto_check", v ? "1" : "0"); await status.reload(); })} /></div>
              </div>
              <div className="mt-3 flex flex-wrap items-center gap-2">
                <Button busy={act.busy && !pct} onClick={check}>{t("set.checkNow")}</Button>
                {found === null && <span className="text-[13px] text-ok">{t("set.upToDate")}</span>}
              </div>
              {found && (
                <div className="mt-3 rounded-xl border border-iris/40 p-3">
                  <div className="flex flex-wrap items-center gap-2 font-medium">{t("set.available", { version: found.version })}{found.prerelease && <Badge tone="warn">{t("set.testBuild")}</Badge>}</div>
                  {found.notes && <details className="mt-1 text-[13px]"><summary className="cursor-pointer text-[var(--muted)]">{t("set.releaseNotes")}</summary><p className="mt-1 whitespace-pre-line">{found.notes}</p></details>}
                  <div className="mt-2 flex gap-2">
                    <Button variant="primary" busy={act.busy} onClick={() => install(found)}>{t("set.install")}</Button>
                    <Button variant="ghost" onClick={() => openExternal(found.html_url)}><ExternalLink className="size-3.5" />GitHub</Button>
                  </div>
                  {pct != null && <div className="mt-2" role="progressbar" aria-valuenow={pct} aria-valuemin={0} aria-valuemax={100}><div className="mb-1 text-xs">{t("set.downloading", { pct })}</div><div className="h-1.5 rounded bg-[var(--panel-2)]"><div className="h-1.5 rounded bg-iris" style={{ width: `${pct}%` }} /></div></div>}
                </div>
              )}
            </>
          )}
          {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
        </>
      )}
    </Panel>
  );
}

export default function SettingsPage() {
  const t = useT();
  const { settings, setSetting, backend } = useStore();
  const { confirm, toast } = useFeedback();
  const providers = useLoad(() => (backend ? api.providers() : Promise.resolve([])), [backend]);
  const info = useLoad(() => (backend ? api.appInfo() : Promise.resolve(null)), [backend]);
  const audit = useLoad(() => (backend ? api.auditLog() : Promise.resolve([])), [backend]);
  const act = useAction();
  const stamp = new Date().toISOString().slice(0, 10);

  const backup = () => act.run(async () => { const p = await saveDialog(`socialeye-backup-${stamp}.db`, "db", "SocialEye backup"); if (p) { await api.backup(p); toast(t("set.backupDone")); } });
  const restore = () => act.run(async () => {
    const [p] = await openDialog({ extensions: ["db"], label: "SocialEye backup" });
    if (p && (await confirm(t("set.restoreConfirm"), { danger: true }))) { await api.restore(p); toast(t("set.restoreDone")); setTimeout(() => location.reload(), 600); }
  });
  const exportJson = () => act.run(async () => { const p = await saveDialog(`socialeye-export-${stamp}.json`, "json", "JSON"); if (p) { await api.exportJson(p); toast(t("set.exportDone")); } });

  return (
    <>
      <PageHeader title={t("set.title")} />
      <div className="grid gap-4">
        <Panel title={t("set.general")}>
          <div className="grid gap-3 md:grid-cols-3">
            <Select label={t("set.language")} value={settings.language ?? ""} onChange={(e) => setSetting("language", e.target.value)}>
              <option value="">{t("set.theme.system")}</option><option value="en">English</option><option value="pt">Português (Portugal)</option>
            </Select>
            <Select label={t("set.theme")} value={settings.theme ?? "system"} onChange={(e) => setSetting("theme", e.target.value)}>
              <option value="system">{t("set.theme.system")}</option><option value="light">{t("set.theme.light")}</option><option value="dark">{t("set.theme.dark")}</option>
            </Select>
            <Select label={t("set.syncInterval")} value={settings.sync_interval_minutes ?? "360"} onChange={(e) => act.run(() => setSetting("sync_interval_minutes", e.target.value))}>
              <option value="0">{t("set.syncOff")}</option>
              {[1, 3, 6, 12, 24].map((h) => <option key={h} value={h * 60}>{t("set.hours", { n: h })}</option>)}
            </Select>
          </div>
          <div className="mt-3"><Toggle label={t("set.desktopNotif")} checked={settings.desktop_notifications !== "0"} onChange={(v) => setSetting("desktop_notifications", v ? "1" : "0")} /></div>
        </Panel>
        <Panel title={t("set.apps")} actions={<Button variant="ghost" onClick={() => openExternal(GUIDE)}><ExternalLink className="size-3.5" />{t("set.guide")}</Button>}>
          <p className="mb-3 max-w-[75ch] text-[var(--muted)]">{t("set.appsBody")}</p>
          {providers.error && <ErrorBox message={providers.error} />}
          <div className="grid gap-3">{providers.data?.map((p) => <ProviderForm key={p.provider} p={p} onSaved={providers.reload} />)}</div>
        </Panel>
        <Updates />
        <Panel title={t("set.data")}>
          <div className="flex flex-wrap gap-2">
            <Button onClick={backup} disabled={!backend}>{t("set.backup")}</Button>
            <Button onClick={restore} disabled={!backend}>{t("set.restore")}</Button>
            <Button onClick={exportJson} disabled={!backend}>{t("set.exportJson")}</Button>
          </div>
          {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
          {info.data && <p className="mt-3 text-[12.5px] text-[var(--muted)]">{t("set.dataDir")}: <code className="select-all">{info.data.data_dir}</code></p>}
        </Panel>
        <div className="grid gap-4 md:grid-cols-2">
          <Panel title={t("set.privacy")}><p className="text-[var(--muted)]">{t("set.privacyBody")}</p></Panel>
          <Panel title={t("set.about")}>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[13px]">
              <dt className="text-[var(--muted)]">{t("set.version")}</dt><dd className="num">{info.data?.version ?? "—"}</dd>
              <dt className="text-[var(--muted)]">{t("set.schema")}</dt><dd className="num">{info.data?.schema_version ?? "—"}</dd>
              <dt className="text-[var(--muted)]">License</dt><dd>MIT</dd>
            </dl>
          </Panel>
        </div>
        <Panel title={t("set.audit")}>
          <Table head={[t("common.date"), t("common.actions"), "Target", ""]} empty={!audit.data?.length}>
            {audit.data?.slice(0, 100).map((a) => <tr key={a.id}><td className="whitespace-nowrap">{fmtDate(a.at, t.lang, true)}</td><td>{a.action}</td><td>{a.target}</td><td className="max-w-md truncate text-[var(--muted)]">{a.detail}</td></tr>)}
          </Table>
        </Panel>
      </div>
    </>
  );
}
