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

/**
 * "Start from an installed bike": the mod bikes already in the rider's `mods/bikes` folder,
 * so the template picker is a list of real bikes instead of a file dialog and a button whose
 * name ("Add placeholder bike") nobody could place. A locked (mxbsecure) mod is listed, not
 * hidden, but can't be picked — Studio never decrypts secured content to read it.
 *
 * OEM/stock bikes aren't in this list yet: they ship inside the game's own locked archive,
 * and reading even their names needs a bit of the same care as a locked mod. Left for a
 * follow-up rather than guessed at here.
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

  useEffect(() => {
    if (!open || bikes !== null) return;
    scanLibrary("mods/bikes")
      .then((entries) => setBikes(entries.filter((e) => e.category === "bike")))
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
            {bikes.map((b) => (
              <li key={`${b.path}#${b.prefix ?? ""}`}>
                <button
                  type="button"
                  disabled={!!b.locked}
                  title={b.locked ? t("bike.bikeLocked") : undefined}
                  onClick={() => {
                    onPick(b.path);
                    setOpen(false);
                  }}
                  className="flex w-full items-center gap-2 px-2 py-1.5 text-left text-sm hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <span className="min-w-0 flex-1 truncate">{b.name}</span>
                  {b.locked && <Lock className="size-3 shrink-0 text-faint" />}
                </button>
              </li>
            ))}
          </ul>
        )}
      </PopoverContent>
    </Popover>
  );
}
