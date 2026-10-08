import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  paintSyncReadiness,
  setPaintSyncEnabled,
  type PaintSyncReadiness,
} from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { usePlatform } from "@frost/shared/lib/usePlatform";
import { useT, type TKey } from "@/i18n";
import { useFrostmod } from "../Context/FrostmodContext";

/**
 * Where paint sync stands, in the words the Settings card, the confirm and the tip use.
 *
 * - `on`: switched on and able to run.
 * - `paused`: switched on, but Game Integration is missing or not set to start, so nothing
 *   runs. The backend's `paint_sync_active` treats this exactly like off.
 * - `off`: switched off, and everything it needs is already in place.
 * - `needsIntegration`: switched off, and Game Integration would have to come on too.
 * - `unavailable`: this game or platform has no Game Integration, so paint sync can't run.
 */
export type PaintSyncStatus = "on" | "paused" | "off" | "needsIntegration" | "unavailable";

/**
 * Paint sync's state, and one action that turns on everything it needs.
 *
 * Paint sync rides on Game Integration (FrostMod): the server a rider picked in the game's own
 * browser, and the moment they leave it, are only known through it. So "turn on paint sync"
 * on a machine without it means: record the Game Integration opt-in, install or update it,
 * set it to start with the app, start it, and then flip the paint sync switch. The first
 * four are exactly what the Game Integration consent's Enable button does
 * (`enableIntegration`), so this calls that rather than doing any of it twice.
 */
export function usePaintSyncSetup() {
  const t = useT();
  const { config, reloadConfig, game } = useConfig();
  const platform = usePlatform();
  const { enableIntegration, status: frostmodStatus, integrationChoice } = useFrostmod();
  const [readiness, setReadiness] = useState<PaintSyncReadiness | null>(null);
  const [busy, setBusy] = useState(false);

  // FrostMod is a Win32 DLL: it runs wherever the game does (Windows, Proton, a Mac bottle),
  // but has no build for a title without the `frostmod` capability.
  const supported =
    (platform === "windows" || platform === "linux" || platform === "macos") &&
    Boolean(game.caps.frostmod);

  const refresh = useCallback(() => {
    paintSyncReadiness()
      .then(setReadiness)
      .catch(() => {});
  }, []);
  // Re-asked whenever Game Integration moves: installed, updated, or switched on or off.
  useEffect(refresh, [
    refresh,
    frostmodStatus?.installed,
    frostmodStatus?.version,
    integrationChoice,
    config.autoRunFrostmod,
  ]);

  const enabled = config.paintSyncEnabled ?? false;
  const blocked = readiness?.ready === false;
  // Unknown until the platform has answered: `null` there would otherwise read as "not
  // supported", and a tip picker would skip paint sync for good on a slow first answer.
  const status: PaintSyncStatus | null = platform === null
    ? null
    : !supported
    ? "unavailable"
    : readiness === null
      ? null
      : enabled
        ? blocked
          ? "paused"
          : "on"
        : blocked
          ? "needsIntegration"
          : "off";

  /** What "Turn on" will do, in order, for the confirm to list. */
  const steps: TKey[] = [];
  if (readiness?.reason === "frostmodMissing") steps.push("paintSync.stepInstallIntegration");
  if (readiness?.reason === "frostmodMissing" || readiness?.reason === "frostmodDisabled")
    steps.push("paintSync.stepStartIntegration");
  if (!enabled) steps.push("paintSync.stepSync");

  /** Turn on paint sync and whatever it needs. Resolves `true` when it ended up on. */
  const turnOn = useCallback(async (): Promise<boolean> => {
    setBusy(true);
    try {
      let ready = await paintSyncReadiness().catch(() => null);
      if (ready?.ready === false) {
        // Installs, updates, sets it to start with the app and starts it. Failures there are
        // already said by its own toasts, so the check below only has to notice the outcome.
        await enableIntegration();
        ready = await paintSyncReadiness().catch(() => null);
        if (ready?.ready === false) {
          toast.error(t("paintSync.setupFailed"), {
            description: t("settings.paintSyncNeedsFrostmod"),
          });
          setReadiness(ready);
          return false;
        }
      }
      await setPaintSyncEnabled(true);
      await reloadConfig();
      toast.success(t("paintSync.turnedOn"), { description: t("paintSync.turnedOnDesc") });
      return true;
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
      await reloadConfig();
      return false;
    } finally {
      refresh();
      setBusy(false);
    }
  }, [enableIntegration, refresh, reloadConfig, t]);

  const turnOff = useCallback(async () => {
    try {
      await setPaintSyncEnabled(false);
    } catch (e) {
      toast.error(t("settings.updateFailed"), { description: String(e) });
    }
    await reloadConfig();
  }, [reloadConfig, t]);

  return { status, enabled, readiness, steps, busy, turnOn, turnOff, refresh };
}
