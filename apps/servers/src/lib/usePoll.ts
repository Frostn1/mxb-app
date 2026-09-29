import { useCallback, useEffect, useRef, useState } from "react";
import { errorText } from "./api";

const cache = new Map<string, { data: unknown; at: number }>();

export interface Poll<T> {
  data: T | null;
  error: string | null;
  loading: boolean;
  /** When the last successful answer arrived. */
  at: number | null;
  refresh: () => void;
}

/** Calls `load` now and every `everyMs` (0: only on refresh), never two at once. Keeps the last
 *  good data while an error is showing, so a blip doesn't blank the screen. */
export function usePoll<T>(load: () => Promise<T>, everyMs: number, key: string): Poll<T> {
  const initial = cache.get(key);
  const [data, setData] = useState<T | null>((initial?.data as T | undefined) ?? null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [at, setAt] = useState<number | null>(initial?.at ?? null);
  const busy = useRef(false);
  const loadRef = useRef(load);
  loadRef.current = load;

  const run = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setLoading(true);
    try {
      const next = await loadRef.current();
      const received = Date.now();
      cache.set(key, { data: next, at: received });
      setData(next);
      setError(null);
      setAt(received);
    } catch (e) {
      setError(errorText(e));
    } finally {
      busy.current = false;
      setLoading(false);
    }
  }, [key]);

  useEffect(() => {
    const cached = cache.get(key);
    setData((cached?.data as T | undefined) ?? null);
    setError(null);
    setAt(cached?.at ?? null);
    void run();
    if (everyMs <= 0) return;
    const timer = window.setInterval(() => void run(), everyMs);
    return () => window.clearInterval(timer);
  }, [key, everyMs, run]);

  return { data, error, loading, at, refresh: () => void run() };
}
