import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { FileDown, FolderOpen, Loader2 } from "lucide-react";

import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@frost/shared/Components/ui/dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import { cn } from "@frost/shared/lib/utils";
import { listStockTracks } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import {
  inspectTrackImport,
  type ScanJumps,
  type TrackImportPreview,
  type TrackProgram,
  importTrack,
} from "@/api/trackgen";

/** The track being imported: a file on disk, or one folder inside a shared archive. */
interface Source {
  path: string;
  prefix: string | null;
  /** What to call it before the track's own `.ini` has been read. */
  label: string;
}

/**
 * Start a track from one somebody already compiled.
 *
 * Two ways in, because there are two things a rider means by "the track I want to build on":
 * one of the game's own — which lives inside the install's shared `tracks.pkz` and can't be
 * browsed to as a file — or any `.pkz` on disk, their own earlier build included.
 *
 * The dialog reads the track before committing to anything, so what it offers is what is
 * actually in there: the plot, the relief, and whether there is a centreline to rebuild
 * around at all. A track finished without `tracked -merge` carries no lap and cannot be
 * imported, and that is much better said here than after a twenty-minute compile.
 */
export default function ImportTrack({
  open,
  onOpenChange,
  onImport,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  /** Hands back a loader, so the caller keeps its own "replace what's open?" guard. */
  onImport: (load: () => Promise<TrackProgram>, name: string) => void;
}) {
  const [stock, setStock] = useState<LibraryEntry[]>([]);
  const [source, setSource] = useState<Source | null>(null);
  const [preview, setPreview] = useState<TrackImportPreview | null>(null);
  const [reading, setReading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [jumps, setJumps] = useState<ScanJumps>("keep");

  useEffect(() => {
    if (!open) return;
    void listStockTracks()
      .then(setStock)
      .catch(() => {});
  }, [open]);

  // A fresh open starts over rather than showing the last track someone looked at.
  useEffect(() => {
    if (open) return;
    setSource(null);
    setPreview(null);
    setError(null);
    setJumps("keep");
  }, [open]);

  useEffect(() => {
    if (!source) {
      setPreview(null);
      setError(null);
      return;
    }
    let alive = true;
    setReading(true);
    setPreview(null);
    setError(null);
    void inspectTrackImport(source.path, source.prefix)
      .then((p) => alive && setPreview(p))
      .catch((e) => alive && setError(e instanceof Error ? e.message : String(e)))
      .finally(() => alive && setReading(false));
    return () => {
      alive = false;
    };
  }, [source]);

  async function browse() {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: "MX Bikes track", extensions: ["pkz"] }],
    });
    if (typeof path !== "string") return;
    setSource({
      path,
      prefix: null,
      label: path.split(/[\\/]/).pop()?.replace(/\.pkz$/i, "") ?? path,
    });
  }

  const ready = !!source && !!preview && preview.hasLap;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>Start from an existing track</DialogTitle>
          <DialogDescription>
            Its layout, elevation and footprint come across. Its ground textures, scenery and
            objects do not — a compiled track has no source in it to read those back from — so
            what you build will wear the Studio&rsquo;s own. Nothing here changes the track you
            import from.
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-4">
          <div className="flex flex-col gap-2">
            <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
              Came with the game
            </span>
            {stock.length === 0 ? (
              <p className="text-[12px] text-muted-foreground">
                No game install found, so there are no stock tracks to list. Browse for a
                <code className="px-1">.pkz</code> instead.
              </p>
            ) : (
              <div className="grid max-h-44 grid-cols-3 gap-1.5 overflow-auto pr-1">
                {stock.map((s) => {
                  const picked = source?.prefix === s.prefix;
                  return (
                    <button
                      key={s.prefix ?? s.name}
                      onClick={() =>
                        setSource({
                          path: s.path,
                          prefix: s.prefix ?? null,
                          label: s.name,
                        })
                      }
                      className={cn(
                        "flex flex-col items-start gap-0.5 rounded-lg border px-2.5 py-1.5 text-left transition-colors",
                        picked
                          ? "border-primary/60 bg-primary/[0.06]"
                          : "border-white/[0.07] hover:border-white/15",
                      )}
                    >
                      <span className="w-full truncate text-[12.5px] font-semibold">
                        {s.name}
                      </span>
                      <span className="text-[11px] text-faint">{s.folder}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>

          <div className="flex items-center gap-2">
            <Button variant="outline" size="sm" onClick={() => void browse()}>
              <FolderOpen className="size-3.5" /> Browse for a .pkz…
            </Button>
            {source?.prefix === null && (
              <span className="truncate text-[12px] text-muted-foreground">{source.label}</span>
            )}
          </div>

          {/* What is actually in the file, before anyone commits to a compile. */}
          {reading && (
            <p className="flex items-center gap-2 text-[12px] text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" /> Reading {source?.label}…
            </p>
          )}
          {error && <p className="text-[12px] text-destructive">{error}</p>}
          {preview && (
            <div className="flex flex-col gap-1.5 rounded-lg border border-white/[0.07] bg-card/40 p-3">
              <span className="text-[13px] font-semibold">{preview.name}</span>
              <span className="text-[11.5px] text-muted-foreground">
                {preview.sizeX.toFixed(0)} × {preview.sizeZ.toFixed(0)} m ·{" "}
                {preview.samplesX}×{preview.samplesZ} samples · {preview.reliefM.toFixed(1)} m
                of relief
                {preview.surfaces.length > 0 && ` · ${preview.surfaces.join(", ")}`}
              </span>
              {preview.hasLap ? (
                <span className="text-[11.5px] text-muted-foreground">
                  {preview.segments} segments of centreline to rebuild around.
                </span>
              ) : (
                <span className="text-[11.5px] text-destructive">
                  This track&rsquo;s terrain file carries no centreline, so there is no lap to
                  rebuild it around. Not every track was finished with the tool that writes
                  one, and there is no way to recover it.
                </span>
              )}
            </div>
          )}

          {ready && (
            <div className="flex flex-col gap-2">
              <span className="text-[11px] font-bold uppercase tracking-[1.2px] text-faint">
                What the ground arrives as
              </span>
              <Segmented
                size="sm"
                value={jumps}
                onChange={setJumps}
                options={[
                  { value: "keep", label: "As it is" },
                  { value: "rut", label: "Ridden in" },
                  { value: "recut", label: "Recut" },
                ]}
              />
              <p className="text-[11.5px] text-muted-foreground">
                {jumps === "keep" &&
                  "The terrain exactly as its builder left it — every jump, camber and rut the source had, and not one invented. A faithful base to change by hand."}
                {jumps === "rut" &&
                  "The same terrain, with ruts and grooves laid over the riding line. For a track that was compiled clean and rides flat."}
                {jumps === "recut" &&
                  "Keep only the landform and cut a fresh corridor into it. The layout survives; the jumps that were built on it are replaced by generated ones."}
              </p>
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button
            disabled={!ready}
            onClick={() => {
              if (!source || !ready) return;
              const { path, prefix } = source;
              onImport(() => importTrack(path, prefix, jumps), preview?.name ?? source.label);
              onOpenChange(false);
            }}
          >
            <FileDown className="size-3.5" /> Import
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
