import { useRef, useState } from "react";
import { api } from "../lib/api";
import { useAction, useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { useStore } from "../lib/store";
import { fmtDate, fmtNumber } from "../lib/format";
import { openExternal, readFiles } from "../lib/files";
import { useFeedback } from "../components/feedback";
import { Badge, Button, Empty, ErrorBox, Input, Panel, Select, Tabs } from "../components/ui";

type ListId = "not_following_back" | "fans" | "mutual" | "new_followers" | "lost_followers" | "new_following";

export function InstagramFollowers() {
  const t = useT();
  const { backend } = useStore();
  const { toast } = useFeedback();
  const fileRef = useRef<HTMLInputElement>(null);
  const [label, setLabel] = useState("");
  const [result, setResult] = useState<{ warnings: string[]; used_files: string[] } | null>(null);
  const imports = useLoad(() => (backend ? api.igImports() : Promise.resolve([])), [backend]);
  const [sel, setSel] = useState<number | null>(null);
  const [prev, setPrev] = useState<number | null>(null);
  const current = sel ?? imports.data?.[0]?.id ?? null;
  const previous = prev ?? imports.data?.find((i) => i.id !== current && i.id < (current ?? 0))?.id ?? null;
  const cmp = useLoad(() => (current ? api.igCompare(current, previous) : Promise.resolve(null)), [current, previous]);
  const [list, setList] = useState<ListId>("not_following_back");
  const [filter, setFilter] = useState("");
  const act = useAction();

  const onFiles = (files: FileList | null) => files?.length && act.run(async () => {
    const r = await api.igImport(label, await readFiles(files));
    setResult(r); setSel(r.id); setPrev(null); setLabel("");
    toast(t("ig.imported", { followers: fmtNumber(r.followers, t.lang), following: fmtNumber(r.following, t.lang) }));
    await imports.reload();
    if (fileRef.current) fileRef.current.value = "";
  });

  const lists: { id: ListId; label: string }[] = [
    { id: "not_following_back", label: t("ig.notFollowingBack") }, { id: "fans", label: t("ig.fans") }, { id: "mutual", label: t("ig.mutual") },
    ...(cmp.data?.new_followers ? [{ id: "new_followers" as const, label: t("ig.newFollowers") }, { id: "lost_followers" as const, label: t("ig.lostFollowers") }, { id: "new_following" as const, label: t("ig.newFollowing") }] : []),
  ];
  const names = (cmp.data?.[list] ?? []).filter((u) => u.includes(filter.toLowerCase()));

  return (
    <div className="grid gap-4">
      <Panel title={t("ig.importTitle")}>
        <p className="max-w-[75ch] text-[var(--muted)]">{t("ig.importBody")}</p>
        <p className="mt-2 max-w-[75ch] text-[13px]">{t("ig.steps")}</p>
        <div className="mt-3 flex flex-wrap items-end gap-2">
          <div className="w-64"><Input label={t("ig.label")} value={label} onChange={(e) => setLabel(e.target.value)} /></div>
          <input ref={fileRef} type="file" multiple accept=".json,.html" className="sr-only" id="ig-files" onChange={(e) => onFiles(e.target.files)} />
          <Button variant="primary" busy={act.busy} onClick={() => fileRef.current?.click()}>{t("ig.import")}</Button>
        </div>
        {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
        {result && result.warnings.length > 0 && <ul className="mt-2 list-inside list-disc text-[12.5px] text-warn">{result.warnings.map((w) => <li key={w}>{w}</li>)}</ul>}
      </Panel>
      {imports.data?.length ? (
        <Panel title={t("ig.followersTab")}>
          <div className="mb-3 grid gap-2 md:grid-cols-3">
            <Select label={t("ig.imports")} value={current ?? ""} onChange={(e) => { setSel(Number(e.target.value)); setPrev(null); }}>
              {imports.data.map((i) => <option key={i.id} value={i.id}>{i.label} — {fmtDate(i.imported_at, t.lang)} ({fmtNumber(i.followers_count, t.lang)} / {fmtNumber(i.following_count, t.lang)})</option>)}
            </Select>
            <Select label={t("ig.compareWith")} value={previous ?? ""} onChange={(e) => setPrev(e.target.value ? Number(e.target.value) : null)}>
              <option value="">{t("common.none")}</option>
              {imports.data.filter((i) => i.id !== current).map((i) => <option key={i.id} value={i.id}>{i.label} — {fmtDate(i.imported_at, t.lang)}</option>)}
            </Select>
            <Input label={t("common.search")} value={filter} onChange={(e) => setFilter(e.target.value)} />
          </div>
          <Tabs value={list} onChange={setList} items={lists.map((l) => ({ ...l, label: `${l.label} (${fmtNumber(cmp.data?.[l.id]?.length ?? 0, t.lang)})` }))} />
          {cmp.error && <ErrorBox message={cmp.error} />}
          <p className="mb-2 text-[12.5px] text-[var(--muted)]">{t("ig.manual")}</p>
          {names.length ? (
            <ul className="grid max-h-[480px] grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-1 overflow-y-auto">
              {names.map((u) => (
                <li key={u} className="flex items-center justify-between gap-2 rounded-lg px-2 py-1 hover:bg-[var(--panel-2)]">
                  <span className="truncate">@{u}</span>
                  <button type="button" className="text-xs text-iris-soft cursor-pointer" onClick={() => openExternal(`https://www.instagram.com/${encodeURIComponent(u)}/`)}>{t("ig.openProfile")}</button>
                </li>
              ))}
            </ul>
          ) : <p className="text-[var(--muted)]">{t("common.noResults")}</p>}
          <div className="mt-3 flex gap-2">
            <Badge>{imports.data.length} {t("ig.imports").toLowerCase()}</Badge>
            {current && <Button variant="ghost" onClick={() => act.run(async () => { await api.igDeleteImport(current); setSel(null); await imports.reload(); })}>{t("common.delete")}</Button>}
          </div>
        </Panel>
      ) : backend && <Empty title={t("ig.importTitle")} />}
    </div>
  );
}
