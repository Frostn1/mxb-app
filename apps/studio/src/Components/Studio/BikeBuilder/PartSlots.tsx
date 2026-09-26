import { useState } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { ROLES } from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";

/**
 * The outliner: the bike's ten roles and what fills each, as a compact list rather than the
 * spacious grid this used to be. The 3D viewport is the main panel now — a rider places parts
 * by dragging them onto it, or by clicking an open mount — so this is where to check what's
 * still missing, not where placing happens. Collapsible, since checking it is occasional and
 * the viewport can use the width the rest of the time.
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
          return (
            <li
              key={role}
              className="flex items-center gap-2 border-b border-border/60 py-1.5 text-[12px] last:border-0"
            >
              <span className="w-24 shrink-0 truncate text-muted-foreground">{t(`bike.role.${role}`)}</span>
              <span className="min-w-0 flex-1 truncate">{filled?.name ?? t("bike.emptySlot")}</span>
              {filled && (
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
              )}
            </li>
          );
        })}
      </ul>
    </section>
  );
}
