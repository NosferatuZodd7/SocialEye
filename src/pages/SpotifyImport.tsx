import { useRef, useState } from "react";
import { ExternalLink } from "lucide-react";
import { api, type Account } from "../lib/api";
import { useAction } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { openExternal } from "../lib/files";
import { metricLabel } from "../lib/platforms";
import { Button, ErrorBox, Panel } from "../components/ui";

export function SpotifyImport({ account }: { account: Account }) {
  const t = useT();
  const ref = useRef<HTMLInputElement>(null);
  const act = useAction();
  const [res, setRes] = useState<Awaited<ReturnType<typeof api.csvImport>> | null>(null);
  const onFile = (f: File | undefined) => f && act.run(async () => {
    setRes(await api.csvImport(account.id, f.name, await f.text()));
    if (ref.current) ref.current.value = "";
  });
  return (
    <Panel title={t("sp.csvTitle")}>
      <p className="max-w-[75ch] text-[var(--muted)]">{t("sp.csvBody")}</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <input ref={ref} type="file" accept=".csv,text/csv" className="sr-only" onChange={(e) => onFile(e.target.files?.[0])} />
        <Button variant="primary" busy={act.busy} onClick={() => ref.current?.click()}>{t("sp.csvImport")}</Button>
        <Button onClick={() => openExternal("https://creators.spotify.com")}><ExternalLink className="size-3.5" />Spotify for Creators</Button>
      </div>
      {act.error && <div className="mt-2"><ErrorBox message={act.error} /></div>}
      {res && (
        <div className="mt-3 text-[13px]">
          <p className="text-ok">{t("sp.csvResult", { n: res.stored, metrics: res.metrics.map((m) => metricLabel(m, t.lang)).join(", ") })}</p>
          {res.skipped_rows > 0 && <p className="text-warn">{res.skipped_rows} rows skipped (unreadable date)</p>}
          {res.ignored_columns.length > 0 && <p className="text-[var(--muted)]">Ignored columns: {res.ignored_columns.join(", ")}</p>}
          {res.unmatched_episodes.length > 0 && <p className="text-warn">Episodes not found in the synced catalogue: {res.unmatched_episodes.slice(0, 10).join(", ")}</p>}
        </div>
      )}
    </Panel>
  );
}
