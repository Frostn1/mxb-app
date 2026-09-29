import { useCallback, useEffect, useRef, useState } from "react";
import { errorText } from "./api";

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
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [at, setAt] = useState<number | null>(null);
  const busy = useRef(false);
  const loadRef = useRef(load);
  loadRef.current = load;

  const run = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    setLoading(true);
    try {
      setData(await loadRef.current());
      setError(null);
      setAt(Date.now());
    } catch (e) {
      setError(errorText(e));
    } finally {
      busy.current = false;
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    setData(null);
    setError(null);
    setAt(null);
    void run();
    if (everyMs <= 0) return;
    const timer = window.setInterval(() => void run(), everyMs);
    return () => window.clearInterval(timer);
  }, [key, everyMs, run]);

  return { data, error, loading, at, refresh: () => void run() };
}
