import { CloudOff, FolderOutput, HardDriveDownload, PowerOff } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import {
  healthCheck,
  keepPibosoOnDevice,
  moveEmptyTyreFolders,
  onModsDehydrated,
  onPinProgress,
  setReshadeEnabled,
} from "@frost/shared/api/mods";
import type { HealthReport } from "@frost/shared/types";
import { useGameRunning } from "@/lib/useGameRunning";
import { useT } from "@/i18n";
import { onedriveNotice, onlineOnlyTotal, tyreNotices } from "@/lib/healthNotice";
import { Bar } from "./RuntimeBanner";

/**
 * The two setups behind "crashes joining busy servers, fine alone": the PiBoSo folder inside
 * OneDrive, and ReShade hooked into the game. Joining loads every other rider's bikes, paints
 * and textures at once on the game's main thread, and both make that load fragile.
 *
 * Each is a slim bar like the runtime ones above it, with a one-click fix that can be undone
 * — "Keep on this device" for OneDrive, "Turn off ReShade" for ReShade — and never anything
 * done without a press.
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
    onedrive?: boolean;
    reshade?: boolean;
    tyreEmpty?: boolean;
    tyreOverride?: boolean;
  }>({});
  const [movingTyres, setMovingTyres] = useState(false);
  const [pinning, setPinning] = useState<{ done: number; total: number } | null>(null);
  const [togglingReshade, setTogglingReshade] = useState(false);
  const [moveOpen, setMoveOpen] = useState(false);

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

  const keepOnDevice = async () => {
    setPinning({ done: 0, total: 0 });
    const stop = onPinProgress((p) => setPinning(p));
    try {
      const r = await keepPibosoOnDevice();
      if (!r.supported) {
        toast.error(t("health.keepFailed"), { description: t("health.keepUnsupported") });
      } else if (r.failed > 0) {
        toast.warning(t("health.keptPartial"), {
          description: t("health.keptPartialDesc", {
            failed: String(r.failed),
            total: String(r.total),
            error: r.firstError ?? "",
          }),
        });
      } else if (r.stillOnlineOnly > 0) {
        toast.success(t("health.keptTitle"), {
          description: t("health.keptDownloading", {
            pinned: String(r.pinned),
            count: String(r.stillOnlineOnly),
          }),
          duration: 12000,
        });
      } else {
        toast.success(t("health.keptTitle"), {
          description: t("health.keptDesc", { count: String(r.pinned) }),
        });
      }
    } catch (e) {
      toast.error(t("health.keepFailed"), { description: String(e) });
    } finally {
      void stop.then((off) => off());
      setPinning(null);
      await refresh();
    }
  };

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

  const notice = onedriveNotice(report);
  const online = onlineOnlyTotal(report);
  const showOnedrive = notice && !dismissed.onedrive;
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
      {showOnedrive && (
        <Bar
          tone={notice.kind === "online" ? "danger" : "warning"}
          icon={CloudOff}
          wrap
          body={
            notice.kind === "online"
              ? t("health.onedriveOnline", { count: online })
              : notice.kind === "piboso"
                ? t("health.onedrivePiboso")
                : t("health.onedriveGame")
          }
          pitch={t("health.onedrivePitch")}
          action={
            notice.canKeep ? (running ? t("health.closeGameFirst") : t("health.keep")) : undefined
          }
          actionIcon={HardDriveDownload}
          actionDisabled={running}
          busy={pinning !== null}
          busyLabel={
            pinning && pinning.total > 0
              ? t("health.keeping", { done: String(pinning.done), total: String(pinning.total) })
              : t("health.keepingStart")
          }
          onAction={() => void keepOnDevice()}
          secondary={t("health.howToMove")}
          onSecondary={() => setMoveOpen(true)}
          onDismiss={() => setDismissed((d) => ({ ...d, onedrive: true }))}
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
      <Dialog open={moveOpen} onOpenChange={setMoveOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t("health.moveTitle")}</DialogTitle>
            <DialogDescription>{t("health.moveIntro")}</DialogDescription>
          </DialogHeader>
          <ol className="flex list-decimal flex-col gap-1.5 pl-5 text-[12.5px] leading-relaxed text-foreground/85">
            <li>{t("health.moveStep1")}</li>
            <li>{t("health.moveStep2")}</li>
            <li>{t("health.moveStep3")}</li>
            <li>{t("health.moveStep4")}</li>
          </ol>
          {report.onedrive.pibosoDir && (
            <p className="text-[11.5px] leading-relaxed text-muted-foreground">
              {t("health.moveCurrent")}{" "}
              <span className="break-all font-mono">{report.onedrive.pibosoDir}</span>
            </p>
          )}
          <p className="text-[11.5px] leading-relaxed text-muted-foreground">
            {t("health.moveNote")}
          </p>
          <DialogFooter>
            <Button size="sm" variant="outline" onClick={() => setMoveOpen(false)}>
              {t("common.close")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
