import { useEffect, useState } from "react";
import { AlertTriangle, ChevronDown, FolderOpen, Grid2x2, Loader2, Lock, Scissors } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Popover, PopoverTrigger, PopoverContent } from "@frost/shared/Components/ui/popover";
import { scanLibrary } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import { useT } from "@/i18n";
import { bikeTemplateReadable, PART_EXTENSIONS } from "../../../api/bikebuild";

/**
 * The base bike: one primary control, not a row of three — "start from an installed bike",
 * "import a full-bike file" and "add the placeholder" all used to be their own button,
 * competing for the same first look. Now there's one thing to click (it shows the current
 * base's name once something's picked, or a plain "Pick a base bike" before that), and its
 * popover holds the installed-bike grid with the other two choices underneath it, in reach
 * but not fighting for attention. Sits inline in the shared header row, not a strip of its
 * own — see `StepHeader` for why.
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
   *  to the placeholder on its own). Drives the button's own label and the mount dots. */
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
    setOpen(false);
    const path = await openDialog({
      multiple: false,
      filters: [{ name: t("bike.partFiles"), extensions: PART_EXTENSIONS }],
    });
    if (typeof path === "string") onImportFile(path);
  }

  return (
    <div className="flex min-w-0 flex-wrap items-center gap-2">
      {onSplitBase && (
        <Button
          type="button"
          size="sm"
          variant="ghost"
          onClick={onSplitBase}
          disabled={busy || splitBusy}
          title={t("bike.splitIntoParts")}
        >
          <Scissors className="size-3.5" />
          {t("bike.splitIntoParts")}
        </Button>
      )}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <Button type="button" size="sm" variant="outline" disabled={busy} className="max-w-[16rem]">
            <Grid2x2 className="size-3.5 shrink-0" />
            <span className="min-w-0 truncate">{baseChosen ? baseName : t("bike.pickBaseBike")}</span>
            <ChevronDown className="size-3.5 shrink-0 opacity-60" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="end" className="w-96 max-w-[calc(100vw-2rem)] p-2">
          <p className="px-1 pb-1.5 text-[11px] font-semibold uppercase tracking-[0.06em] text-faint">
            {t("bike.startFromBike")}
          </p>
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
          <div className="my-1.5 h-px bg-border" />
          <button
            type="button"
            onClick={() => void importFile()}
            disabled={busy || !blenderReady}
            title={blenderReady ? undefined : t("bike.looking")}
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] hover:bg-accent disabled:pointer-events-none disabled:opacity-50"
          >
            <FolderOpen className="size-3.5" />
            {t("bike.importFullBikeFile")}
          </button>
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              onPlaceholder();
            }}
            disabled={busy || !blenderReady}
            title={blenderReady ? t("bike.placeholderHint") : t("bike.looking")}
            className="flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] hover:bg-accent disabled:pointer-events-none disabled:opacity-50"
          >
            {t("bike.addPlaceholder")}
          </button>
        </PopoverContent>
      </Popover>
      {problem && (
        <p className="flex w-full items-center gap-1 text-[12px] text-amber-500">
          <AlertTriangle className="size-3.5" />
          {t("bike.templateProblem", { problem })}
        </p>
      )}
    </div>
  );
}
