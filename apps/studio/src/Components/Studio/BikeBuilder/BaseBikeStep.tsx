// New i18n keys:
// "bike.baseBikeStep": "Step 1: Base bike"
// "bike.importFullBikeFile": "Import a full-bike file…"

import { useEffect, useState } from "react";
import { AlertTriangle, FolderOpen, Grid2x2, Loader2, Lock, Scissors } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Popover, PopoverTrigger, PopoverContent } from "@frost/shared/Components/ui/popover";
import { scanLibrary } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import { useT } from "@/i18n";
import { bikeTemplateReadable, PART_EXTENSIONS } from "../../../api/bikebuild";

/**
 * The base bike: one persistent strip above the working area, so choosing what the build
 * starts from doesn't depend on finding a scattered "Choose a bike…" button. Installed
 * bikes and full-bike files live here; the placeholder is here too, without top billing.
 */
export default function BaseBikeStep({
  baseName,
  baseChosen,
  problem,
  onPick,
  onImportFile,
  onPlaceholder,
  busy,
  blenderReady,
  onSplitBase,
  splitBusy,
}: {
  baseName: string | null;
  /** Whether the rider has actually picked a base yet — distinct from `baseName` being
   *  non-null, since the backend always has *some* template loaded (a fresh build defaults
   *  to the placeholder on its own). Drives the empty-state hint. */
  baseChosen: boolean;
  /** Why the chosen template couldn't be read, when the placeholder had to stand in for it. */
  problem: string | null;
  onPick: (path: string) => void;
  onImportFile: (path: string) => void;
  onPlaceholder: () => void;
  /** One of this step's own picks is in flight. Disables everything here, since two at once
   *  could race on the same template/library state. */
  busy: boolean;
  /** Importing a file or adding the placeholder both run through Blender; picking an installed
   *  bike doesn't (it's a plain header check, `bikeTemplateReadable`), so only those two wait
   *  on it — the picker itself works before Blender's even found. */
  blenderReady: boolean;
  /** Split the current base bike part into its parts, right here — not just in the tray, and
   *  not only when the auto-detected `multiPartHint` happens to fire (a real-world file's
   *  object names don't always trip that heuristic). Undefined when there's no base part to
   *  split (nothing chosen yet, or the placeholder, which isn't one file to cut up). */
  onSplitBase: (() => void) | undefined;
  /** The split this button starts is a library change, not one of this step's own picks — so
   *  it needs the library's own busy state too, or a second click before the first split has
   *  replaced the part queues a duplicate split of a part that's already gone. */
  splitBusy: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [bikes, setBikes] = useState<LibraryEntry[] | null>(null);
  const [readable, setReadable] = useState<Record<string, boolean>>({});
  const [checking, setChecking] = useState(false);

  useEffect(() => {
    if (!open || bikes !== null) return;
    scanLibrary("mods/bikes")
      .then(async (entries) => {
        const list = entries.filter((e) => e.category === "bike");
        setBikes(list);
        const toCheck = list.filter((e) => e.kind === "pkz" && !e.locked).map((e) => e.path);
        if (!toCheck.length) return;
        setChecking(true);
        try {
          setReadable(await bikeTemplateReadable(toCheck));
        } catch {
          // left empty: every one of `toCheck` stays blocked, same as a path the call never got to
        } finally {
          setChecking(false);
        }
      })
      .catch(() => setBikes([]));
  }, [open, bikes]);

  async function importFile() {
    const path = await openDialog({
      multiple: false,
      filters: [{ name: t("bike.partFiles"), extensions: PART_EXTENSIONS }],
    });
    if (typeof path === "string") onImportFile(path);
  }

  return (
    <section className="flex flex-col gap-1.5 border-b border-border bg-window px-4 py-2.5">
      <div className="flex flex-wrap items-center gap-3">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">
          {t("bike.baseBikeStep")}
        </h2>
        {baseName !== null && (
          <div className="flex min-w-0 flex-col">
            <span className="text-[11px] text-muted-foreground">{t("bike.template")}</span>
            <span className="truncate text-sm font-medium" title={baseName}>
              {baseName}
            </span>
          </div>
        )}
        {onSplitBase && (
          <Button type="button" size="sm" variant="outline" onClick={onSplitBase} disabled={busy || splitBusy}>
            <Scissors className="size-3.5" />
            {t("bike.splitIntoParts")}
          </Button>
        )}
        <Popover open={open} onOpenChange={setOpen}>
          <PopoverTrigger asChild>
            <Button type="button" size="sm" variant="outline" disabled={busy}>
              <Grid2x2 className="size-3.5" />
              {t("bike.startFromBike")}
            </Button>
          </PopoverTrigger>
          <PopoverContent align="start" className="w-96 max-w-[calc(100vw-2rem)] p-2">
            {bikes === null ? (
              <div className="flex items-center gap-2 p-2 text-sm text-muted-foreground">
                <Loader2 className="size-3.5 animate-spin" />
                {t("bike.looking")}
              </div>
            ) : bikes.length === 0 ? (
              <p className="p-2 text-sm text-muted-foreground">{t("bike.startFromBikeEmpty")}</p>
            ) : (
              <div className="grid max-h-72 grid-cols-2 gap-2 overflow-y-auto">
                {bikes.map((b) => {
                  const isPkz = b.kind === "pkz" && !b.locked;
                  const stillChecking = isPkz && checking && readable[b.path] === undefined;
                  const protectedPkz = isPkz && !stillChecking && readable[b.path] !== true;
                  const blocked = !!b.locked || protectedPkz || stillChecking;
                  const reason = b.locked
                    ? "bike.bikeLocked"
                    : stillChecking
                      ? "bike.looking"
                      : "bike.bikeProtected";
                  return (
                    <Button
                      key={`${b.path}#${b.prefix ?? ""}`}
                      type="button"
                      size="sm"
                      variant="outline"
                      className="h-auto min-h-14 min-w-0 justify-start border-border p-3 text-left disabled:pointer-events-auto disabled:cursor-not-allowed disabled:opacity-50"
                      disabled={busy || blocked}
                      title={blocked ? t(reason) : b.name}
                      onClick={() => {
                        onPick(b.path);
                        setOpen(false);
                      }}
                    >
                      <span className="min-w-0 flex-1 truncate">{b.name}</span>
                      {blocked && <Lock className="size-3 shrink-0 text-faint" />}
                    </Button>
                  );
                })}
              </div>
            )}
            <p className="px-2 pt-2 text-[11px] text-faint">{t("bike.startFromBikeScopeNote")}</p>
          </PopoverContent>
        </Popover>
        <Button
          type="button"
          size="sm"
          variant="outline"
          onClick={importFile}
          disabled={busy || !blenderReady}
          title={blenderReady ? undefined : t("bike.looking")}
        >
          <FolderOpen className="size-3.5" />
          {t("bike.importFullBikeFile")}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          className="text-[12px] text-muted-foreground underline-offset-4 hover:underline"
          title={blenderReady ? t("bike.placeholderHint") : t("bike.looking")}
          onClick={onPlaceholder}
          disabled={busy || !blenderReady}
        >
          {t("bike.addPlaceholder")}
        </Button>
      </div>
      {problem && (
        <p className="flex items-center gap-1 text-[12px] text-amber-500">
          <AlertTriangle className="size-3.5" />
          {t("bike.templateProblem", { problem })}
        </p>
      )}
      {!baseChosen && <p className="text-[12px] text-muted-foreground">{t("bike.baseBikeHint")}</p>}
    </section>
  );
}
