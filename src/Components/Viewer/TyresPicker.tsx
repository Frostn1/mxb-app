import { useT } from "../../i18n/context";
import { cn } from "../../lib/utils";
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
    <label
      className={cn("flex items-center gap-1.5 text-xs text-muted-foreground", className)}
    >
      {t("viewer.tyres")}
      <select
        value={pick.tyres}
        onChange={(e) => pick.choose(e.target.value)}
        className="rounded-md border border-border bg-background px-2 py-1 text-xs text-foreground"
      >
        <option value={BIKE_OWN_TYRES}>{t("viewer.tyresOwn")}</option>
        {pick.options.map((name) => (
          <option key={name} value={name}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}
