import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { check as checkForUpdate, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { toast } from "sonner";
import { getConfig, isGameRunning } from "@frost/shared/api/mods";
import { useT } from "@/i18n";

/** The updater only works inside the Tauri runtime (no-op in the browser). */
function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** Beta channel: the app finds the newest release, pre-releases included, and the plugin
 *  installs it. Stable stays on the plugin's own `releases/latest` check. */
async function checkBeta(): Promise<Update | null> {
  const meta = await invoke<ConstructorParameters<typeof Update>[0] | null>(
    "check_beta_update",
  );
  return meta ? new Update(meta) : null;
}

/** localStorage key remembering the last update version the user dismissed. */
const DISMISSED_UPDATE_KEY = "mxb-dismissed-update";

/**
 * How often to re-check for a new release while the app is running. The window
 * often stays open for days, so a single launch check isn't enough.
 */
const POLL_INTERVAL_MS = 6 * 60 * 60 * 1000; // 6 hours

/** No input for this long and the app counts as unused, so restarting it costs nothing. */
const IDLE_MS = 10 * 60 * 1000;

/** How often a waiting update looks for that idle moment. */
const IDLE_CHECK_MS = 60 * 1000;

/** Whether an update may install without asking right now: the setting is on and the game
 *  isn't running. A probe that fails counts as running. */
async function mayAutoInstall(): Promise<boolean> {
  const cfg = await getConfig().catch(() => null);
  if (cfg?.autoUpdates === false) return false;
  return !(await isGameRunning().catch(() => true));
}

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
  const t = useT();
  const [available, setAvailable] = useState<Update | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const inFlight = useRef(false);
  const busy = useRef(false);
  /** The newest update found, dismissed or not: an automatic install ignores the banner. */
  const pending = useRef<Update | null>(null);
  /** Version already downloaded, so a deferred install doesn't fetch it twice. */
  const downloaded = useRef<string | null>(null);
  /** Versions whose automatic install failed: left to the banner until the next launch. */
  const autoFailed = useRef(new Set<string>());
  const lastInput = useRef(Date.now());

  /** Download `update` and restart into it. `auto` installs stay quiet on failure, and
   *  back off if the game started while the download ran. */
  const apply = useCallback(
    async (update: Update, auto: boolean) => {
      if (busy.current) return;
      busy.current = true;
      setAvailable(update);
      setInstalling(true);
      setProgress(null);
      try {
        if (downloaded.current !== update.version) {
          let total = 0;
          let received = 0;
          await update.download((event) => {
            switch (event.event) {
              case "Started":
                total = event.data.contentLength ?? 0;
                break;
              case "Progress":
                received += event.data.chunkLength;
                if (total > 0) setProgress(Math.round((received / total) * 100));
                break;
              case "Finished":
                setProgress(100);
                break;
            }
          });
          downloaded.current = update.version;
        }
        if (auto && !(await mayAutoInstall())) {
          busy.current = false;
          setInstalling(false);
          setProgress(null);
          return;
        }
        // Updating from the tray: the restart goes back there.
        await invoke("park_for_update", { parked: auto });
        await update.install();
        await relaunch();
      } catch (e) {
        void invoke("park_for_update", { parked: false }).catch(() => {});
        if (auto) {
          autoFailed.current.add(update.version);
          console.warn("automatic update failed", e);
        } else {
          toast.error(t("update.failed"), { description: String(e) });
        }
        busy.current = false;
        setInstalling(false);
        setProgress(null);
      }
    },
    [t],
  );

  const check = useCallback(
    async ({ silent = false, launch = false } = {}) => {
      if (!inTauri() || inFlight.current) return;
      inFlight.current = true;
      try {
        // A config that can't be read falls back to stable, never to no updates at all.
        const beta = (await getConfig().catch(() => null))?.betaUpdates ?? false;
        const update = beta ? await checkBeta() : await checkForUpdate();
        pending.current = update;
        if (!update) {
          if (!silent) toast.success(t("update.onLatest"));
          setAvailable(null);
          return;
        }
        // Just opened, so nothing is in use yet: the moment to update.
        if (launch && (await mayAutoInstall())) {
          void apply(update, true);
          return;
        }
        // On a silent (launch/poll) check, stay quiet if the user already
        // dismissed this exact version.
        if (silent && localStorage.getItem(DISMISSED_UPDATE_KEY) === update.version) {
          return;
        }
        setAvailable(update);
      } catch (e) {
        if (!silent)
          toast.error(t("update.checkFailed"), { description: String(e) });
      } finally {
        inFlight.current = false;
      }
    },
    [t, apply],
  );

  const install = useCallback(async () => {
    if (available) await apply(available, false);
  }, [available, apply]);

  const dismiss = useCallback(() => {
    if (available) localStorage.setItem(DISMISSED_UPDATE_KEY, available.version);
    setAvailable(null);
  }, [available]);

  // Check once on launch, then poll while the app stays open.
  useEffect(() => {
    void check({ silent: true, launch: true });
    const id = setInterval(() => void check({ silent: true }), POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [check]);

  // An update found while the app is open installs once it has sat unused. A window
  // parked in the tray gets no input, so it counts as unused too.
  useEffect(() => {
    if (!inTauri()) return;
    const touch = () => {
      lastInput.current = Date.now();
    };
    const events = ["pointerdown", "pointermove", "keydown", "wheel"] as const;
    for (const e of events) window.addEventListener(e, touch, { passive: true });
    const id = setInterval(async () => {
      const update = pending.current;
      if (!update || busy.current || autoFailed.current.has(update.version)) return;
      if (Date.now() - lastInput.current < IDLE_MS) return;
      if (await mayAutoInstall()) void apply(update, true);
    }, IDLE_CHECK_MS);
    return () => {
      for (const e of events) window.removeEventListener(e, touch);
      clearInterval(id);
    };
  }, [apply]);

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
