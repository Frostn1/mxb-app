import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Loader2, PackageOpen } from "lucide-react";
import { toast } from "sonner";
import { planDrop } from "../../api/mods";
import { useDropReview } from "../../Context/DropReview";
import { useT } from "../../i18n/context";

/**
 * Whole-window drop target.
 *
 * Uses Tauri's `onDragDropEvent` rather than the HTML5 drag events: with the default
 * `dragDropEnabled`, the OS handler consumes the drop and the webview never sees one — and
 * even if it did, an HTML5 `File` carries no filesystem path, which is the only thing the
 * installer can work with.
 *
 * Nothing is written when a drop lands. `planDrop` stages and classifies, and the review sheet
 * — owned by `DropReviewProvider`, since purchases stage plans too — takes it from there.
 */
export default function DropZone() {
  const t = useT();
  const { reviewPlan, reviewing } = useDropReview();
  const [hovering, setHovering] = useState(false);
  const [scanning, setScanning] = useState(false);

  const tRef = useRef(t);
  tRef.current = t;
  const reviewRef = useRef(reviewPlan);
  reviewRef.current = reviewPlan;

  const handlePaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    setScanning(true);
    try {
      reviewRef.current(await planDrop(paths));
    } catch (e) {
      toast.error(tRef.current("drop.scanFailed"), { description: String(e) });
    } finally {
      setScanning(false);
    }
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setHovering(true);
        } else if (event.payload.type === "drop") {
          setHovering(false);
          void handlePaths(event.payload.paths);
        } else {
          setHovering(false);
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [handlePaths]);

  if (!(hovering || scanning) || reviewing) return null;

  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div className="flex flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-primary/50 bg-card/80 px-12 py-10">
        {scanning ? (
          <Loader2 className="size-8 animate-spin text-primary" />
        ) : (
          <PackageOpen className="size-8 text-primary" />
        )}
        <div className="text-center">
          <p className="text-[15px] font-bold">
            {scanning ? t("drop.scanning") : t("drop.dropHere")}
          </p>
          {!scanning && (
            <p className="mt-0.5 text-[12px] text-muted-foreground">
              {t("drop.dropHint")}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
