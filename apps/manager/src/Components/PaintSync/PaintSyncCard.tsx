import { useState } from "react";
import { Loader2, Palette } from "lucide-react";
import { toast } from "sonner";
import { setPaintSyncModels } from "@frost/shared/api/mods";
import { Button } from "@frost/shared/Components/ui/button";
import { Switch } from "@frost/shared/Components/ui/switch";
import { useConfig } from "@frost/shared/Context/Config";
import { cn } from "@frost/shared/lib/utils";
import { useT, type TKey } from "@/i18n";
import { usePaintSyncSetup, type PaintSyncStatus } from "../../lib/usePaintSyncSetup";
import PaintSyncConfirm from "./PaintSyncConfirm";

const STATUS_LABEL: Record<PaintSyncStatus, TKey> = {
  on: "paintSync.statusOn",
  paused: "paintSync.statusPaused",
  off: "paintSync.statusOff",
  needsIntegration: "paintSync.statusNeedsIntegration",
  unavailable: "paintSync.statusUnavailable",
};

/**
 * The card at the top of Settings: what paint sync does, whether it's on, what's missing,
 * and one button that turns on everything it needs.
 *
 * Paint sync used to be a switch in Settings → Advanced, greyed out until Game Integration was
 * installed and set to start, with the fix described in a sentence. Most players never found
 * it, and those who did had to go and flip two other switches first. This says the same
 * things up front and does the flipping, after a confirm that lists it.
 */
export default function PaintSyncCard() {
  const t = useT();
  const { config, reloadConfig } = useConfig();
  const setup = usePaintSyncSetup();
  const { status, busy, turnOff } = setup;
  const [confirming, setConfirming] = useState(false);
  const [modelsBusy, setModelsBusy] = useState(false);

  const on = status === "on";
  const canTurnOn = status === "off" || status === "needsIntegration" || status === "paused";
  const modelsOn = config.paintSyncModels ?? false;

  const changeModels = async (enabled: boolean) => {
    setModelsBusy(true);
    try {
      await setPaintSyncModels(enabled);
      await reloadConfig();
    } catch (error) {
      toast.error(String(error));
    } finally {
      setModelsBusy(false);
    }
  };

  return (
    <div className="flex min-w-0 flex-col gap-3 rounded-xl border border-border bg-card p-[18px]">
      <div className="flex items-start gap-3">
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <div className="flex items-center gap-2">
            <Palette className="size-4 text-muted-foreground" />
            <span className="text-[15px] font-bold">{t("settings.paintSync")}</span>
            {status && (
              <span
                className={cn(
                  "flex items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] font-semibold",
                  on
                    ? "bg-success/10 text-success"
                    : status === "paused"
                      ? "bg-warning/10 text-warning"
                      : "bg-foreground/[0.06] text-muted-foreground",
                )}
              >
                <span
                  className={cn(
                    "size-[6px] rounded-full",
                    on ? "bg-success" : status === "paused" ? "bg-warning" : "bg-muted-foreground/60",
                  )}
                />
                {t(STATUS_LABEL[status])}
              </span>
            )}
          </div>
          <span className="text-[12.5px] text-muted-foreground">{t("paintSync.what")}</span>
        </div>
        {/* Once it's on, the way back is the plain switch every other setting uses. */}
        {on && (
          <div className="pt-1">
            <Switch
              checked
              onCheckedChange={(v) => !v && void turnOff()}
              aria-label={t("settings.paintSyncOn")}
            />
          </div>
        )}
      </div>

      {canTurnOn && (
        <div className="flex flex-wrap items-center gap-3">
          <Button size="sm" onClick={() => setConfirming(true)} disabled={busy}>
            {busy && <Loader2 className="size-3.5 animate-spin" />}
            {t(status === "paused" ? "paintSync.fix" : "paintSync.turnOn")}
          </Button>
          {status !== "off" && (
            <span className="text-[11.5px] text-muted-foreground">
              {t("paintSync.needsIntegrationHint")}
            </span>
          )}
        </div>
      )}
      {status === "paused" && (
        <div>
          <button
            type="button"
            onClick={() => void turnOff()}
            className="cursor-default text-[11.5px] font-semibold text-muted-foreground hover:text-foreground"
          >
            {t("paintSync.turnOff")}
          </button>
        </div>
      )}

      <div className="flex items-start gap-3 border-t border-border/60 pt-3">
        <div className="min-w-0 flex-1">
          <div className="text-[12.5px] font-semibold">{t("paintSync.models")}</div>
          <div className="mt-0.5 text-[11.5px] leading-relaxed text-muted-foreground">
            {t("paintSync.modelsHelp")}
          </div>
        </div>
        <Switch
          checked={modelsOn}
          disabled={!config.paintSyncEnabled || modelsBusy}
          onCheckedChange={(enabled) => void changeModels(enabled)}
          aria-label={t("paintSync.models")}
        />
      </div>

      <PaintSyncConfirm open={confirming} onOpenChange={setConfirming} setup={setup} />
    </div>
  );
}
