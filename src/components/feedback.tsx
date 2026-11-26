import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { Button, Modal } from "./ui";
import { useT } from "../lib/i18n";

interface Feedback {
  confirm: (message: string, opts?: { danger?: boolean; confirmLabel?: string }) => Promise<boolean>;
  toast: (message: string, tone?: "ok" | "bad") => void;
}
const Ctx = createContext<Feedback>({ confirm: async () => false, toast: () => {} });

export function FeedbackProvider({ children }: { children: ReactNode }) {
  const t = useT();
  const [ask, setAsk] = useState<{ message: string; danger?: boolean; confirmLabel?: string } | null>(null);
  const resolver = useRef<(v: boolean) => void>(() => {});
  const [toasts, setToasts] = useState<{ id: number; message: string; tone: "ok" | "bad" }[]>([]);

  const confirm = useCallback((message: string, opts?: { danger?: boolean; confirmLabel?: string }) => {
    setAsk({ message, ...opts });
    return new Promise<boolean>((r) => (resolver.current = r));
  }, []);
  const done = (v: boolean) => { resolver.current(v); setAsk(null); };
  const toast = useCallback((message: string, tone: "ok" | "bad" = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((x) => [...x, { id, message, tone }]);
    setTimeout(() => setToasts((x) => x.filter((y) => y.id !== id)), 5000);
  }, []);

  return (
    <Ctx.Provider value={{ confirm, toast }}>
      {children}
      <Modal open={!!ask} onClose={() => done(false)} title={t("common.confirm")}>
        <p className="mb-4 whitespace-pre-line">{ask?.message}</p>
        <div className="flex justify-end gap-2">
          <Button data-close onClick={() => done(false)}>{t("common.cancel")}</Button>
          <Button variant={ask?.danger ? "danger" : "primary"} onClick={() => done(true)}>{ask?.confirmLabel ?? t("common.confirm")}</Button>
        </div>
      </Modal>
      <div aria-live="polite" className="no-print fixed right-4 bottom-4 z-[60] flex flex-col gap-2">
        {toasts.map((x) => (
          <div key={x.id} className={`max-w-sm rounded-lg border px-3 py-2 text-[13px] shadow-lg ${x.tone === "ok" ? "border-ok/30 bg-[var(--panel)]" : "border-bad/40 bg-[var(--panel)] text-bad"}`}>{x.message}</div>
        ))}
      </div>
    </Ctx.Provider>
  );
}

export const useFeedback = () => useContext(Ctx);
