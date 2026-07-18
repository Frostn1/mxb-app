import { useEffect, useState } from "react";
import { Maximize2, Bike, User, Box } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "../ui/button";
import { Dialog, DialogContent } from "../ui/dialog";
import { ModelViewer, type ViewerMode } from "./ModelViewer";
import { loadRiderModel } from "../../api/mods";
import type { Loadout, RiderPart } from "../../types";

interface ViewerPanelProps {
  /** Paint texture (`data:` URI) to map onto the model, once resolved. */
  texture?: string | null;
  /** Current loadout — its rider slots drive the live rider gear preview. */
  loadout?: Loadout;
  /** Lock the panel to the rider view (the Rider studio has no bike): starts in
   * rider mode and hides the Bike/Rider toggle. */
  riderOnly?: boolean;
  className?: string;
}

/** Bike / Rider segmented toggle. */
function ModeToggle({
  mode,
  onChange,
}: {
  mode: ViewerMode;
  onChange: (m: ViewerMode) => void;
}) {
  return (
    <div className="inline-flex rounded-md border border-border bg-background/60 p-0.5">
      {(
        [
          { m: "bike" as const, icon: Bike, label: "Bike" },
          { m: "rider" as const, icon: User, label: "Rider" },
        ]
      ).map(({ m, icon: Icon, label }) => (
        <button
          key={m}
          type="button"
          onClick={() => onChange(m)}
          className={cn(
            "flex items-center gap-1.5 rounded px-2.5 py-1 text-xs font-medium transition-colors",
            mode === m
              ? "bg-primary text-primary-foreground"
              : "text-muted-foreground hover:text-foreground",
          )}
        >
          <Icon className="h-3.5 w-3.5" />
          {label}
        </button>
      ))}
    </div>
  );
}

/**
 * Live 3D preview of the current loadout — a panel that sits beside the preset
 * builder, with an expand button that opens the same viewer full-screen.
 */
export function ViewerPanel({ texture, loadout, riderOnly = false, className }: ViewerPanelProps) {
  const [mode, setMode] = useState<ViewerMode>(riderOnly ? "rider" : "bike");
  const [expanded, setExpanded] = useState(false);
  const [riderParts, setRiderParts] = useState<RiderPart[] | null>(null);

  // Re-resolve the rider gear whenever a rider-affecting slot changes (a plain
  // key so bike-only edits don't trigger a reload). Debounced — the loadout
  // updates on every keystroke/slot pick.
  const riderKey = loadout
    ? [
        loadout.rider,
        loadout.helmet,
        loadout.helmetPaint,
        loadout.boots,
        loadout.bootsPaint,
        loadout.protection,
        loadout.protectionPaint,
        loadout.suitPaint,
        loadout.glovesPaint,
      ].join("|")
    : "";

  useEffect(() => {
    if (!loadout) {
      setRiderParts(null);
      return;
    }
    let alive = true;
    const t = setTimeout(() => {
      loadRiderModel(loadout)
        .then((m) => alive && setRiderParts(m.parts))
        .catch(() => alive && setRiderParts(null));
    }, 200);
    return () => {
      alive = false;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [riderKey]);

  return (
    <>
      <div
        className={cn(
          "flex flex-col overflow-hidden rounded-lg border border-border bg-card",
          className,
        )}
      >
        <div className="flex items-center justify-between border-b border-border px-3 py-2">
          <div className="flex items-center gap-2 text-sm font-medium">
            <Box className="h-4 w-4 text-muted-foreground" />
            3D Preview
          </div>
          <div className="flex items-center gap-2">
            {!riderOnly && <ModeToggle mode={mode} onChange={setMode} />}
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7"
              title="Expand"
              onClick={() => setExpanded(true)}
            >
              <Maximize2 className="h-4 w-4" />
            </Button>
          </div>
        </div>
        <div className="relative min-h-[280px] flex-1">
          <ModelViewer
            mode={mode}
            texture={texture}
            riderParts={riderParts}
            className="absolute inset-0"
          />
        </div>
      </div>

      <Dialog open={expanded} onOpenChange={setExpanded}>
        <DialogContent className="h-[85vh] w-[92vw] max-w-none gap-0 overflow-hidden p-0 sm:max-w-none">
          <div className="flex items-center justify-between border-b border-border px-4 py-2.5">
            <div className="flex items-center gap-2 text-sm font-medium">
              <Box className="h-4 w-4 text-muted-foreground" />
              3D Preview
            </div>
            {!riderOnly && <ModeToggle mode={mode} onChange={setMode} />}
          </div>
          <div className="relative flex-1">
            <ModelViewer
            mode={mode}
            texture={texture}
            riderParts={riderParts}
            className="absolute inset-0"
          />
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
