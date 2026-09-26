import { useEffect, useState } from "react";
import { Lock, Loader2 } from "lucide-react";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@frost/shared/Components/ui/popover";
import { Button } from "@frost/shared/Components/ui/button";
import { scanLibrary } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";
import { useT } from "@/i18n";
import { bikeTemplateReadable } from "../../../api/bikebuild";

/**
 * "Start from an installed bike": the mod bikes already in the rider's `mods/bikes` folder
 * (packed or installed as a plain folder), so the template picker is a list of real bikes
 * instead of a file dialog and a button whose name ("Add placeholder bike") nobody could
 * place. A locked (mxbsecure) mod is listed, not hidden, but can't be picked — Studio never
 * decrypts secured content to read it. Neither can a `.pkz` that isn't one of those *and*
 * isn't a plain zip either: that's the game's own protected format (OEM/stock content), and
 * a build without the (optional) sidecar module can't open it — `bikeTemplateReadable` is
 * what tells the difference, since `locked` alone only ever means the `.mxbsecure` case.
 *
 * OEM/stock content that ships inside the game's own install (not `mods/bikes`) isn't in
 * this list at all yet: finding it needs the same sidecar support, plus knowing where the
 * game keeps it, which is a bigger follow-up than this picker.
 */
export default function BikePicker({
  onPick,
  disabled,
}: {
  onPick: (path: string) => void;
  disabled?: boolean;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [bikes, setBikes] = useState<LibraryEntry[] | null>(null);
  /** Which `.pkz` paths this build actually opened — everything else (folders, and
   *  `.mxbsecure` blobs already caught by `locked`) doesn't need asking. */
  const [readable, setReadable] = useState<Record<string, boolean>>({});

  useEffect(() => {
    if (!open || bikes !== null) return;
    scanLibrary("mods/bikes")
      .then(async (entries) => {
        const list = entries.filter((e) => e.category === "bike");
        setBikes(list);
        const toCheck = list.filter((e) => e.kind === "pkz" && !e.locked).map((e) => e.path);
        if (toCheck.length) setReadable(await bikeTemplateReadable(toCheck).catch(() => ({})));
      })
      .catch(() => setBikes([]));
  }, [open, bikes]);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button size="sm" variant="outline" disabled={disabled}>
          {t("bike.startFromBike")}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="start" className="w-72 p-1.5">
        {bikes === null ? (
          <div className="flex items-center gap-2 p-2 text-sm text-muted-foreground">
            <Loader2 className="size-3.5 animate-spin" />
            {t("bike.looking")}
          </div>
        ) : bikes.length === 0 ? (
          <p className="p-2 text-sm text-muted-foreground">{t("bike.startFromBikeEmpty")}</p>
        ) : (
          <ul className="flex max-h-72 flex-col gap-0.5 overflow-y-auto">
            {bikes.map((b) => {
              const protectedPkz = b.kind === "pkz" && !b.locked && readable[b.path] === false;
              const blocked = !!b.locked || protectedPkz;
              return (
                <li key={`${b.path}#${b.prefix ?? ""}`}>
                  <button
                    type="button"
                    disabled={blocked}
                    title={blocked ? t(b.locked ? "bike.bikeLocked" : "bike.bikeProtected") : undefined}
                    onClick={() => {
                      onPick(b.path);
                      setOpen(false);
                    }}
                    className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    <span className="min-w-0 flex-1 truncate">{b.name}</span>
                    {blocked && <Lock className="size-3 shrink-0 text-faint" />}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
        <p className="border-t border-border px-2 pt-1.5 text-[11px] text-faint">
          {t("bike.startFromBikeScopeNote")}
        </p>
      </PopoverContent>
    </Popover>
  );
}
