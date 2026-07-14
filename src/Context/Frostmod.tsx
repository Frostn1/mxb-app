import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { isFrostmodRunning, reloadFrostmod } from "../api/mods";
import type { ReloadOutcome } from "../types";

interface FrostmodContextValue {
  /** Whether FrostMod is currently running (polled). `null` until first probe. */
  running: boolean | null;
  /** Manually ask FrostMod to live-reload the game now. */
  reload: () => Promise<ReloadOutcome>;
  /** Re-probe running status immediately. */
  refresh: () => void;
}

const FrostmodContext = createContext<FrostmodContextValue | null>(null);

const POLL_MS = 5000;

export function FrostmodProvider({ children }: { children: ReactNode }) {
  const [running, setRunning] = useState<boolean | null>(null);
  const mounted = useRef(true);

  const probe = useCallback(async () => {
    try {
      const r = await isFrostmodRunning();
      if (mounted.current) setRunning(r);
    } catch {
      if (mounted.current) setRunning(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    probe();
    const id = setInterval(probe, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [probe]);

  const reload = useCallback(async () => {
    const outcome = await reloadFrostmod();
    probe();
    return outcome;
  }, [probe]);

  const value = useMemo(
    () => ({ running, reload, refresh: probe }),
    [running, reload, probe],
  );

  return (
    <FrostmodContext.Provider value={value}>
      {children}
    </FrostmodContext.Provider>
  );
}

export function useFrostmod() {
  const ctx = useContext(FrostmodContext);
  if (!ctx) throw new Error("useFrostmod must be used within FrostmodProvider");
  return ctx;
}
