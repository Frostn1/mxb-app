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
import { toast } from "sonner";
import {
  frostmodInstall,
  frostmodStart,
  frostmodStatus,
  isFrostmodRunning,
  reloadFrostmod,
} from "../api/mods";
import type { FrostmodStatus, ReloadOutcome } from "../types";

interface FrostmodContextValue {
  /** Whether FrostMod is currently running (polled). `null` until first probe. */
  running: boolean | null;
  /** Install/version snapshot (`null` until first fetched). */
  status: FrostmodStatus | null;
  /** True while an install/update download is in flight. */
  installing: boolean;
  /** Manually ask FrostMod to live-reload the game now. */
  reload: () => Promise<ReloadOutcome>;
  /** Re-probe running status immediately. */
  refresh: () => void;
  /** Re-fetch the full install/version status (hits GitHub). */
  refreshStatus: () => Promise<void>;
  /** Download the latest FrostMod, then start it. */
  install: () => Promise<void>;
}

const FrostmodContext = createContext<FrostmodContextValue | null>(null);

const POLL_MS = 5000;

export function FrostmodProvider({ children }: { children: ReactNode }) {
  const [running, setRunning] = useState<boolean | null>(null);
  const [status, setStatus] = useState<FrostmodStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const mounted = useRef(true);

  const probe = useCallback(async () => {
    try {
      const r = await isFrostmodRunning();
      if (mounted.current) setRunning(r);
    } catch {
      if (mounted.current) setRunning(false);
    }
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await frostmodStatus();
      if (mounted.current) {
        setStatus(s);
        setRunning(s.running);
      }
    } catch {
      /* offline / non-Tauri — leave prior status */
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    probe();
    void refreshStatus();
    const id = setInterval(probe, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [probe, refreshStatus]);

  const reload = useCallback(async () => {
    const outcome = await reloadFrostmod();
    probe();
    return outcome;
  }, [probe]);

  const install = useCallback(async () => {
    setInstalling(true);
    try {
      const version = await frostmodInstall();
      await frostmodStart().catch(() => {
        /* start is Windows-only; the download still succeeded */
      });
      await refreshStatus();
      toast.success(`FrostMod ${version} installed`, {
        description: "It'll live-reload the game when you add mods.",
      });
    } catch (e) {
      toast.error("Couldn't install FrostMod", { description: String(e) });
    } finally {
      setInstalling(false);
    }
  }, [refreshStatus]);

  const value = useMemo(
    () => ({
      running,
      status,
      installing,
      reload,
      refresh: probe,
      refreshStatus,
      install,
    }),
    [running, status, installing, reload, probe, refreshStatus, install],
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
