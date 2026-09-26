import { Box } from "lucide-react";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import { ROLES, type LibraryPart } from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";

function Thumb({ part }: { part: LibraryPart | undefined }) {
  return part?.thumb ? (
    <img src={part.thumb} alt="" className="size-12 shrink-0 bg-muted/40 object-contain" draggable={false} />
  ) : (
    <div className="flex size-12 shrink-0 items-center justify-center bg-muted/40 text-faint">
      <Box className="size-5" />
    </div>
  );
}

/**
 * The middle panel: the bike's ten roles, one part in each. Split out of the tray so the
 * slots read as their own list — what the bike needs — rather than a second section under
 * the parts you happen to have brought in.
 */
export default function PartSlots({ lib }: { lib: ReturnType<typeof useBikeLibrary> }) {
  const t = useT();
  const { parts, slots, busy, onSlot } = lib;
  const byId = new Map((parts ?? []).map((p) => [p.id, p]));

  return (
    <section className="flex h-full min-w-0 flex-col gap-3 overflow-y-auto p-4">
      <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.slots")}</h2>
      <p className="text-sm text-muted-foreground">{t("bike.slotsHint")}</p>
      <ul className="flex flex-col gap-2">
        {ROLES.map((role) => {
          const filled = slots[role] ? byId.get(slots[role]!) : undefined;
          const fits = (parts ?? []).filter((p) => p.role === role);
          return (
            <li key={role} className="flex items-center gap-2 border border-border bg-background p-2">
              <Thumb part={filled} />
              <div className="flex min-w-0 flex-1 flex-col gap-1">
                <span className="text-[11px] font-semibold uppercase tracking-[0.06em] text-faint">
                  {t(`bike.role.${role}`)}
                </span>
                <Select
                  value={filled?.id ?? NONE}
                  onValueChange={(v) => onSlot(role, v)}
                  disabled={busy || fits.length === 0}
                >
                  <SelectTrigger className="h-8">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value={NONE}>
                      {fits.length === 0 ? t("bike.noPartsForRole") : t("bike.emptySlot")}
                    </SelectItem>
                    {fits.map((p) => (
                      <SelectItem key={p.id} value={p.id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
