import { useState } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@frost/shared/Components/ui/select";
import { useT } from "@/i18n";
import { ROLES } from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";

/**
 * The outliner: the bike's ten roles and what fills each, as a compact list rather than the
 * spacious grid this used to be. The 3D viewport is the main panel now — a rider can drag a
 * part onto it or click an open mount to fill a role — but not every role has a mount to
 * click (the chassis has none: everything else mounts *on* it) and not every rider's system
 * plays along with an in-page drag over a Tauri window. So an empty row keeps a plain picker
 * of its own — the one way to fill a role that never depends on the viewport at all.
 */
export default function PartSlots({ lib }: { lib: ReturnType<typeof useBikeLibrary> }) {
  const t = useT();
  const [open, setOpen] = useState(true);
  const { parts, slots, busy, onSlot } = lib;
  const byId = new Map((parts ?? []).map((p) => [p.id, p]));

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        title={t("bike.slots")}
        className="flex h-full w-8 shrink-0 items-center justify-center border-l border-border bg-window text-faint hover:text-foreground"
      >
        <ChevronLeft className="size-4" />
      </button>
    );
  }

  return (
    <section data-dock="right" className="flex h-full w-64 shrink-0 flex-col gap-2 overflow-y-auto p-3">
      <div className="flex items-center gap-2">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.09em] text-faint">{t("bike.slots")}</h2>
        <button
          type="button"
          onClick={() => setOpen(false)}
          title={t("bike.collapseSlots")}
          className="ml-auto text-faint hover:text-foreground"
        >
          <ChevronRight className="size-4" />
        </button>
      </div>
      <ul className="flex flex-col gap-1">
        {ROLES.map((role) => {
          const filled = slots[role] ? byId.get(slots[role]!) : undefined;
          const fits = (parts ?? []).filter((p) => p.role === role);
          return (
            <li key={role} className="flex flex-col gap-1 border-b border-border/60 py-1.5 last:border-0">
              <span className="text-[11px] font-semibold uppercase tracking-[0.04em] text-faint">
                {t(`bike.role.${role}`)}
              </span>
              {filled ? (
                <div className="flex items-center gap-2 text-[12px]">
                  <span className="min-w-0 flex-1 truncate">{filled.name}</span>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="size-6 shrink-0 p-0"
                    onClick={() => onSlot(role, NONE)}
                    disabled={busy}
                    title={t("bike.removeFromSlot")}
                  >
                    <X className="size-3" />
                  </Button>
                </div>
              ) : (
                <Select value={NONE} onValueChange={(v) => onSlot(role, v)} disabled={busy || fits.length === 0}>
                  <SelectTrigger className="h-7 text-[12px]">
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
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
