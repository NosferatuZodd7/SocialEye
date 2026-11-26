import { useState } from "react";
import { Link } from "react-router-dom";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { api } from "../lib/api";
import { useLoad } from "../lib/hooks";
import { useT } from "../lib/i18n";
import { isoDay } from "../lib/format";
import { openExternal } from "../lib/files";
import { Button, ErrorBox, PageHeader, PlatformDot, cx } from "../components/ui";

/** Monday-first grid of dates covering the whole month. */
export function monthGrid(year: number, month: number): Date[] {
  const first = new Date(Date.UTC(year, month, 1));
  const start = new Date(first);
  start.setUTCDate(1 - ((first.getUTCDay() + 6) % 7));
  return Array.from({ length: 42 }, (_, i) => new Date(start.getTime() + i * 86400000));
}

export default function CalendarPage() {
  const t = useT();
  const now = new Date();
  const [ym, setYm] = useState({ y: now.getUTCFullYear(), m: now.getUTCMonth() });
  const days = monthGrid(ym.y, ym.m);
  const items = useLoad(() => api.calendar(isoDay(days[0]), isoDay(days[41])), [ym.y, ym.m]);
  const shift = (d: number) => setYm(({ y, m }) => ({ y: m + d < 0 ? y - 1 : m + d > 11 ? y + 1 : y, m: (m + d + 12) % 12 }));
  const locale = t.lang === "pt" ? "pt-PT" : "en-GB";
  const weekdays = days.slice(0, 7).map((d) => new Intl.DateTimeFormat(locale, { weekday: "short", timeZone: "UTC" }).format(d));
  const today = isoDay(now);
  // Items are bucketed by local date so a post scheduled for 00:30 local shows on that day.
  const local = (iso: string) => { const d = new Date(iso); return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`; };

  return (
    <>
      <PageHeader title={t("cal.title")} subtitle={t("cal.subtitle")} actions={<>
        <Button variant="ghost" aria-label={t("cal.prev")} onClick={() => shift(-1)}><ChevronLeft className="size-4" /></Button>
        <span className="num min-w-40 text-center text-[15px] font-semibold">{new Intl.DateTimeFormat(locale, { month: "long", year: "numeric", timeZone: "UTC" }).format(new Date(Date.UTC(ym.y, ym.m, 1)))}</span>
        <Button variant="ghost" aria-label={t("cal.next")} onClick={() => shift(1)}><ChevronRight className="size-4" /></Button>
        <Button onClick={() => setYm({ y: now.getUTCFullYear(), m: now.getUTCMonth() })}>{t("cal.today")}</Button>
      </>} />
      {items.error && <ErrorBox message={items.error} onRetry={items.reload} />}
      <div className="grid grid-cols-7 overflow-hidden rounded-[14px] border border-[var(--line)] bg-[var(--line)] gap-px">
        {weekdays.map((w) => <div key={w} className="bg-[var(--panel)] px-2 py-1.5 text-[12px] text-[var(--muted)]">{w}</div>)}
        {days.map((d) => {
          const key = isoDay(d);
          const its = (items.data ?? []).filter((i) => i.at && local(i.at) === key);
          return (
            <div key={key} className={cx("min-h-28 bg-[var(--panel)] p-1.5", d.getUTCMonth() !== ym.m && "opacity-45")}>
              <div className={cx("num mb-1 inline-flex size-6 items-center justify-center rounded-full text-[12px]", key === today && "bg-iris text-white")}>{d.getUTCDate()}</div>
              <ul className="space-y-0.5">
                {its.slice(0, 4).map((i) => (
                  <li key={`${i.source}${i.id}`}>
                    {i.source === "variant"
                      ? <Link to={`/studio?draft=${i.draft_id}`} className={cx("flex items-center gap-1 truncate rounded px-1 text-[11.5px] hover:bg-[var(--panel-2)]", i.status === "scheduled" && "border border-dashed border-iris/60")}><PlatformDot platform={i.platform} /><span className="truncate">{i.title}</span></Link>
                      : <button type="button" onClick={() => i.url && openExternal(i.url)} className="flex w-full items-center gap-1 truncate rounded px-1 text-left text-[11.5px] text-[var(--muted)] hover:bg-[var(--panel-2)] cursor-pointer"><PlatformDot platform={i.platform} /><span className="truncate">{i.title}</span></button>}
                  </li>
                ))}
                {its.length > 4 && <li className="px-1 text-[11px] text-[var(--muted)]">+{its.length - 4}</li>}
              </ul>
            </div>
          );
        })}
      </div>
    </>
  );
}
