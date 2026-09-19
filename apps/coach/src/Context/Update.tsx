import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";
import { useT } from "@/i18n";

/** The updater only works inside the Tauri runtime (no-op in the browser). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** localStorage key remembering the last update version the user dismissed. */
const DISMISSED_UPDATE_KEY = "coach-dismissed-update";
const BETA_UPDATES_KEY = "coach-beta-updates";

/** The window stays open for days, so a launch check alone isn't enough. */
const POLL_INTERVAL_MS = 6 * 60 * 60 * 1000; // 6 hours

/** On until turned off: while the coach is in beta, its betas are its updates. */
export function betaUpdates(): boolean {
  try {
    return localStorage.getItem(BETA_UPDATES_KEY) !== "0";
  } catch {
    return true;
  }
}

export function setBetaUpdates(on: boolean) {
  try {
    localStorage.setItem(BETA_UPDATES_KEY, on ? "1" : "0");
  } catch {
    // Storage blocked: the default (on) stands.
  }
}

/** The coach releases from its own repo, Frostn1/mxb-coach, which this command reads. */
async function checkChannel(): Promise<Update | null> {
  const meta = await invoke<ConstructorParameters<typeof Update>[0] | null>("check_coach_update", {
    beta: betaUpdates(),
  });
  return meta ? new Update(meta) : null;
}

type UpdateContextValue = {
  /** The newer signed build waiting to be installed, or null. */
  available: Update | null;
  /** True while a download+install is in progress. */
  installing: boolean;
  /** Download progress 0–100 while installing (null if unknown). */
  progress: number | null;
  /** Re-check. Manual (silent:false) checks report either way. */
  check: (opts?: { silent?: boolean }) => Promise<void>;
  /** Download the update and relaunch into it. */
  install: () => Promise<void>;
  /** Hide the banner and don't resurface this version on future launches. */
  dismiss: () => void;
};

const UpdateContext = createContext<UpdateContextValue | null>(null);

/** The manager's updater, on the coach's own releases. */
export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const t = useT();
  const [available, setAvailable] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const inFlight = useRef(false);

  const check = useCallback(
    async ({ silent = false } = {}) => {
      if (!inTauri() || inFlight.current) return;
      inFlight.current = true;
      try {
        const update = await checkChannel();
        if (!update) {
          if (!silent) toast.success(t("update.onLatest"));
          setAvailable(null);
          return;
        }
        // A launch or poll check stays quiet about a version the user already dismissed.
        if (silent && localStorage.getItem(DISMISSED_UPDATE_KEY) === update.version) return;
        setAvailable(update);
      } catch (e) {
        if (!silent) toast.error(t("update.checkFailed"), { description: String(e) });
      } finally {
        inFlight.current = false;
      }
    },
    [t],
  );

  const install = useCallback(async () => {
    if (!available || installing) return;
    setInstalling(true);
    setProgress(null);
    try {
      let total = 0;
      let downloaded = 0;
      await available.downloadAndInstall((event) => {
        switch (event.event) {
          case "Started":
            total = event.data.contentLength ?? 0;
            break;
          case "Progress":
            downloaded += event.data.chunkLength;
            if (total > 0) setProgress(Math.round((downloaded / total) * 100));
            break;
          case "Finished":
            setProgress(100);
            break;
        }
      });
      await relaunch();
    } catch (e) {
      toast.error(t("update.failed"), { description: String(e) });
      setInstalling(false);
      setProgress(null);
    }
  }, [available, installing, t]);

  const dismiss = useCallback(() => {
    if (available) localStorage.setItem(DISMISSED_UPDATE_KEY, available.version);
    setAvailable(null);
  }, [available]);

  // Check once on launch, then poll while the app stays open.
  useEffect(() => {
    void check({ silent: true });
    const id = setInterval(() => void check({ silent: true }), POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [check]);

  return (
    <UpdateContext.Provider value={{ available, installing, progress, check, install, dismiss }}>
      {children}
    </UpdateContext.Provider>
  );
}

export function useUpdate(): UpdateContextValue {
  const ctx = useContext(UpdateContext);
  if (!ctx) throw new Error("useUpdate must be used within an UpdateProvider");
  return ctx;
}
