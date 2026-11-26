import { useEffect, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { ExternalLink, Plus } from "lucide-react";
import { api, parseJson, type Draft, type Media, type Variant } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { fmtDate, fmtNumber, isoToLocal, localToIso } from "../lib/format";
import { PLATFORM_META } from "../lib/platforms";
import { fileSrc, openDialog, openExternal } from "../lib/files";
import { useFeedback } from "../components/feedback";
import { Badge, Button, Empty, ErrorBox, Input, PageHeader, Panel, PlatformBadge, PlatformDot, Select, Spinner, Table, Tabs, Textarea, cx } from "../components/ui";

const STATUS_TONE = { draft: "neutral", scheduled: "iris", publishing: "warn", published: "ok", failed: "bad" } as const;
const LIMITS: Record<string, number> = { instagram: 2200, facebook: 63206, youtube: 5000 };

function StatusBadge({ status }: { status: string }) {
  const t = useT();
  return <Badge tone={STATUS_TONE[status as keyof typeof STATUS_TONE] ?? "neutral"}>{t(`st.status.${status}` as "st.status.draft")}</Badge>;
}

function MediaThumb({ m, className }: { m: Pick<Media, "path" | "kind" | "name">; className?: string }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => { void fileSrc(m.path).then(setSrc).catch(() => setSrc(null)); }, [m.path]);
  if (!src) return <div className={cx("rounded bg-[var(--panel-2)]", className)} />;
  if (m.kind === "image") return <img src={src} alt={m.name} className={cx("rounded object-cover", className)} />;
  if (m.kind === "video") return <video src={src} className={cx("rounded object-cover", className)} muted preload="metadata" />;
  return <div className={cx("flex items-center justify-center rounded bg-[var(--panel-2)] text-xs", className)}>{m.kind}</div>;
}

