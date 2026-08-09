import { useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { Loader2, PackageOpen } from "lucide-react";
import { useDropReview } from "../../Context/DropReview";
import { useT } from "../../i18n/context";
import { useImport } from "./useImport";

/**
 * Whole-window drop target.
 *
 * Uses Tauri's `onDragDropEvent` rather than the HTML5 drag events: with the default
 * `dragDropEnabled`, the OS handler consumes the drop and the webview never sees one — and
 * even if it did, an HTML5 `File` carries no filesystem path, which is the only thing the
 * installer can work with.
 *
 * That event is also the one part of this we don't control — where it never arrives, dropping
 * silently does nothing. So the paths it yields go through `useImport`, shared with the
 * Library's Import button, rather than a handler only a drop can reach.
 *
 * Nothing is written when a drop lands. `planDrop` stages and classifies, and the review sheet
 * — owned by `DropReviewProvider`, since purchases stage plans too — takes it from there.
 */
export default function DropZone() {
  const t = useT();
  const { reviewing } = useDropReview();
  const { stagePaths, staging } = useImport();
  const [hovering, setHovering] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") {
          setHovering(true);
        } else if (event.payload.type === "drop") {
          setHovering(false);
          void stagePaths(event.payload.paths);
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
  }, [stagePaths]);

  // `staging` is this hook instance's own — a pick from the Library drives a separate one, and
  // reports itself on its own button — so the overlay still only ever follows a drop.
  if (!(hovering || staging) || reviewing) return null;

  return (
    <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center bg-background/70 backdrop-blur-sm">
      <div className="flex flex-col items-center gap-3 rounded-2xl border-2 border-dashed border-primary/50 bg-card/80 px-12 py-10">
        {staging ? (
          <Loader2 className="size-8 animate-spin text-primary" />
        ) : (
          <PackageOpen className="size-8 text-primary" />
        )}
        <div className="text-center">
          <p className="text-[15px] font-bold">
            {staging ? t("drop.scanning") : t("drop.dropHere")}
          </p>
          {!staging && (
            <p className="mt-0.5 text-[12px] text-muted-foreground">
              {t("drop.dropHint")}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
