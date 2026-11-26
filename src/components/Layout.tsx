import { useEffect, useRef, useState, type ReactNode } from "react";
import { NavLink, Outlet, useNavigate } from "react-router-dom";
import { BarChart3, Bell, Bot, CalendarDays, FileBarChart, LayoutDashboard, Moon, PenSquare, Search, Settings, Sun, Users } from "lucide-react";
import { api, type Platform } from "../lib/api";
import { useT, type Key } from "../lib/i18n";
import { useStore } from "../lib/store";
import { PLATFORM_META } from "../lib/platforms";
import { PlatformDot, cx } from "./ui";
import { openExternal } from "../lib/files";

const NAV: { group: Key; items: { to: string; label: Key | string; icon?: ReactNode; platform?: Platform }[] }[] = [
  { group: "nav.group.overview", items: [{ to: "/", label: "nav.dashboard", icon: <LayoutDashboard /> }, { to: "/accounts", label: "nav.accounts", icon: <Users /> }] },
  { group: "nav.group.platforms", items: (["instagram", "facebook", "youtube", "spotify"] as Platform[]).map((p) => ({ to: `/${p}`, label: PLATFORM_META[p].label, platform: p })) },
  { group: "nav.group.create", items: [{ to: "/studio", label: "nav.studio", icon: <PenSquare /> }, { to: "/calendar", label: "nav.calendar", icon: <CalendarDays /> }] },
  { group: "nav.group.insight", items: [{ to: "/analytics", label: "nav.analytics", icon: <BarChart3 /> }, { to: "/reports", label: "nav.reports", icon: <FileBarChart /> }] },
  { group: "nav.group.system", items: [{ to: "/automations", label: "nav.automations", icon: <Bot /> }, { to: "/notifications", label: "nav.notifications", icon: <Bell /> }, { to: "/settings", label: "nav.settings", icon: <Settings /> }] },
];

function Logo() {
  return (
    <svg viewBox="0 0 512 512" className="size-7" aria-hidden>
      <defs><linearGradient id="lg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#2dd4bf" /><stop offset=".55" stopColor="#7c5cff" /><stop offset="1" stopColor="#ff5d8f" /></linearGradient></defs>
      <path d="M40 256 C 120 130, 392 130, 472 256 C 392 382, 120 382, 40 256 Z" fill="none" stroke="url(#lg)" strokeWidth="36" />
      <circle cx="256" cy="256" r="86" fill="url(#lg)" /><circle cx="256" cy="256" r="38" fill="var(--bg)" />
    </svg>
  );
}

function SearchBox() {
  const t = useT();
  const nav = useNavigate();
  const { backend } = useStore();
  const [q, setQ] = useState("");
  const [open, setOpen] = useState(false);
  const [res, setRes] = useState<Awaited<ReturnType<typeof api.search>> | null>(null);
  const ref = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const key = (e: KeyboardEvent) => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); ref.current?.focus(); } };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, []);
  useEffect(() => {
    if (!backend || q.trim().length < 2) { setRes(null); return; }
    const h = setTimeout(() => api.search(q).then(setRes).catch(() => setRes(null)), 180);
    return () => clearTimeout(h);
  }, [q, backend]);

  // Pages are searchable too, so search works with no data.
  const pages = NAV.flatMap((g) => g.items).map((i) => ({ to: i.to, label: i.label.includes(".") ? t(i.label as Key) : i.label }))
    .filter((p) => q.trim().length >= 1 && p.label.toLowerCase().includes(q.toLowerCase()));
  const go = (to: string) => { setOpen(false); setQ(""); nav(to); };
  const has = pages.length || res?.accounts.length || res?.content.length || res?.drafts.length;

  return (
    <div className="relative w-full max-w-md">
      <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-[var(--muted)]" aria-hidden />
      <input ref={ref} value={q} onChange={(e) => { setQ(e.target.value); setOpen(true); }} onFocus={() => setOpen(true)} onBlur={() => setTimeout(() => setOpen(false), 150)}
        placeholder={`${t("top.search")}  (Ctrl+K)`} aria-label={t("top.search")}
        className="w-full rounded-lg border border-[var(--line)] bg-[var(--panel)] py-1.5 pr-3 pl-8 text-[13px] placeholder:text-[var(--muted)]" />
      {open && q && (
        <div className="absolute top-full z-40 mt-1 max-h-[60vh] w-full overflow-y-auto rounded-lg border border-[var(--line)] bg-[var(--panel)] p-1 shadow-xl">
          {!has && <div className="p-2 text-[var(--muted)]">{t("common.noResults")}</div>}
          {pages.map((p) => <SearchItem key={p.to} onClick={() => go(p.to)}>{p.label}</SearchItem>)}
          {res?.accounts.map((a) => <SearchItem key={`a${a.id}`} onClick={() => go(`/${a.platform}?account=${a.id}`)}><PlatformDot platform={a.platform} /> {a.name}{a.handle ? ` (@${a.handle.replace(/^@/, "")})` : ""}</SearchItem>)}
          {res?.content.map((c) => <SearchItem key={`c${c.id}`} onClick={() => c.url ? openExternal(c.url) : go(`/${c.platform}`)}><PlatformDot platform={c.platform} /> {c.title}</SearchItem>)}
          {res?.drafts.map((d) => <SearchItem key={`d${d.id}`} onClick={() => go(`/studio?draft=${d.id}`)}><PenSquare className="size-3.5" /> {d.title}</SearchItem>)}
        </div>
      )}
    </div>
  );
}