function VariantEditor({ v, media, draft, onChange }: { v: Variant; media: Media[]; draft: Draft; onChange: () => void }) {
  const t = useT();
  const { confirm, toast } = useFeedback();
  const [f, setF] = useState(v);
  const [issues, setIssues] = useState<string[] | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const act = useAction();
  useEffect(() => setF(v), [v]);
  useEffect(() => {
    let off: (() => void) | undefined;
    void import("@tauri-apps/api/event").then(({ listen }) => listen<{ variant_id: number; sent: number; total: number }>("upload-progress", (e) => {
      if (e.payload.variant_id === v.id) setProgress(Math.round((e.payload.sent / Math.max(e.payload.total, 1)) * 100));
    })).then((u) => (off = u)).catch(() => {});
    return () => off?.();
  }, [v.id]);

  const locked = f.status === "published" || f.status === "publishing" || (f.status === "scheduled" && !!f.external_id);
  const save = () => api.saveVariant({ id: f.id, draft_id: f.draft_id, account_id: f.account_id, title: f.title || null, body: f.body, media_asset_id: f.media_asset_id, thumbnail_asset_id: f.thumbnail_asset_id, link: f.link || null, privacy: f.privacy, scheduled_at: f.scheduled_at });
  const check = () => act.run(async () => { await save(); setIssues(await api.validateVariant(f.id)); onChange(); });
  const publish = () => act.run(async () => {
    await save();
    const errs = await api.validateVariant(f.id);
    setIssues(errs);
    if (errs.length) return;
    const msg = f.scheduled_at
      ? `${t("st.scheduleConfirm", { when: fmtDate(f.scheduled_at, t.lang, true), account: f.account_name })}${f.platform === "instagram" ? `\n\n${t("st.localSchedule")}` : ""}`
      : t("st.publishConfirm", { account: f.account_name, platform: PLATFORM_META[f.platform].label });
    if (!(await confirm(msg, { confirmLabel: f.scheduled_at ? t("st.scheduleIt") : t("st.publishNow") }))) return;
    const r = await api.publishVariant(f.id);
    setProgress(null);
    toast(r.status === "scheduled" ? t("st.scheduled") : t("st.published"));
    onChange();
  });
  const set = <K extends keyof Variant>(k: K, val: Variant[K]) => setF((x) => ({ ...x, [k]: val }));
  const effectiveBody = f.body || draft.body;
  const limit = LIMITS[f.platform];

  return (
    <div className="rounded-xl border border-[var(--line)] p-3.5">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <PlatformBadge platform={f.platform} /><span className="font-medium">{f.account_name}</span><StatusBadge status={f.status} />
        {f.scheduled_at && <span className="text-xs text-[var(--muted)]">{fmtDate(f.scheduled_at, t.lang, true)}</span>}
        <div className="flex-1" />
        {f.external_id && f.platform === "youtube" && <Button variant="ghost" onClick={() => openExternal(`https://www.youtube.com/watch?v=${f.external_id}`)}><ExternalLink className="size-3.5" />{t("common.open")}</Button>}
        {f.external_id && f.platform === "facebook" && <Button variant="ghost" onClick={() => openExternal(`https://www.facebook.com/${f.external_id}`)}><ExternalLink className="size-3.5" />{t("common.open")}</Button>}
        {!locked && <Button variant="ghost" onClick={() => act.run(async () => { await api.deleteVariant(f.id); onChange(); })}>{t("common.remove")}</Button>}
      </div>
      <fieldset disabled={locked} className="grid gap-3 md:grid-cols-2">
        {f.platform === "youtube" && <Input label={`${t("st.titleOverride")} (${(f.title || draft.title).length}/100)`} value={f.title ?? ""} placeholder={draft.title} onChange={(e) => set("title", e.target.value)} />}
        {f.platform === "youtube" && (
          <Select label={t("st.privacy")} value={f.privacy ?? "private"} onChange={(e) => set("privacy", e.target.value)}>
            <option value="private">Private</option><option value="unlisted">Unlisted</option><option value="public">Public</option>
          </Select>
        )}
        {f.platform === "facebook" && <Input label={t("st.link")} type="url" value={f.link ?? ""} onChange={(e) => set("link", e.target.value)} placeholder="https://" />}
        <div className="md:col-span-2">
          <Textarea label={t("st.bodyOverride")} value={f.body} placeholder={draft.body} onChange={(e) => set("body", e.target.value)}
            hint={<span className={limit && effectiveBody.length > limit ? "text-bad" : ""}>{t("st.chars", { n: fmtNumber(effectiveBody.length, t.lang) })}{limit ? ` / ${fmtNumber(limit, t.lang)}` : ""}</span>} />
        </div>
        <Select label={t("st.mediaPick")} value={f.media_asset_id ?? ""} onChange={(e) => set("media_asset_id", e.target.value ? Number(e.target.value) : null)}>
          <option value="">{draft.media_name ? `${draft.media_name} (draft)` : t("st.noMedia")}</option>
          {media.map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
        </Select>
        {f.platform === "youtube" && (
          <Select label={t("st.thumbnail")} value={f.thumbnail_asset_id ?? ""} onChange={(e) => set("thumbnail_asset_id", e.target.value ? Number(e.target.value) : null)}>
            <option value="">{t("common.none")}</option>
            {media.filter((m) => m.kind === "image").map((m) => <option key={m.id} value={m.id}>{m.name}</option>)}
          </Select>
        )}
        <Input label={t("st.schedule")} type="datetime-local" value={isoToLocal(f.scheduled_at)} onChange={(e) => set("scheduled_at", localToIso(e.target.value))}
          hint={f.platform === "instagram" ? t("st.localSchedule") : undefined} />
      </fieldset>
      {f.error && <div className="mt-2"><ErrorBox message={f.error} /></div>}
      {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
      {issues && (issues.length ? <ul className="mt-2 list-inside list-disc text-[13px] text-warn">{issues.map((i) => <li key={i}>{i}</li>)}</ul> : <p className="mt-2 text-[13px] text-ok">{t("st.valid")}</p>)}
      {progress != null && act.busy && (
        <div className="mt-2" role="progressbar" aria-valuenow={progress} aria-valuemin={0} aria-valuemax={100}>
          <div className="mb-1 text-xs">{t("st.uploading", { pct: progress })}</div>
          <div className="h-1.5 rounded bg-[var(--panel-2)]"><div className="h-1.5 rounded bg-iris" style={{ width: `${progress}%` }} /></div>
        </div>
      )}
      {!locked && (
        <div className="mt-3 flex flex-wrap gap-2">
          <Button onClick={() => act.run(async () => { await save(); toast(t("common.saved")); onChange(); })}>{t("common.save")}</Button>
          <Button onClick={check} busy={act.busy}>{t("st.validate")}</Button>
          <Button variant="primary" busy={act.busy} onClick={publish}>{f.scheduled_at ? t("st.scheduleIt") : t("st.publishNow")}</Button>
          {f.status === "scheduled" && !f.external_id && <Button variant="ghost" onClick={() => act.run(async () => { await api.unschedule(f.id); onChange(); })}>{t("st.unschedule")}</Button>}
        </div>
      )}
    </div>
  );
}

function DraftEditor({ draft, media, onSaved, onDeleted }: { draft: Draft | null; media: Media[]; onSaved: (id: number) => void; onDeleted: () => void }) {
  const t = useT();
  const { accounts } = useStore();
  const { confirm, toast } = useFeedback();
  const empty = { id: null as number | null, title: "", body: "", category: "", tags: "", hashtags: "", media_asset_id: null as number | null };
  const [f, setF] = useState(empty);
  const [addFor, setAddFor] = useState("");
  const act = useAction();
  const templates = useLoad(() => api.templates(), []);
  const variants = useLoad(() => (draft ? api.variants(draft.id) : Promise.resolve([])), [draft?.id]);
  useEffect(() => setF(draft ? { id: draft.id, title: draft.title, body: draft.body, category: draft.category ?? "", tags: draft.tags, hashtags: draft.hashtags, media_asset_id: draft.media_asset_id } : empty), [draft]); // eslint-disable-line react-hooks/exhaustive-deps

  const save = () => act.run(async () => { const id = await api.saveDraft({ ...f, category: f.category || null }); toast(t("common.saved")); onSaved(id); return id; });
  const targets = accounts.filter((a) => a.status === "connected" && a.platform !== "spotify" && !variants.data?.some((v) => v.account_id === a.id));
  const addVariant = () => addFor && draft && act.run(async () => {
    await api.saveVariant({ draft_id: draft.id, account_id: Number(addFor), title: null, body: "", media_asset_id: null, thumbnail_asset_id: null, link: null, privacy: null, scheduled_at: null });
    setAddFor(""); await variants.reload(); onSaved(draft.id);
  });
  const remove = async () => { if (draft && (await confirm(t("st.deleteDraft"), { danger: true }))) await act.run(async () => { await api.deleteDraft(draft.id); onDeleted(); }); };
  const applyTemplate = (id: string) => { const tp = templates.data?.find((x) => String(x.id) === id); if (tp) setF((x) => ({ ...x, body: x.body ? `${x.body}\n\n${tp.body}` : tp.body, hashtags: [x.hashtags, tp.hashtags].filter(Boolean).join(" ") })); };

  return (
    <div className="grid gap-4">
      <Panel title={draft ? draft.title : t("st.newDraft")} actions={draft && <Button variant="ghost" onClick={remove}>{t("common.delete")}</Button>}>
        <div className="grid gap-3 md:grid-cols-2">
          <div className="md:col-span-2"><Input label={t("st.draftTitle")} value={f.title} onChange={(e) => setF({ ...f, title: e.target.value })} required /></div>
          <div className="md:col-span-2"><Textarea label={t("st.body")} rows={6} value={f.body} onChange={(e) => setF({ ...f, body: e.target.value })} hint={t("st.chars", { n: fmtNumber(f.body.length, t.lang) })} /></div>
          <Input label={t("st.hashtags")} value={f.hashtags} onChange={(e) => setF({ ...f, hashtags: e.target.value })} placeholder="#podcast #newepisode" />
          <Input label={t("st.tags")} value={f.tags} onChange={(e) => setF({ ...f, tags: e.target.value })} />
          <Input label={t("st.category")} value={f.category} onChange={(e) => setF({ ...f, category: e.target.value })} />
          <Select label={t("st.mediaPick")} value={f.media_asset_id ?? ""} onChange={(e) => setF({ ...f, media_asset_id: e.target.value ? Number(e.target.value) : null })}>
            <option value="">{t("st.noMedia")}</option>{media.map((m) => <option key={m.id} value={m.id}>{m.name} ({m.kind})</option>)}
          </Select>
          {!!templates.data?.length && (
            <Select label={t("st.applyTemplate")} value="" onChange={(e) => applyTemplate(e.target.value)}>
              <option value="">—</option>{templates.data.map((tp) => <option key={tp.id} value={tp.id}>{tp.name}</option>)}
            </Select>
          )}
        </div>
        {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
        <div className="mt-3"><Button variant="primary" busy={act.busy} disabled={!f.title.trim()} onClick={save}>{t("common.save")}</Button></div>
      </Panel>
      {draft && (
        <Panel title={t("st.versions")}>
          <div className="mb-3 flex flex-wrap items-end gap-2">
            <div className="w-72"><Select label={t("st.addVersion")} value={addFor} onChange={(e) => setAddFor(e.target.value)}>
              <option value="">—</option>{targets.map((a) => <option key={a.id} value={a.id}>{PLATFORM_META[a.platform].label}: {a.name}</option>)}
            </Select></div>
            <Button onClick={addVariant} disabled={!addFor}><Plus className="size-3.5" />{t("common.add")}</Button>
            <Button variant="ghost" onClick={() => openExternal("https://creators.spotify.com")}><PlatformDot platform="spotify" />{t("sp.publish")}</Button>
          </div>
          {variants.loading && !variants.data ? <Spinner /> : <div className="grid gap-3">{variants.data?.map((v) => <VariantEditor key={v.id} v={v} draft={draft} media={media} onChange={() => { void variants.reload(); onSaved(draft.id); }} />)}</div>}
        </Panel>
      )}
    </div>
  );
}

function MediaLibrary({ media, reload }: { media: Media[]; reload: () => void }) {
  const t = useT();
  const act = useAction();
  const add = () => act.run(async () => {
    const paths = await openDialog({ multiple: true, extensions: ["jpg", "jpeg", "png", "webp", "gif", "mp4", "mov", "m4v", "webm", "mp3", "m4a", "wav"], label: "Media" });
    if (paths.length) { await api.addMedia(paths); reload(); }
  });
  return (
    <Panel title={t("st.media")} actions={<Button variant="primary" busy={act.busy} onClick={add}><Plus className="size-3.5" />{t("st.addMedia")}</Button>}>
      {act.error && <ErrorBox message={act.error} />}
      {!media.length ? <Empty title={t("st.noMedia")} /> : (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
          {media.map((m) => (
            <div key={m.id} className="rounded-xl border border-[var(--line)] p-2">
              <MediaThumb m={m} className="aspect-video w-full" />
              <div className="mt-2 truncate text-[13px] font-medium" title={m.path}>{m.name}</div>
              <div className="text-xs text-[var(--muted)]">{m.kind}, {fmtNumber(m.size_bytes / 1048576, t.lang)} MB {m.missing && <Badge tone="bad">{t("st.missingFile")}</Badge>}</div>
              <div className="mt-2"><Input label={t("st.publicUrl")} defaultValue={m.public_url ?? ""} placeholder="https://" onBlur={(e) => act.run(async () => { await api.setMediaUrl(m.id, e.target.value || null); reload(); })} /></div>
              <Button variant="ghost" className="mt-1" onClick={() => act.run(async () => { await api.removeMedia(m.id); reload(); })}>{t("common.remove")}</Button>
            </div>
          ))}
        </div>
      )}
    </Panel>
  );
}

function Templates() {
  const t = useT();
  const list = useLoad(() => api.templates(), []);
  const [f, setF] = useState({ id: null as number | null, name: "", body: "", hashtags: "" });
  const act = useAction();
  return (
    <div className="grid gap-4 md:grid-cols-2">
      <Panel title={t("st.templates")}>
        <ul className="space-y-1">{list.data?.map((tp) => (
          <li key={tp.id} className="flex items-center gap-2">
            <button type="button" className="flex-1 truncate text-left hover:text-iris-soft cursor-pointer" onClick={() => setF(tp)}>{tp.name}</button>
            <Button variant="ghost" onClick={() => act.run(async () => { await api.deleteTemplate(tp.id); await list.reload(); })}>{t("common.delete")}</Button>
          </li>
        ))}</ul>
        {!list.data?.length && <p className="text-[var(--muted)]">{t("common.noResults")}</p>}
      </Panel>
      <Panel title={f.id ? f.name : t("st.newTemplate")}>
        <div className="grid gap-3">
          <Input label={t("st.templateName")} value={f.name} onChange={(e) => setF({ ...f, name: e.target.value })} />
          <Textarea label={t("st.body")} value={f.body} onChange={(e) => setF({ ...f, body: e.target.value })} />
          <Input label={t("st.hashtags")} value={f.hashtags} onChange={(e) => setF({ ...f, hashtags: e.target.value })} />
          {act.error && <ErrorBox message={act.error} />}
          <div className="flex gap-2">
            <Button variant="primary" disabled={!f.name.trim()} onClick={() => act.run(async () => { await api.saveTemplate(f.id, f.name, f.body, f.hashtags); setF({ id: null, name: "", body: "", hashtags: "" }); await list.reload(); })}>{t("common.save")}</Button>
            {f.id && <Button variant="ghost" onClick={() => setF({ id: null, name: "", body: "", hashtags: "" })}>{t("st.newTemplate")}</Button>}
          </div>
        </div>
      </Panel>
    </div>
  );
}

function History() {
  const t = useT();
  const h = useLoad(() => api.publishHistory(), []);
  return (
    <Panel title={t("st.history")}>
      {h.error && <ErrorBox message={h.error} />}
      <Table head={[t("common.date"), t("common.account"), t("st.draftTitle"), t("common.type"), t("common.status"), ""]} empty={!h.data?.length}>
        {h.data?.map((r) => (
          <tr key={String(r.id)}>
            <td className="whitespace-nowrap">{fmtDate(String(r.created_at), t.lang, true)}</td><td>{String(r.account_name ?? "—")}</td><td>{String(r.draft_title ?? "—")}</td>
            <td>{String(r.action)}</td><td><Badge tone={r.status === "ok" ? "ok" : "bad"}>{String(r.status)}</Badge></td><td className="max-w-sm truncate text-[var(--muted)]" title={String(r.message ?? "")}>{String(r.message ?? "")}</td>
          </tr>
        ))}
      </Table>
    </Panel>
  );
}

export default function Studio() {
  const t = useT();
  const [params, setParams] = useSearchParams();
  const tab = (params.get("tab") ?? "drafts") as "drafts" | "media" | "templates" | "history";
  const drafts = useLoad(() => api.drafts(), []);
  const media = useLoad(() => api.media(), []);
  const draftId = params.get("draft");
  const current = drafts.data?.find((d) => String(d.id) === draftId) ?? null;
  const select = (id: number | null) => setParams(id ? { draft: String(id) } : { new: "1" });

  return (
    <>
      <PageHeader title={t("st.title")} subtitle={t("st.subtitle")} actions={<Button variant="primary" onClick={() => { setParams({ new: "1" }); }}><Plus className="size-3.5" />{t("st.newDraft")}</Button>} />
      <Tabs value={tab} onChange={(v) => setParams({ tab: v })} items={[{ id: "drafts", label: t("st.drafts") }, { id: "media", label: t("st.media") }, { id: "templates", label: t("st.templates") }, { id: "history", label: t("st.history") }]} />
      {drafts.error && <ErrorBox message={drafts.error} onRetry={drafts.reload} />}
      {tab === "drafts" && (
        <div className="grid gap-4 lg:grid-cols-[280px_1fr]">
          <Panel title={t("st.drafts")} pad={false}>
            <ul className="max-h-[70vh] overflow-y-auto p-2">
              {drafts.data?.map((d) => {
                const vs = parseJson<{ platform: keyof typeof PLATFORM_META; status: string }[]>(d.variants_json, []);
                return (
                  <li key={d.id}>
                    <button type="button" onClick={() => select(d.id)} className={cx("w-full rounded-lg p-2 text-left cursor-pointer", String(d.id) === draftId ? "bg-[var(--panel-2)]" : "hover:bg-[var(--panel-2)]/60")}>
                      <div className="truncate font-medium">{d.title}</div>
                      <div className="mt-1 flex flex-wrap items-center gap-1.5 text-xs text-[var(--muted)]">{vs.map((v, i) => <PlatformDot key={i} platform={v.platform} />)}<span>{fmtDate(d.updated_at, t.lang)}</span></div>
                    </button>
                  </li>
                );
              })}
            </ul>
            {!drafts.data?.length && <p className="p-4 text-[var(--muted)]">{t("st.empty")}</p>}
          </Panel>
          {draftId && !current && drafts.data ? <Empty title={t("st.selectDraft")} /> : (
            <DraftEditor draft={current} media={media.data ?? []} onSaved={(id) => { void drafts.reload(); if (String(id) !== draftId) select(id); }} onDeleted={() => { setParams({}); void drafts.reload(); }} />
          )}
        </div>
      )}
      {tab === "media" && <MediaLibrary media={media.data ?? []} reload={media.reload} />}
      {tab === "templates" && <Templates />}
      {tab === "history" && <History />}
    </>
  );
}
