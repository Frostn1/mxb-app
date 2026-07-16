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
  /** True while a `refreshStatus` GitHub check is in flight. */
  checking: boolean;
  /** True when the last `refreshStatus` failed (offline / GitHub error). */
  statusError: boolean;
  /** Manually ask FrostMod to live-reload the game now. */
  reload: () => Promise<ReloadOutcome>;
  /** Re-probe running status immediately. */
  refresh: () => void;
  /** Re-fetch the full install/version status (hits GitHub). */
  refreshStatus: () => Promise<void>;
  /** Download the latest FrostMod, then start it. */
  install: () => Promise<void>;
  /** Launch FrostMod now if it isn't already running. */
  start: () => Promise<void>;
}

const FrostmodContext = createContext<FrostmodContextValue | null>(null);

const POLL_MS = 5000;

export function FrostmodProvider({ children }: { children: ReactNode }) {
  const [running, setRunning] = useState<boolean | null>(null);
  const [status, setStatus] = useState<FrostmodStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [checking, setChecking] = useState(false);
  const [statusError, setStatusError] = useState(false);
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
    if (mounted.current) setChecking(true);
    try {
      const s = await frostmodStatus();
      if (mounted.current) {
        setStatus(s);
        setRunning(s.running);
        // A successful call always yields a `latest` tag; a null one means the
        // GitHub check inside `status()` failed even though the command returned.
        setStatusError(s.latest === null);
      }
    } catch {
      /* offline / non-Tauri — leave prior status but flag the failure */
      if (mounted.current) setStatusError(true);
    } finally {
      if (mounted.current) setChecking(false);
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

  const start = useCallback(async () => {
    try {
      const started = await frostmodStart();
      await probe();
      if (started) toast.success("FrostMod started");
      else toast.info("FrostMod is already running");
    } catch (e) {
      toast.error("Couldn't start FrostMod", { description: String(e) });
    }
  }, [probe]);

  const install = useCallback(async () => {
    setInstalling(true);
    try {
      // The backend stops a running FrostMod before overwriting (its exe/dll are
      // locked while running) and restarts it after — so we don't start it here,
      // which would race that restart and spawn a second instance.
      const version = await frostmodInstall();
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

  // FrostMod is core to the app, so set it up automatically on first run instead
  // of prompting: once we learn it isn't installed, download + start it silently.
  // Runs at most once; a failed status check (`statusError`) is skipped so we only
  // auto-install off a confirmed "not installed" snapshot, never an offline guess.
  const autoInstallTried = useRef(false);
  useEffect(() => {
    if (
      !autoInstallTried.current &&
      status &&
      !status.installed &&
      !statusError &&
      !installing
    ) {
      autoInstallTried.current = true;
      void install();
    }
  }, [status, statusError, installing, install]);

  const value = useMemo(
    () => ({
      running,
      status,
      installing,
      checking,
      statusError,
      reload,
      refresh: probe,
      refreshStatus,
      install,
      start,
    }),
    [running, status, installing, checking, statusError, reload, probe, refreshStatus, install, start],
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
