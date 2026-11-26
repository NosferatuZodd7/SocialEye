import { useEffect, useId, useRef, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes, type TextareaHTMLAttributes } from "react";
import { AlertTriangle, Loader2, X } from "lucide-react";
import type { Platform } from "../lib/api";
import { PLATFORM_META } from "../lib/platforms";
import { useT } from "../lib/i18n";
import { fmtNumber, fmtRelative } from "../lib/format";

const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

type Variant = "primary" | "ghost" | "danger" | "subtle";
export function Button({ variant = "subtle", busy, className, children, ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; busy?: boolean }) {
  const styles: Record<Variant, string> = {
    primary: "bg-iris text-white hover:bg-[#6a48ff] disabled:opacity-50",
    subtle: "bg-[var(--panel-2)] text-[var(--ink)] hover:brightness-110 border border-[var(--line)] disabled:opacity-50",
    ghost: "text-[var(--muted)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] disabled:opacity-40",
    danger: "bg-bad/10 text-bad border border-bad/30 hover:bg-bad/20 disabled:opacity-50",
  };
  return (
    <button type="button" {...p} disabled={p.disabled || busy} className={cx("inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-[13px] font-medium transition-colors cursor-pointer disabled:cursor-not-allowed", styles[variant], className)}>
      {busy && <Loader2 className="size-3.5 animate-spin" aria-hidden />}
      {children}
    </button>
  );
}

