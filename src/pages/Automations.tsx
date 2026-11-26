import { useState } from "react";
import { Plus } from "lucide-react";
import { api, parseJson, type Automation, type Platform } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT, type Key } from "../lib/i18n";
import { fmtDate, fmtNumber } from "../lib/format";
import { METRIC_LABELS, PLATFORMS, PLATFORM_META, metricLabel } from "../lib/platforms";
import { Badge, Button, ErrorBox, Input, Modal, PageHeader, Panel, Select, Table, Toggle } from "../components/ui";

export const TRIGGERS = ["metric_threshold", "content_threshold", "metric_gain", "new_content", "new_comments", "sync_failed", "reconnect_required", "token_expiring", "publish_failed"] as const;
export type TriggerType = (typeof TRIGGERS)[number];

export interface Rule {
  trigger: { type: TriggerType; metric?: string; value?: number; days?: number };
  platforms: Platform[];
  account_ids: number[];
  notify_in_app: boolean;
  notify_desktop: boolean;
}

/** Build the rule payload the backend expects (only fields relevant to the trigger). */
export function toRule(f: { type: TriggerType; metric: string; value: number; days: number; platforms: Platform[]; inApp: boolean; desktop: boolean }): Rule {
  const trigger: Rule["trigger"] = { type: f.type };
  if (f.type === "metric_threshold" || f.type === "content_threshold") Object.assign(trigger, { metric: f.metric, value: f.value });
  if (f.type === "metric_gain") Object.assign(trigger, { metric: f.metric, value: f.value, days: f.days });
  if (f.type === "token_expiring") trigger.days = f.days;
  return { trigger, platforms: f.platforms, account_ids: [], notify_in_app: f.inApp, notify_desktop: f.desktop };
}

export function describe(rule: Rule, t: ReturnType<typeof useT>): string {
  const tr = rule.trigger;
  let s = t(`au.t.${tr.type}` as Key);
  if (tr.metric) s += `: ${metricLabel(tr.metric, t.lang)}`;
  if (tr.value != null) s += ` ${tr.type === "metric_gain" ? "+" : "≥ "}${fmtNumber(tr.value, t.lang)}`;
  if (tr.days != null) s += ` (${tr.days} ${t("common.days").toLowerCase()})`;
  return s;
}

function Editor({ initial, onClose, onSaved }: { initial: Automation | null; onClose: () => void; onSaved: () => void }) {
  const t = useT();
  const r = initial ? parseJson<Rule | null>(initial.rule_json, null) : null;
  const [name, setName] = useState(initial?.name ?? "");
  const [f, setF] = useState({ type: (r?.trigger.type ?? "metric_threshold") as TriggerType, metric: r?.trigger.metric ?? "views", value: r?.trigger.value ?? 10000, days: r?.trigger.days ?? 7, platforms: r?.platforms ?? [], inApp: r?.notify_in_app ?? true, desktop: r?.notify_desktop ?? false });
  const act = useAction();
  const needsMetric = ["metric_threshold", "content_threshold", "metric_gain"].includes(f.type);
  const needsDays = f.type === "metric_gain" || f.type === "token_expiring";
  const save = () => act.run(async () => { await api.saveAutomation(initial?.id ?? null, name, initial ? !!initial.enabled : true, toRule(f)); onSaved(); onClose(); });
  return (
    <Modal open onClose={onClose} title={initial ? initial.name : t("au.new")}>
      <div className="grid gap-3">
        <Input label={t("common.name")} value={name} onChange={(e) => setName(e.target.value)} />
        <Select label={t("au.trigger")} value={f.type} onChange={(e) => setF({ ...f, type: e.target.value as TriggerType })}>
          {TRIGGERS.map((x) => <option key={x} value={x}>{t(`au.t.${x}` as Key)}</option>)}
        </Select>
        {needsMetric && (
          <div className="grid grid-cols-2 gap-3">
            <Select label={t("common.metric")} value={f.metric} onChange={(e) => setF({ ...f, metric: e.target.value })}>
              {Object.keys(METRIC_LABELS).map((m) => <option key={m} value={m}>{metricLabel(m, t.lang)}</option>)}
            </Select>
            <Input label={t("au.threshold")} type="number" min={0} value={f.value} onChange={(e) => setF({ ...f, value: Number(e.target.value) })} />
          </div>
        )}
        {needsDays && <Input label={t("common.days")} type="number" min={1} max={f.type === "token_expiring" ? 60 : 365} value={f.days} onChange={(e) => setF({ ...f, days: Number(e.target.value) })} />}
        <fieldset>
          <legend className="mb-1 text-[12.5px] font-medium text-[var(--muted)]">{t("au.conditions")} ({f.platforms.length ? "" : t("au.allPlatforms")})</legend>
          <div className="flex flex-wrap gap-3">{PLATFORMS.map((p) => <Toggle key={p} label={PLATFORM_META[p].label} checked={f.platforms.includes(p)} onChange={(v) => setF({ ...f, platforms: v ? [...f.platforms, p] : f.platforms.filter((x) => x !== p) })} />)}</div>
        </fieldset>
        <fieldset>
          <legend className="mb-1 text-[12.5px] font-medium text-[var(--muted)]">{t("au.actions")}</legend>
          <div className="flex flex-col gap-2"><Toggle label={t("au.inApp")} checked={f.inApp} onChange={(v) => setF({ ...f, inApp: v })} /><Toggle label={t("au.desktop")} checked={f.desktop} onChange={(v) => setF({ ...f, desktop: v })} /></div>
        </fieldset>
        {act.error && <ErrorBox message={act.error} />}
        <div className="flex justify-end gap-2"><Button onClick={onClose}>{t("common.cancel")}</Button><Button variant="primary" busy={act.busy} disabled={!name.trim()} onClick={save}>{t("common.save")}</Button></div>
      </div>
    </Modal>
  );
}

