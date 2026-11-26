export type Lang = "en" | "pt";

const locale = (l: Lang) => (l === "pt" ? "pt-PT" : "en-GB");

export function fmtNumber(n: number | null | undefined, lang: Lang = "en"): string {
  if (n == null || Number.isNaN(n)) return "—";
  return new Intl.NumberFormat(locale(lang), { maximumFractionDigits: Math.abs(n) < 10 && n % 1 ? 1 : 0 }).format(n);
}

export function fmtCompact(n: number | null | undefined, lang: Lang = "en"): string {
  if (n == null || Number.isNaN(n)) return "—";
  return new Intl.NumberFormat(locale(lang), { notation: "compact", maximumFractionDigits: 1 }).format(n);
}

/** SQLite `datetime('now')` values are UTC without a zone marker. */
export function parseDate(s: string | null | undefined): Date | null {
  if (!s) return null;
  const iso = /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/.test(s) ? s.replace(" ", "T") + "Z" : s;
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? null : d;
}

export function fmtDate(s: string | null | undefined, lang: Lang = "en", withTime = false): string {
  const d = parseDate(s);
  if (!d) return "—";
  return new Intl.DateTimeFormat(locale(lang), withTime ? { dateStyle: "medium", timeStyle: "short" } : { dateStyle: "medium" }).format(d);
}

export function fmtRelative(s: string | null | undefined, lang: Lang = "en", now = Date.now()): string {
  const d = parseDate(s);
  if (!d) return "—";
  const secs = Math.round((d.getTime() - now) / 1000);
  const rtf = new Intl.RelativeTimeFormat(locale(lang), { numeric: "auto" });
  const units: [Intl.RelativeTimeFormatUnit, number][] = [["day", 86400], ["hour", 3600], ["minute", 60]];
  for (const [u, n] of units) if (Math.abs(secs) >= n) return rtf.format(Math.round(secs / n), u);
  return rtf.format(secs, "second");
}

export function fmtDuration(secs: number | null | undefined): string {
  if (secs == null) return "—";
  const h = Math.floor(secs / 3600), m = Math.floor((secs % 3600) / 60), s = Math.floor(secs % 60);
  return h ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}` : `${m}:${String(s).padStart(2, "0")}`;
}

export const isoDay = (d: Date) => d.toISOString().slice(0, 10);
export const daysAgo = (n: number, from = new Date()) => isoDay(new Date(from.getTime() - n * 86400000));

/** Previous period of equal length, for period-over-period comparison. */
export function previousRange(from: string, to: string): { from: string; to: string } {
  const f = new Date(from + "T00:00:00Z").getTime(), t = new Date(to + "T00:00:00Z").getTime();
  const len = t - f + 86400000;
  return { from: isoDay(new Date(f - len)), to: isoDay(new Date(f - 86400000)) };
}

export function pctChange(now: number, before: number): number | null {
  if (!before) return null;
  return ((now - before) / Math.abs(before)) * 100;
}

/** <input type="datetime-local"> value (local time) → RFC 3339 UTC. */
export const localToIso = (v: string) => (v ? new Date(v).toISOString().replace(/\.\d{3}Z$/, "Z") : null);
export const isoToLocal = (v: string | null) => {
  if (!v) return "";
  const d = new Date(v);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
};
