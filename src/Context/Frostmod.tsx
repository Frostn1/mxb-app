import {
  useCallback,
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
  MODS_WATCH_SLUG,
  onFrostmodReload,
  reloadFrostmod,
} from "../api/mods";
import type { FrostmodStatus } from "../types";
import { displayName } from "../lib/mods";
import { FrostmodContext } from "./FrostmodContext";

const POLL_MS = 5000;

/**
 * Name what the folder watcher picked up. The watcher reports mods as `<type>/<name>`;
 * showing them beats a generic "your folder changed", which left people unsure whether
 * the drop had been seen at all.
 */
function watchDescription(mods: string[]): string {
  const asked = "Asked FrostMod to reload the game.";
  if (mods.length === 0) return asked;
  const names = mods.map((m) => displayName(m.split("/").pop() ?? m));
  const shown = names.slice(0, 3).join(", ");
  const rest = names.length - 3;
  return `${rest > 0 ? `${shown} and ${rest} more` : shown} — ${asked.toLowerCase()}`;
}

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

  // Surface reloads the mods-folder watcher triggers (a manual download dropped into
  // the folder). In-app installs carry their own slug and toast, so we only react to
  // the watcher's sentinel here.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void onFrostmodReload((p) => {
      if (p.slug !== MODS_WATCH_SLUG) return;
      if (p.outcome === "signaled") {
        // "Added", not "loaded": signalling FrostMod only tells us its reload event
        // exists and was poked — whether the game picked the mods up is FrostMod's to
        // report, and claiming otherwise is what makes a failed reload so confusing.
        const mods = p.mods ?? [];
        toast.success(
          mods.length === 1 ? "New mod added" : `${mods.length || "New"} mods added`,
          { description: watchDescription(mods) },
        );
      }
      void probe();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [probe]);

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
