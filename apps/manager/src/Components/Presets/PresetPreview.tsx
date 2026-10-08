import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Box, ExternalLink, Lock, X } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@frost/shared/Components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogTitle } from "@frost/shared/Components/ui/dialog";
import { ViewerPanel } from "@frost/shared/Components/Viewer/ViewerPanel";
import { useConfig } from "@frost/shared/Context/Config";
import { launchStudio, studioInstall } from "@frost/shared/api/mods";
import {
  bikeLocked,
  pickedModel,
  previewIssues,
  type Scans,
} from "@frost/shared/lib/presets";
import { useGearPaints } from "@frost/shared/lib/useGearPaints";
import type { Preset } from "@frost/shared/types";
import { useT } from "@/i18n";

interface PresetPreviewProps {
  preset: Preset;
  /** The bike the Presets page is on. A loadout names no bike, so its livery is read here. */
  bike: string;
  scans: Scans | null;
  onClose: () => void;
}

/**
 * A saved preset in 3D: the bike with its model, livery and tyres, and the rider in the
 * preset's gear, as the game would dress them. Look only — orbit and zoom, nothing to edit.
 *
 * Its own chunk, loaded when somebody opens it: the viewer pulls in three.js and the list of
 * presets has no business waiting on that.
 */
export default function PresetPreview({ preset, bike, scans, onClose }: PresetPreviewProps) {
  const t = useT();
  const { bikePreview } = useConfig();
  const { loadout } = preset;
  // Packed gear paints count as installed — the library scan can't see inside a `.pkz`.
  const { missingFor } = useGearPaints(loadout);
  const issues = useMemo(
    () => previewIssues(loadout, scans, (slot) => missingFor(slot, bike, scans)),
    [loadout, scans, missingFor, bike],
  );
  const lockedBike = bikeLocked(bike, scans);
  const showBike = bikePreview && !!bike && !lockedBike;
  // An empty model-swap slot leaves the bike as it is, so draw what's on it now.
  const variant = pickedModel(bike, loadout, scans) || "Stock";
  // Only a pack that's on disk can be drawn; a built-in name or a missing one falls back to
  // the bike's own, and the note below says which.
  const tyres =
    scans?.tyres.find((n) => n.toLowerCase() === loadout.tyres.toLowerCase()) ?? "";

  // Only offered where the Studio is actually installed — a link that can only fail isn't one.
  const [hasStudio, setHasStudio] = useState(false);
  useEffect(() => {
    let alive = true;
    studioInstall()
      .then((s) => alive && setHasStudio(!!s))
      .catch(() => alive && setHasStudio(false));
    return () => {
      alive = false;
    };
  }, []);

  const openStudio = () => {
    launchStudio("rider").catch((e) =>
      toast.error(t("presets.editInStudioFailed"), {
        description: String(e).replace(/^Error:\s*/, ""),
      }),
    );
  };

  return (
    <Dialog open onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        showClose={false}
        className="flex h-[85vh] w-[92vw] max-w-none flex-col gap-0 overflow-hidden p-0 sm:max-w-none"
      >
        <div className="flex flex-none items-center justify-between gap-3 border-b border-border px-4 py-2.5">
          <div className="flex min-w-0 items-center gap-2">
            <Box className="size-4 flex-none text-muted-foreground" />
            <DialogTitle className="truncate text-sm font-medium">
              {bike ? `${preset.name} · ${bike}` : preset.name}
            </DialogTitle>
          </div>
          <div className="flex flex-none items-center gap-2">
            <span className="hidden text-[11px] text-faint sm:inline">
              {t("presets.previewReadOnly")}
            </span>
            {hasStudio && (
              <Button variant="ghost" size="sm" className="h-7" onClick={openStudio}>
                <ExternalLink className="size-3.5" />
                {t("presets.editInStudio")}
              </Button>
            )}
            <DialogClose className="rounded-md p-1 text-muted-foreground opacity-70 transition-opacity hover:opacity-100 focus:outline-none">
              <X className="size-4" />
              <span className="sr-only">{t("common.close")}</span>
            </DialogClose>
          </div>
        </div>

        <div className="flex min-h-0 flex-1 flex-col">
          <ViewerPanel
            readOnly
            loadout={loadout}
            riderOnly={!showBike}
            bikeId={showBike ? bike : undefined}
            bikeVariant={variant}
            tyres={tyres}
            className="min-h-0 flex-1 rounded-none border-0"
          />
        </div>

        {(issues.length > 0 || lockedBike || !bike) && (
          <ul className="flex flex-none flex-wrap gap-1.5 border-t border-border px-4 py-2.5 text-[11.5px]">
            {!bike && (
              <li className="flex items-center gap-1.5 rounded-md bg-foreground/[0.05] px-2 py-1 text-muted-foreground">
                <AlertTriangle className="size-3.5 flex-none" />
                {t("presets.previewNoBike")}
              </li>
            )}
            {lockedBike && (
              <li className="flex items-center gap-1.5 rounded-md bg-foreground/[0.05] px-2 py-1 text-muted-foreground">
                <Lock className="size-3.5 flex-none" />
                {t("presets.previewBikeLocked", { bike })}
              </li>
            )}
            {issues.map(({ slot, kind }) => (
              <li
                key={slot.key}
                title={loadout[slot.key]}
                className={
                  kind === "locked"
                    ? "flex items-center gap-1.5 rounded-md bg-foreground/[0.05] px-2 py-1 text-muted-foreground"
                    : "flex items-center gap-1.5 rounded-md bg-warning/10 px-2 py-1 text-warning"
                }
              >
                {kind === "locked" ? (
                  <Lock className="size-3.5 flex-none" />
                ) : (
                  <AlertTriangle className="size-3.5 flex-none" />
                )}
                {t(kind === "locked" ? "presets.previewLocked" : "presets.previewMissing", {
                  part: t(slot.label),
                })}
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
