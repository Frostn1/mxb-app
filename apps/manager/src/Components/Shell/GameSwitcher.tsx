import { useState } from "react";
import { Check, ChevronsUpDown, Loader2 } from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@frost/shared/Components/ui/dropdown-menu";
import { useConfig } from "@frost/shared/Context/Config";
import { useT } from "@frost/shared/i18n/context";
import type { GameId } from "@frost/shared/types";

/**
 * Picks which PiBoSo title the app is driving.
 *
 * Hidden entirely when the build only knows one game, so a single-title install doesn't
 * carry a control with nothing to choose.
 *
 * `compact` is the top-rail shape: one line beside the wordmark, where a stacked
 * label-over-value would not fit the 52px row. Settings uses the full form.
 */
export default function GameSwitcher({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const { games, game, switchGame } = useConfig();
  const [busy, setBusy] = useState(false);

  if (games.length < 2) return null;

  const pick = async (id: GameId) => {
    if (id === game.id || busy) return;
    setBusy(true);
    try {
      await switchGame(id);
    } catch (err) {
      // The switch is a config write plus a Steam scan; if it fails the app is still on
      // the game it was on, so say so rather than leaving the label looking changed.
      toast.error(t("game.switchFailed"), { description: String(err) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={busy}
        className={cn(
          "flex cursor-default items-center gap-2 border border-input text-left transition-colors hover:bg-foreground/[0.05] disabled:opacity-60",
          compact ? "h-7 px-2.5" : "w-full rounded-lg px-3 py-2",
        )}
      >
        {compact ? (
          <span className="truncate font-cond text-[12.5px] font-semibold uppercase tracking-[0.1em]">
            {game.display}
          </span>
        ) : (
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="text-[10px] font-medium uppercase tracking-wide text-muted-foreground">
              {t("game.label")}
            </span>
            <span className="truncate text-[12.5px] font-semibold">
              {game.display}
            </span>
          </div>
        )}
        {busy ? (
          <Loader2 className="size-3.5 flex-none animate-spin text-muted-foreground" />
        ) : (
          <ChevronsUpDown className="size-3.5 flex-none text-muted-foreground" />
        )}
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="w-[196px]">
        <DropdownMenuLabel>{t("game.switch")}</DropdownMenuLabel>
        {games.map((g) => (
          <DropdownMenuItem key={g.id} onSelect={() => void pick(g.id)}>
            <Check
              className={cn(
                "size-3.5",
                g.id === game.id ? "opacity-100" : "opacity-0",
              )}
            />
            <span>{g.display}</span>
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
