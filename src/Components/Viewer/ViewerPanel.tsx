import { useEffect, useRef, useState } from "react";
import { Maximize2, Bike, User, Box, Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "../ui/button";
import { Dialog, DialogContent } from "../ui/dialog";
import { ModelViewer, type ViewerMode } from "./ModelViewer";
import { loadRiderModel } from "../../api/mods";
import type { Loadout, RiderPart } from "../../types";

interface ViewerPanelProps {
  texture?: string | null;
  loadout?: Loadout;
  riderOnly?: boolean;
  hiddenParts?: RiderPart["part"][];
  className?: string;
}

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

export function ViewerPanel({
  texture,
  loadout,
  riderOnly = false,
  hiddenParts,
  className,
}: ViewerPanelProps) {
  const [mode, setMode] = useState<ViewerMode>(riderOnly ? "rider" : "bike");
  const [expanded, setExpanded] = useState(false);
  const [riderParts, setRiderParts] = useState<RiderPart[] | null>(null);
  const [loading, setLoading] = useState(false);
  // First resolve loads immediately; later slot edits are debounced so picks don't thrash the decoder.
  const firstLoad = useRef(true);

  // Drop any toggled-off gear before rendering (keep the body + everything else).
  const shownParts = hiddenParts?.length
    ? riderParts?.filter((p) => !hiddenParts.includes(p.part)) ?? null
    : riderParts;

  // Re-resolve rider gear when a rider-affecting slot changes (debounced; loadout updates per keystroke).
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
      setLoading(false);
      return;
    }
    let alive = true;
    setLoading(true);
    const delay = firstLoad.current ? 0 : 200;
    firstLoad.current = false;
    const t = setTimeout(() => {
      loadRiderModel(loadout)
        // Keep the previous model on screen until the new one is ready (and on failure) so it never blanks.
        .then((m) => alive && setRiderParts(m.parts))
        .catch(() => {})
        .finally(() => alive && setLoading(false));
    }, delay);
    return () => {
      alive = false;
      clearTimeout(t);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [riderKey]);

  // Non-blocking spinner over the canvas while a model resolves; the current model stays visible.
  const spinner = loading && (
    <div className="pointer-events-none absolute right-3 top-3 flex items-center gap-1.5 rounded-md bg-black/55 px-2 py-1 text-[11px] text-white/85">
      <Loader2 className="h-3.5 w-3.5 animate-spin" />
      Loading…
    </div>
  );

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
            riderParts={shownParts}
            className="absolute inset-0"
          />
          {spinner}
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
            riderParts={shownParts}
            className="absolute inset-0"
          />
            {spinner}
          </div>
        </DialogContent>
      </Dialog>
    </>
  );
}