export function Panel({ title, actions, children, className, pad = true }: { title?: ReactNode; actions?: ReactNode; children: ReactNode; className?: string; pad?: boolean }) {
  return (
    <section className={cx("rounded-[14px] border border-[var(--line)] bg-[var(--panel)]", className)}>
      {(title || actions) && (
        <header className="flex items-center justify-between gap-3 px-4 pt-3.5 pb-1">
          {title && <h2 className="text-[15px] font-semibold">{title}</h2>}
          {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
        </header>
      )}
      <div className={pad ? "p-4 pt-2" : ""}>{children}</div>
    </section>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-[26px] font-semibold">{title}</h1>
        {subtitle && <p className="mt-1 max-w-[70ch] text-[var(--muted)]">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap items-center gap-2">{actions}</div>}
    </div>
  );
}

export function PlatformDot({ platform, className }: { platform: Platform; className?: string }) {
  return <span aria-hidden className={cx("inline-block size-2.5 shrink-0 rounded-full", className)} style={{ background: PLATFORM_META[platform].color }} />;
}

export function PlatformBadge({ platform }: { platform: Platform }) {
  return (
    <span className="inline-flex items-center gap-1.5 rounded-full border border-[var(--line)] px-2 py-0.5 text-xs">
      <PlatformDot platform={platform} />{PLATFORM_META[platform].label}
    </span>
  );
}

export function Badge({ tone = "neutral", children }: { tone?: "neutral" | "ok" | "warn" | "bad" | "iris"; children: ReactNode }) {
  const t = { neutral: "bg-[var(--panel-2)] text-[var(--muted)]", ok: "bg-ok/15 text-ok", warn: "bg-warn/15 text-warn", bad: "bg-bad/15 text-bad", iris: "bg-iris/15 text-iris-soft" }[tone];
  return <span className={cx("inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium whitespace-nowrap", t)}>{children}</span>;
}

export function Spinner({ label }: { label?: string }) {
  const t = useT();
  return (
    <div role="status" className="flex items-center gap-2 py-6 text-[var(--muted)]">
      <Loader2 className="size-4 animate-spin" aria-hidden />{label ?? t("common.loading")}
    </div>
  );
}

export function ErrorBox({ message, onRetry }: { message: string; onRetry?: () => void }) {
  const t = useT();
  return (
    <div role="alert" className="flex items-start gap-3 rounded-lg border border-bad/30 bg-bad/10 p-3 text-[13px]">
      <AlertTriangle className="mt-0.5 size-4 shrink-0 text-bad" aria-hidden />
      <div className="min-w-0 flex-1 break-words">{message}</div>
      {onRetry && <Button variant="ghost" onClick={onRetry}>{t("common.retry")}</Button>}
    </div>
  );
}

export function Empty({ title, children, action }: { title: string; children?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-start gap-2 rounded-lg border border-dashed border-[var(--line)] p-6">
      <p className="font-medium">{title}</p>
      {children && <div className="max-w-[65ch] text-[var(--muted)]">{children}</div>}
      {action}
    </div>
  );
}

/** A figure with its provenance. `value === null` renders the hatched "not available" state. */
export function Stat({ label, value, source, day, reason, delta }: { label: string; value: number | null | undefined; source?: string; day?: string; reason?: string; delta?: number | null }) {
  const t = useT();
  const missing = value == null;
  return (
    <div className={cx("rounded-xl border border-[var(--line)] p-3.5", missing ? "hatched" : "bg-[var(--panel)]")} title={reason}>
      <div className="text-[12.5px] text-[var(--muted)]">{label}</div>
      {missing ? (
        <div className="mt-1 text-[13px] font-medium text-[var(--muted)]">{t("common.unavailable")}</div>
      ) : (
        <div className="mt-0.5 flex items-baseline gap-2">
          <span className="num text-[24px] font-semibold leading-tight">{fmtNumber(value, t.lang)}</span>
          {delta != null && Number.isFinite(delta) && (
            <span className={cx("text-xs font-medium", delta >= 0 ? "text-ok" : "text-bad")}>{delta >= 0 ? "+" : ""}{delta.toFixed(1)}%</span>
          )}
        </div>
      )}
      {(source || reason) && (
        <div className="mt-1 truncate text-[11.5px] text-[var(--muted)]" title={source ?? reason}>
          {missing ? reason : `${source}${day ? ` · ${fmtRelative(day + "T12:00:00Z", t.lang)}` : ""}`}
        </div>
      )}
    </div>
  );
}

function FieldWrap({ label, hint, children, id }: { label: string; hint?: ReactNode; children: ReactNode; id: string }) {
  return (
    <div className="flex flex-col gap-1">
      <label htmlFor={id} className="text-[12.5px] font-medium text-[var(--muted)]">{label}</label>
      {children}
      {hint && <div className="text-[11.5px] text-[var(--muted)]">{hint}</div>}
    </div>
  );
}

const inputCls = "w-full rounded-lg border border-[var(--line)] bg-[var(--bg)] px-2.5 py-1.5 text-[13px] text-[var(--ink)] placeholder:text-[var(--muted)]/70";

export function Input({ label, hint, ...p }: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode }) {
  const id = useId();
  return <FieldWrap label={label} hint={hint} id={id}><input id={id} {...p} className={cx(inputCls, p.className)} /></FieldWrap>;
}

export function Textarea({ label, hint, ...p }: TextareaHTMLAttributes<HTMLTextAreaElement> & { label: string; hint?: ReactNode }) {
  const id = useId();
  return <FieldWrap label={label} hint={hint} id={id}><textarea id={id} rows={4} {...p} className={cx(inputCls, "resize-y", p.className)} /></FieldWrap>;
}

export function Select({ label, hint, children, ...p }: SelectHTMLAttributes<HTMLSelectElement> & { label: string; hint?: ReactNode }) {
  const id = useId();
  return <FieldWrap label={label} hint={hint} id={id}><select id={id} {...p} className={cx(inputCls, p.className)}>{children}</select></FieldWrap>;
}

export function Toggle({ checked, onChange, label, hideLabel }: { checked: boolean; onChange: (v: boolean) => void; label: string; hideLabel?: boolean }) {
  return (
    <label className="inline-flex cursor-pointer items-center gap-2 text-[13px]">
      <button type="button" role="switch" aria-checked={checked} aria-label={label} onClick={() => onChange(!checked)}
        className={cx("relative h-5 w-9 rounded-full transition-colors", checked ? "bg-iris" : "bg-[var(--line)]")}>
        <span className={cx("absolute top-0.5 size-4 rounded-full bg-white transition-all", checked ? "left-[18px]" : "left-0.5")} />
      </button>
      {!hideLabel && <span>{label}</span>}
    </label>
  );
}

export function Tabs<T extends string>({ value, onChange, items }: { value: T; onChange: (v: T) => void; items: { id: T; label: string }[] }) {
  return (
    <div role="tablist" className="mb-4 flex flex-wrap gap-1 border-b border-[var(--line)]">
      {items.map((i) => (
        <button key={i.id} role="tab" type="button" aria-selected={value === i.id} onClick={() => onChange(i.id)}
          className={cx("-mb-px border-b-2 px-3 py-2 text-[13px] font-medium cursor-pointer", value === i.id ? "border-iris text-[var(--ink)]" : "border-transparent text-[var(--muted)] hover:text-[var(--ink)]")}>
          {i.label}
        </button>
      ))}
    </div>
  );
}

export function Modal({ open, onClose, title, children, wide }: { open: boolean; onClose: () => void; title: string; children: ReactNode; wide?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);
  const t = useT();
  useEffect(() => {
    if (!open) return;
    const prev = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("input,textarea,select,button:not([data-close])")?.focus();
    const key = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", key);
    return () => { window.removeEventListener("keydown", key); prev?.focus(); };
  }, [open, onClose]);
  if (!open) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center overflow-y-auto bg-black/50 p-6 pt-[8vh]" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={ref} role="dialog" aria-modal="true" aria-label={title} className={cx("w-full rounded-[14px] border border-[var(--line)] bg-[var(--panel)] shadow-2xl", wide ? "max-w-4xl" : "max-w-lg")}>
        <div className="flex items-center justify-between border-b border-[var(--line)] px-4 py-3">
          <h2 className="text-[16px] font-semibold">{title}</h2>
          <button type="button" data-close onClick={onClose} aria-label={t("common.close")} className="rounded p-1 text-[var(--muted)] hover:text-[var(--ink)] cursor-pointer"><X className="size-4" /></button>
        </div>
        <div className="p-4">{children}</div>
      </div>
    </div>
  );
}

export function Table({ head, children, empty }: { head: ReactNode[]; children: ReactNode; empty?: boolean }) {
  const t = useT();
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-left text-[13px]">
        <thead className="text-[12px] text-[var(--muted)]">
          <tr>{head.map((h, i) => <th key={i} className="border-b border-[var(--line)] px-2 py-2 font-medium">{h}</th>)}</tr>
        </thead>
        <tbody className="[&_td]:border-b [&_td]:border-[var(--line)]/60 [&_td]:px-2 [&_td]:py-2">{children}</tbody>
      </table>
      {empty && <p className="py-4 text-center text-[var(--muted)]">{t("common.noResults")}</p>}
    </div>
  );
}

export { cx };
