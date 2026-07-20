import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";

/** The updater only works inside the Tauri runtime (no-op in the browser). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** localStorage key remembering the last update version the user dismissed. */
const DISMISSED_UPDATE_KEY = "mxb-dismissed-update";

/**
 * How often to re-check for a new release while the app is running. The window
 * often stays open for days, so a single launch check isn't enough.
 */
const POLL_INTERVAL_MS = 6 * 60 * 60 * 1000; // 6 hours

type UpdateContextValue = {
  /** The newer signed build waiting to be installed, or null. */
  available: Update | null;
  /** True while a download+install is in progress. */
  installing: boolean;
  /** Download progress 0–100 while installing (null if unknown). */
  progress: number | null;
  /** Re-check GitHub Releases. Manual (silent:false) checks report either way. */
  check: (opts?: { silent?: boolean }) => Promise<void>;
  /** Download the update and relaunch into it. */
  install: () => Promise<void>;
  /** Hide the banner and don't resurface this version on future launches. */
  dismiss: () => void;
};

const UpdateContext = createContext<UpdateContextValue | null>(null);

export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const [available, setAvailable] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const inFlight = useRef(false);

  const check = useCallback(async ({ silent = false } = {}) => {
    if (!inTauri() || inFlight.current) return;
    inFlight.current = true;
    try {
      const update = await checkForUpdate();
      if (!update) {
        if (!silent) toast.success("You're on the latest version");
        setAvailable(null);
        return;
      }
      // On a silent (launch/poll) check, stay quiet if the user already
      // dismissed this exact version.
      if (silent && localStorage.getItem(DISMISSED_UPDATE_KEY) === update.version) {
        return;
      }
      setAvailable(update);
    } catch (e) {
      if (!silent) toast.error("Couldn't check for updates", { description: String(e) });
    } finally {
      inFlight.current = false;
    }
  }, []);

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
      toast.error("Update failed", { description: String(e) });
      setInstalling(false);
      setProgress(null);
    }
  }, [available, installing]);

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
    <UpdateContext.Provider
      value={{ available, installing, progress, check, install, dismiss }}
    >
      {children}
    </UpdateContext.Provider>
  );
}

export function useUpdate(): UpdateContextValue {
  const ctx = useContext(UpdateContext);
  if (!ctx) throw new Error("useUpdate must be used within an UpdateProvider");
  return ctx;
}
