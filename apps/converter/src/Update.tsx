import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { check as checkForUpdate, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { ArrowUpCircle, Loader2, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";

const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The last version dismissed, not offered again. */
const DISMISSED = "converter-dismissed-update";
/** The window can stay open for days, so a launch check alone isn't enough. */
const POLL_MS = 6 * 60 * 60 * 1000;

interface UpdateState {
  available: Update | null;
  installing: boolean;
  progress: number | null;
  install: () => Promise<void>;
  dismiss: () => void;
}

const UpdateContext = createContext<UpdateState | null>(null);

/** The Studio's updater: `releases/latest` from Frostn1/frost-converter, signed. */
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
      setAvailable(update && localStorage.getItem(DISMISSED) !== update.version ? update : null);
    } catch {
      // Offline, or nothing released yet: try again on the next poll.
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
        if (event.event === "Started") total = event.data.contentLength ?? 0;
        else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) setProgress(Math.round((downloaded / total) * 100));
        } else setProgress(100);
      });
      await relaunch();
    } catch (e) {
      toast.error(t("update.failed"), { description: String(e) });
      setInstalling(false);
      setProgress(null);
    }
  }, [available, installing, t]);

  const dismiss = useCallback(() => {
    if (available) localStorage.setItem(DISMISSED, available.version);
    setAvailable(null);
  }, [available]);

  useEffect(() => {
    void check();
    const id = setInterval(() => void check(), POLL_MS);
    return () => clearInterval(id);
  }, [check]);

  return <UpdateContext.Provider value={{ available, installing, progress, install, dismiss }}>{children}</UpdateContext.Provider>;
}

/** The Studio's update bar: the version and one button. Nothing when there is no update. */
export function UpdateBanner() {
  const t = useT();
  const ctx = useContext(UpdateContext);
  if (!ctx?.available) return null;
  const { available, installing, progress, install, dismiss } = ctx;
  return (
    <div className="flex shrink-0 items-center gap-3 border-b border-primary/25 bg-primary/10 px-4 py-2 text-sm text-foreground">
      <ArrowUpCircle className="size-4 shrink-0 text-primary" />
      <span className="min-w-0 truncate font-semibold">{t("update.available", { version: `v${available.version}` })}</span>
      {installing && (
        <span className="text-muted-foreground">
          {progress != null ? t("update.downloadingPct", { pct: progress }) : t("update.downloading")}
        </span>
      )}
      <div className="ml-auto flex shrink-0 items-center gap-1.5">
        <Button size="sm" onClick={() => void install()} disabled={installing}>
          {installing ? <Loader2 className="size-3.5 animate-spin" /> : <ArrowUpCircle className="size-3.5" />}
          {installing ? t("update.updating") : t("update.updateAndRestart")}
        </Button>
        <Button size="icon" variant="ghost" className="size-8" onClick={dismiss} disabled={installing} aria-label={t("update.dismiss")}>
          <X className="size-4" />
        </Button>
      </div>
    </div>
  );
}
