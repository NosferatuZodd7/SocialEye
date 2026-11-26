import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import { api, isTauri, type Account } from "./api";
import type { Lang } from "./format";

interface Store {
  accounts: Account[];
  settings: Record<string, string>;
  unread: number;
  ready: boolean;
  backend: boolean;
  syncing: Set<number>;
  reloadAccounts: () => Promise<void>;
  reloadUnread: () => Promise<void>;
  setSetting: (k: string, v: string) => Promise<void>;
  lang: Lang;
  dark: boolean;
}

const Ctx = createContext<Store | null>(null);

export function StoreProvider({ children }: { children: ReactNode }) {
  const backend = isTauri();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [unread, setUnread] = useState(0);
  const [ready, setReady] = useState(!backend);
  const [syncing, setSyncing] = useState<Set<number>>(new Set());
  const [systemDark, setSystemDark] = useState(() => window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true);

  const reloadAccounts = useCallback(async () => { if (backend) setAccounts(await api.accounts()); }, [backend]);
  const reloadUnread = useCallback(async () => {
    if (backend) setUnread((await api.notifications()).filter((n) => !n.read).length);
  }, [backend]);

  useEffect(() => {
    if (!backend) return;
    Promise.all([api.settings().then(setSettings), reloadAccounts(), reloadUnread()]).finally(() => setReady(true));
    let unlisten: (() => void)[] = [];
    import("@tauri-apps/api/event").then(async ({ listen }) => {
      unlisten = [
        await listen("notifications-changed", () => void reloadUnread()),
        await listen<{ account_id: number; running: boolean }>("sync-state", (e) => {
          setSyncing((s) => { const n = new Set(s); if (e.payload.running) n.add(e.payload.account_id); else n.delete(e.payload.account_id); return n; });
          if (!e.payload.running) void reloadAccounts();
        }),
      ];
    });
    return () => unlisten.forEach((u) => u());
  }, [backend, reloadAccounts, reloadUnread]);

  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    const h = (e: MediaQueryListEvent) => setSystemDark(e.matches);
    mq?.addEventListener("change", h);
    return () => mq?.removeEventListener("change", h);
  }, []);

  const setSetting = useCallback(async (k: string, v: string) => {
    if (backend) await api.setSetting(k, v);
    setSettings((s) => ({ ...s, [k]: v }));
  }, [backend]);

  const theme = settings.theme ?? "system";
  const dark = theme === "dark" || (theme === "system" && systemDark);
  const lang: Lang = settings.language === "pt" ? "pt" : settings.language === "en" ? "en" : navigator.language?.startsWith("pt") ? "pt" : "en";

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
    document.documentElement.lang = lang === "pt" ? "pt-PT" : "en";
  }, [dark, lang]);

  return <Ctx.Provider value={{ accounts, settings, unread, ready, backend, syncing, reloadAccounts, reloadUnread, setSetting, lang, dark }}>{children}</Ctx.Provider>;
}

export function useStore() {
  const s = useContext(Ctx);
  if (!s) throw new Error("useStore outside StoreProvider");
  return s;
}