export default function Automations() {
  const t = useT();
  const list = useLoad(() => api.automations(), []);
  const runs = useLoad(() => api.automationRuns(), [list.data]);
  const [edit, setEdit] = useState<Automation | null | "new">(null);
  const act = useAction();
  return (
    <>
      <PageHeader title={t("au.title")} subtitle={t("au.subtitle")} actions={<Button variant="primary" onClick={() => setEdit("new")}><Plus className="size-3.5" />{t("au.new")}</Button>} />
      {(list.error || act.error) && <ErrorBox message={(list.error || act.error)!} onRetry={list.reload} />}
      <Panel>
        <Table head={[t("common.enabled"), t("common.name"), t("au.trigger"), t("au.conditions"), t("au.actions"), t("au.lastFired"), ""]} empty={!list.data?.length}>
          {list.data?.map((a) => {
            const r = parseJson<Rule | null>(a.rule_json, null);
            return (
              <tr key={a.id}>
                <td><Toggle label={a.name} hideLabel checked={!!a.enabled} onChange={(v) => act.run(async () => { await api.setAutomationEnabled(a.id, v); await list.reload(); })} /></td>
                <td className="font-medium">{a.name}</td>
                <td>{r ? describe(r, t) : <Badge tone="bad">invalid</Badge>}</td>
                <td>{r?.platforms.length ? r.platforms.map((p) => PLATFORM_META[p].label).join(", ") : t("au.allPlatforms")}</td>
                <td className="text-[12.5px]">{[r?.notify_in_app && t("nav.notifications"), r?.notify_desktop && "Desktop"].filter(Boolean).join(", ")}</td>
                <td className="text-[12.5px] text-[var(--muted)]">{a.last_fired_at ? fmtDate(a.last_fired_at, t.lang, true) : t("common.never")} ({a.run_count})</td>
                <td className="whitespace-nowrap"><Button variant="ghost" onClick={() => setEdit(a)}>{t("common.edit")}</Button><Button variant="ghost" onClick={() => act.run(async () => { await api.deleteAutomation(a.id); await list.reload(); })}>{t("common.delete")}</Button></td>
              </tr>
            );
          })}
        </Table>
      </Panel>
      <Panel title={t("au.runs")} className="mt-4">
        <Table head={[t("common.date"), t("common.name"), t("common.status"), ""]} empty={!runs.data?.length}>
          {runs.data?.slice(0, 100).map((r) => <tr key={r.id}><td className="whitespace-nowrap">{fmtDate(r.ran_at, t.lang, true)}</td><td>{r.name}</td><td><Badge tone={r.status === "fired" ? "ok" : "bad"}>{r.status}</Badge></td><td className="text-[var(--muted)]">{r.message}</td></tr>)}
        </Table>
      </Panel>
      {edit && <Editor initial={edit === "new" ? null : edit} onClose={() => setEdit(null)} onSaved={list.reload} />}
    </>
  );
}
