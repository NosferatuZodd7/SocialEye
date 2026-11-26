import { useCallback, useEffect, useRef, useState } from "react";
import { errorText } from "./api";

/** Load data with loading/error state and a reload function. */
export function useLoad<T>(fn: () => Promise<T>, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const seq = useRef(0);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const run = useCallback(fn, deps);
  const reload = useCallback(async () => {
    const id = ++seq.current;
    setLoading(true);
    try {
      const v = await run();
      if (id === seq.current) { setData(v); setError(null); }
    } catch (e) {
      if (id === seq.current) setError(errorText(e));
    } finally {
      if (id === seq.current) setLoading(false);
    }
  }, [run]);
  useEffect(() => { void reload(); }, [reload]);
  return { data, error, loading, reload, setData };
}

/** Run an action with busy + error state. */
export function useAction() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    setBusy(true); setError(null);
    try { return await fn(); } catch (e) { setError(errorText(e)); return undefined; } finally { setBusy(false); }
  }, []);
  return { busy, error, run, setError };
}
