import { FolderOutput, PowerOff } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import {
  healthCheck,
  moveEmptyTyreFolders,
  onModsDehydrated,
  setReshadeEnabled,
} from "@frost/shared/api/mods";
import type { HealthReport } from "@frost/shared/types";
import { useGameRunning } from "@/lib/useGameRunning";
import { useT } from "@/i18n";
import { tyreNotices } from "@/lib/healthNotice";
import { Bar } from "./RuntimeBanner";

/**
 * ReShade hooked into the game is behind "crashes joining busy servers, fine alone": joining
 * loads every other rider's bikes, paints and textures at once on the game's main thread, and
 * ReShade makes that load fragile. It gets a slim bar like the runtime ones above it, with a
 * one-click "Turn off ReShade" that can be undone, and never anything done without a press.
 *
 * Plus the tyre check: an empty folder in `mods/tyres` crashes the bike list, so it gets a
 * loud bar with "Move out" (moved beside `mods`, never deleted). A folder or `.pkz` that
 * replaces a stock tyre gets a quiet one. Renders nothing when none applies.
 */
export default function HealthBanner() {
  const t = useT();
  const { running } = useGameRunning();
  const [report, setReport] = useState<HealthReport | null>(null);
  const [dismissed, setDismissed] = useState<{
    reshade?: boolean;
    tyreEmpty?: boolean;
    tyreOverride?: boolean;
  }>({});
  const [movingTyres, setMovingTyres] = useState(false);
  const [togglingReshade, setTogglingReshade] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setReport(await healthCheck());
    } catch {
      setReport(null);
    }
  }, []);

  useEffect(() => {
    void refresh();
    // A session start re-checks the mods tree; take the chance to re-read ours too.
    const stop = onModsDehydrated(() => void refresh());
    return () => void stop.then((off) => off());
  }, [refresh]);

  const toggleReshade = async (enable: boolean) => {
    setTogglingReshade(true);
    try {
      const next = await setReshadeEnabled(enable);
      setReport((r) => (r ? { ...r, reshade: next } : r));
      if (enable) {
        toast.success(t("health.reshadeOnDone"));
      } else {
        toast.success(t("health.reshadeOffDone"), {
          description: t("health.reshadeOffDesc"),
          action: { label: t("health.reshadeOn"), onClick: () => void toggleReshade(true) },
          duration: 12000,
        });
      }
    } catch (e) {
      toast.error(t("health.reshadeFailed"), { description: String(e) });
    } finally {
      setTogglingReshade(false);
    }
  };

  const moveTyres = async () => {
    setMovingTyres(true);
    try {
      await moveEmptyTyreFolders();
      toast.success(t("health.tyreMovedDone"));
    } catch (e) {
      toast.error(t("health.tyreMoveFailed"), { description: String(e) });
    } finally {
      setMovingTyres(false);
      await refresh();
    }
  };

  if (!report) return null;

  const showReshade = report.reshade.active && !dismissed.reshade;
  const tyres = tyreNotices(report);

  return (
    <>
      {tyres.empty && !dismissed.tyreEmpty && (
        <Bar
          tone="danger"
          icon={FolderOutput}
          wrap
          body={t("health.tyreEmpty", { names: tyres.empty.names, count: tyres.empty.count })}
          pitch={t("health.tyreEmptyPitch")}
          action={running ? t("health.closeGameFirst") : t("health.tyreMove")}
          actionIcon={FolderOutput}
          actionDisabled={running}
          busy={movingTyres}
          busyLabel={t("health.tyreMoving")}
          onAction={() => void moveTyres()}
          onDismiss={() => setDismissed((d) => ({ ...d, tyreEmpty: true }))}
          dismissLabel={t("runtime.dismiss")}
        />
      )}
      {tyres.overrides && !dismissed.tyreOverride && (
        <Bar
          tone="warning"
          wrap
          body={t("health.tyreOverride", {
            names: tyres.overrides.names,
            count: tyres.overrides.count,
          })}
          pitch={t("health.tyreOverridePitch")}
          onDismiss={() => setDismissed((d) => ({ ...d, tyreOverride: true }))}
          dismissLabel={t("runtime.dismiss")}
        />
      )}
      {showReshade && (
        <Bar
          tone="warning"
          icon={PowerOff}
          wrap
          body={
            report.reshade.version
              ? t("health.reshadeBodyVersion", { version: report.reshade.version })
              : t("health.reshadeBody")
          }
          pitch={t("health.reshadePitch")}
          action={running ? t("health.closeGameFirst") : t("health.reshadeOff")}
          actionIcon={PowerOff}
          actionDisabled={running}
          busy={togglingReshade}
          busyLabel={t("health.reshadeBusy")}
          onAction={() => void toggleReshade(false)}
          onDismiss={() => setDismissed((d) => ({ ...d, reshade: true }))}
          dismissLabel={t("runtime.dismiss")}
        />
      )}
    </>
  );
}
