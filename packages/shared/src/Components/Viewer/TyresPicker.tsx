import { useT } from "../../i18n/context";
import { cn } from "../../lib/utils";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "../ui/select";
import { BIKE_OWN_TYRES, type TyresPick } from "./tyresPick";

/**
 * Which tyre pack to draw a bike on — the same control in each of the three previews.
 *
 * Hidden when nothing is installed under `mods/tyres`: with no pack to switch to, the only
 * entry would be the one the bike already names, which is a dropdown that does nothing.
 * The pick itself lives in {@link TyresPick} so the caller can load the model with it.
 */
export function TyresPicker({ pick, className }: { pick: TyresPick; className?: string }) {
  const t = useT();
  if (!pick.options.length) return null;
  return (
    <div className={cn("flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground", className)}>
      <span className="flex-none">{t("viewer.tyres")}</span>
      <Select value={pick.tyres} onValueChange={pick.choose}>
        <SelectTrigger className="h-7 min-w-0 max-w-[150px] text-xs">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value={BIKE_OWN_TYRES}>{t("viewer.tyresOwn")}</SelectItem>
          {pick.options.map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}
