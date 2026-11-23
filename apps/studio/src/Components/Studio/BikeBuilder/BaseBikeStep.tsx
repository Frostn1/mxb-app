import { useEffect, useState } from "react";
import { AlertTriangle, Bike, ChevronDown, FolderOpen, Grid2x2, Loader2, Lock } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { Popover, PopoverTrigger, PopoverContent } from "@frost/shared/Components/ui/popover";
import { scanLibrary } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import { useT } from "@/i18n";
import { bikeTemplateReadable, PART_EXTENSIONS } from "../../../api/bikebuild";
import BlenderNeeded, { type useBlenderGate } from "./BlenderNeeded";

/** A pick from the popover that would replace parts on the bike, held until it's confirmed. */
type Pending = "import" | "placeholder";

/**
 * The base bike: one control, showing the current base's name once something's picked (or a
 * plain "Pick a base bike" before that). Its popover is step 1 of the tab — the stage line's
 * "Base bike" opens it too, to go back and change it.
 *
 * Changing the base keeps the parts on the bike, with two exceptions, which ask first in one
 * line rather than a dialog: importing a full-bike file takes the chassis slot (replacing the
 * part in it), and the placeholder fills every slot with its own parts.
 */
export default function BaseBikeStep({
  open,
  onOpenChange,
  baseName,
  baseChosen,
  problem,
  onPick,
  onImportFile,
  onPlaceholder,
  busy,
  gate,
  slotsFilled,
  chassisFilled,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  baseName: string | null;
  /** Whether the rider has actually picked a base yet — distinct from `baseName` being
   *  non-null, since the backend always has *some* template loaded (a fresh build defaults
   *  to the placeholder on its own). */
  baseChosen: boolean;
  /** Why the chosen template couldn't be read, when the placeholder had to stand in for it. */
  problem: string | null;
  onPick: (path: string) => void;
  onImportFile: (path: string) => void;
  onPlaceholder: () => void;
  /** One of this step's own picks, or a library change, is in flight. */
  busy: boolean;
  /** Importing a file or adding the placeholder both run through Blender; picking an
   *  installed bike doesn't (a plain header check), so the list works before it's found. */
  gate: ReturnType<typeof useBlenderGate>;
  /** How many slots hold a part now, and whether the chassis is one — what the two
   *  replacing picks would throw away. */
  slotsFilled: number;
  chassisFilled: boolean;
}) {
  const t = useT();
  const [bikes, setBikes] = useState<LibraryEntry[] | null>(null);
  const [readable, setReadable] = useState<Record<string, boolean>>({});
  const [checking, setChecking] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);

  useEffect(() => {
    if (!open) setPending(null);
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
    onOpenChange(false);
    const path = await openDialog({
      multiple: false,
      filters: [{ name: t("bike.partFiles"), extensions: PART_EXTENSIONS }],
    });
    if (typeof path === "string") onImportFile(path);
  }

  function run(what: Pending) {
    if (!gate.need("base")) return;
    const replaces = what === "import" ? chassisFilled : slotsFilled > 0;
    if (replaces && pending !== what) {
      setPending(what);
      return;
    }
    setPending(null);
    if (what === "import") void importFile();
    else {
      onOpenChange(false);
      onPlaceholder();
    }
  }

  const choice =
    "flex w-full items-center gap-2 rounded-sm px-2 py-1.5 text-left text-[13px] hover:bg-accent disabled:pointer-events-none disabled:opacity-50";

  return (
    <div className="flex min-w-0 items-center gap-2">
      {problem && (
        <span title={t("bike.templateProblem", { problem })} className="text-amber-500">
          <AlertTriangle className="size-3.5" />
        </span>
      )}
      <Popover open={open} onOpenChange={onOpenChange}>
        <PopoverTrigger asChild>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={busy}
            className="h-7 max-w-[15rem] px-2.5"
            title={baseChosen && baseName ? `${baseName}
${t("bike.pickBaseTip")}` : t("bike.pickBaseTip")}
          >
            <Grid2x2 className="size-3.5 shrink-0" />
            <span className="min-w-0 truncate">{baseChosen ? baseName : t("bike.pickBaseBike")}</span>
            <ChevronDown className="size-3.5 shrink-0 opacity-60" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="end" className="flex w-[22rem] max-w-[calc(100vw-2rem)] flex-col p-2">
          <p className="px-1 pb-2 text-[11px] font-semibold uppercase tracking-[0.06em] text-faint">
            {t("bike.startFromBike")}
          </p>
          {bikes === null ? (
            <div className="flex items-center gap-2 p-2 text-sm text-muted-foreground">
              <Loader2 className="size-3.5 animate-spin" />
              {t("common.loading")}
            </div>
          ) : bikes.length === 0 ? (
            <p className="p-2 text-sm text-muted-foreground">{t("bike.startFromBikeEmpty")}</p>
          ) : (
            // One column, names wrapped over two lines rather than cut — installed bikes are
            // named like "MX1OEM_2023_Husqvarna_FC450", and the part that tells two apart is
            // the end. The full name is the tooltip too, for the rare one past two lines.
            <ul className="flex max-h-[min(18rem,45vh)] flex-col gap-0.5 overflow-y-auto pr-1">
              {bikes.map((b) => {
                const isPkz = b.kind === "pkz" && !b.locked;
                const stillChecking = isPkz && checking && readable[b.path] === undefined;
                const protectedPkz = isPkz && !stillChecking && readable[b.path] !== true;
                const blocked = !!b.locked || protectedPkz || stillChecking;
                const reason = b.locked
                  ? "bike.bikeLocked"
                  : stillChecking
                    ? "common.loading"
                    : "bike.bikeProtected";
                return (
                  <li key={`${b.path}#${b.prefix ?? ""}`}>
                    <button
                      type="button"
                      disabled={busy || blocked}
                      title={blocked ? `${b.name}\n${t(reason)}` : b.name}
                      onClick={() => {
                        onPick(b.path);
                        onOpenChange(false);
                      }}
                      className="flex w-full items-start gap-2 rounded-sm px-2 py-1.5 text-left text-[12.5px] hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50 disabled:hover:bg-transparent"
                    >
                      <Bike className="mt-px size-3.5 shrink-0 text-faint" />
                      <span className="line-clamp-2 min-w-0 flex-1 [overflow-wrap:anywhere]">{b.name}</span>
                      {blocked && <Lock className="mt-0.5 size-3 shrink-0 text-faint" />}
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
          <div className="my-2 h-px shrink-0 bg-border" />
          <button type="button" onClick={() => run("import")} disabled={busy} className={choice}>
            <FolderOpen className="size-3.5" />
            {t("bike.importFullBikeFile")}
          </button>
          {pending === "import" && <ReplaceLine onYes={() => run("import")} onNo={() => setPending(null)} />}
          <button
            type="button"
            onClick={() => run("placeholder")}
            disabled={busy}
            title={t("bike.placeholderHint")}
            className={choice}
          >
            <Grid2x2 className="size-3.5" />
            {t("bike.addPlaceholder")}
          </button>
          {pending === "placeholder" && (
            <ReplaceLine onYes={() => run("placeholder")} onNo={() => setPending(null)} />
          )}
          <BlenderNeeded gate={gate} at="base" className="px-2 pt-1" />
          {problem && (
            <p className="flex items-start gap-1.5 px-2 pt-2 text-[11.5px] text-amber-500">
              <AlertTriangle className="mt-px size-3.5 shrink-0" />
              <span className="min-w-0 [overflow-wrap:anywhere]">{t("bike.templateProblem", { problem })}</span>
            </p>
          )}
        </PopoverContent>
      </Popover>
    </div>
  );
}

/** The one-line "this throws work away" check, under the pick that would. */
function ReplaceLine({ onYes, onNo }: { onYes: () => void; onNo: () => void }) {
  const t = useT();
  return (
    <div className="flex items-center gap-2 px-2 pb-1 text-[12px] text-amber-500">
      <AlertTriangle className="size-3.5 shrink-0" />
      <span className="min-w-0 flex-1">{t("bike.replaceConfirm")}</span>
      <Button size="sm" variant="destructive" className="h-6 px-2 text-[11.5px]" onClick={onYes}>
        {t("bike.replace")}
      </Button>
      <Button size="sm" variant="ghost" className="h-6 px-2 text-[11.5px]" onClick={onNo}>
        {t("common.cancel")}
      </Button>
    </div>
  );
}