function SearchItem({ children, onClick }: { children: ReactNode; onClick: () => void }) {
  return <button type="button" onMouseDown={(e) => e.preventDefault()} onClick={onClick} className="flex w-full items-center gap-2 truncate rounded px-2 py-1.5 text-left hover:bg-[var(--panel-2)] cursor-pointer">{children}</button>;
}

export function Layout() {
  const t = useT();
  const { unread, dark, setSetting, backend } = useStore();
  return (
    <div className="flex h-full">
      <aside className="no-print flex w-[228px] shrink-0 flex-col border-r border-[var(--line)] bg-[var(--panel)]">
        <div className="flex items-center gap-2.5 px-4 py-4">
          <Logo /><span className="font-display text-[17px] font-semibold tracking-tight" style={{ fontFamily: "var(--font-display)" }}>SocialEye</span>
        </div>
        <nav className="flex-1 overflow-y-auto px-2 pb-4" aria-label="Main">
          {NAV.map((g) => (
            <div key={g.group} className="mt-3">
              <div className="px-2 pb-1 text-[11.5px] font-medium text-[var(--muted)]">{t(g.group)}</div>
              {g.items.map((i) => (
                <NavLink key={i.to} to={i.to} end={i.to === "/"}
                  className={({ isActive }) => cx("relative flex items-center gap-2.5 rounded-lg px-2 py-1.5 text-[13.5px] [&_svg]:size-4", isActive ? "bg-[var(--panel-2)] font-medium text-[var(--ink)] before:absolute before:top-1.5 before:bottom-1.5 before:-left-2 before:w-[3px] before:rounded-r before:bg-gradient-to-b before:from-teal before:via-iris before:to-rose" : "text-[var(--muted)] hover:text-[var(--ink)]")}>
                  {i.platform ? <PlatformDot platform={i.platform} className="mx-[3px]" /> : i.icon}
                  <span className="flex-1">{i.label.includes(".") ? t(i.label as Key) : i.label}</span>
                  {i.to === "/notifications" && unread > 0 && <span className="num rounded-full bg-iris px-1.5 text-[11px] text-white">{unread}</span>}
                </NavLink>
              ))}
            </div>
          ))}
        </nav>
      </aside>
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="no-print flex items-center gap-3 border-b border-[var(--line)] px-6 py-2.5">
          <SearchBox />
          <div className="flex-1" />
          <NavLink to="/notifications" aria-label={t("top.notifications")} className="relative rounded-lg p-2 text-[var(--muted)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)]">
            <Bell className="size-4" />{unread > 0 && <span className="absolute top-1 right-1 size-2 rounded-full bg-rose" />}
          </NavLink>
          <button type="button" aria-label={t("top.theme")} onClick={() => setSetting("theme", dark ? "light" : "dark")} className="rounded-lg p-2 text-[var(--muted)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)] cursor-pointer">
            {dark ? <Sun className="size-4" /> : <Moon className="size-4" />}
          </button>
        </header>
        <main className="print-area flex-1 overflow-y-auto px-6 py-6">
          {!backend && <div className="mb-4 rounded-lg border border-warn/40 bg-warn/10 px-3 py-2 text-[13px]">{t("common.backendMissing")}</div>}
          <div className="mx-auto max-w-[1280px]"><Outlet /></div>
        </main>
      </div>
    </div>
  );
}
