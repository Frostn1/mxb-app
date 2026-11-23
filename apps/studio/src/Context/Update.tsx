import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { check as checkForUpdate, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";
import { useT } from "@/i18n";

/** The updater only works inside the Tauri runtime (no-op in the browser). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** localStorage key remembering the last update version the user dismissed. */
const DISMISSED_UPDATE_KEY = "studio-dismissed-update";

/** The window stays open for days, so a launch check alone isn't enough. */
const POLL_INTERVAL_MS = 6 * 60 * 60 * 1000; // 6 hours

type UpdateContextValue = {
  /** The newer signed build waiting to be installed, or null. */
  available: Update | null;
  /** True while a download+install is in progress. */
  installing: boolean;
  /** Download progress 0–100 while installing (null if unknown). */
  progress: number | null;
  /** Download the update and relaunch into it. */
  install: () => Promise<void>;
  /** Hide the banner and don't resurface this version on future launches. */
  dismiss: () => void;
};

const UpdateContext = createContext<UpdateContextValue | null>(null);

/**
 * The manager's updater, minus its beta channel: Frost's Studio only ever takes
 * `releases/latest` from Frostn1/frost-studio.
 */
export function UpdateProvider({ children }: { children: React.ReactNode }) {
  const t = useT();
  const [available, setAvailable] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const inFlight = useRef(false);

  // Silent by design: a failed background check is not worth a toast.
  const check = useCallback(async () => {
    if (!inTauri() || inFlight.current) return;
    inFlight.current = true;
    try {
      const update = await checkForUpdate();
      if (!update || localStorage.getItem(DISMISSED_UPDATE_KEY) === update.version) {
        setAvailable(null);
        return;
      }
      setAvailable(update);
    } catch {
      // Offline, or GitHub unreachable — try again on the next poll.
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
    void check();
    const id = setInterval(() => void check(), POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [check]);

  return (
    <UpdateContext.Provider value={{ available, installing, progress, install, dismiss }}>
      {children}
    </UpdateContext.Provider>
  );
}

export function useUpdate(): UpdateContextValue {
  const ctx = useContext(UpdateContext);
  if (!ctx) throw new Error("useUpdate must be used within an UpdateProvider");
  return ctx;
}
