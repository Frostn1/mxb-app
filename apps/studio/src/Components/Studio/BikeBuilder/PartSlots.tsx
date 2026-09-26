import { useState } from "react";
import { ChevronLeft, ChevronRight, X } from "lucide-react";
import { Button } from "@frost/shared/Components/ui/button";
import { useT } from "@/i18n";
import { ROLES } from "../../../api/bikebuild";
import { NONE, type useBikeLibrary } from "./useBikeLibrary";

/**
 * The outliner: the bike's ten roles and what fills each, read-only. Setting a part's role,
 * and putting it on the bike, both happen in the tray now — drag a part onto the viewport,
 * click an open mount, or the tray's own "Place" — so this is where to check what's still
 * missing, not a second place to do either. It used to keep a picker of its own for an empty
 * role with nothing to click (the chassis has no mount, since everything else mounts *on*
 * it); the tray's "Place" covers that role exactly as it covers every other one, so there's
 * one way to fill a slot, not two. Collapsible, since checking it is occasional.
 */
export default function PartSlots({ lib }: { lib: ReturnType<typeof useBikeLibrary> }) {
  const t = useT();
  const [open, setOpen] = useState(true);
  const { parts, slots, busy, onSlot } = lib;
  const byId = new Map((parts ?? []).map((p) => [p.id, p]));

  if (!open) {
    return (
      // Centered top-to-bottom used to read as "jumped to the middle" next to the expanded
      // header's chevron sitting at the top — pinned to the same spot here instead, so
      // collapsing doesn't move it.
      <button
        type="button"
        onClick={() => setOpen(true)}
        title={t("bike.slots")}
        className="flex h-full w-8 shrink-0 justify-center border-l border-border bg-window pt-3 text-faint hover:text-foreground"
      >
        <ChevronLeft className="size-4" />
      </button>
    );
  }

  return (
    <section data-dock="right" className="flex h-full w-56 shrink-0 flex-col gap-2 overflow-y-auto p-3">
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
      <p className="text-[11px] text-muted-foreground">{t("bike.slotsHint")}</p>
      <ul className="flex flex-col gap-1">
        {ROLES.map((role) => {
          const filled = slots[role] ? byId.get(slots[role]!) : undefined;
          return (
            <li key={role} className="flex flex-col gap-0.5 border-b border-border/60 py-1.5 last:border-0">
              <span className="text-[11px] font-semibold uppercase tracking-[0.04em] text-faint">
                {t(`bike.role.${role}`)}
              </span>
              <div className="flex items-center gap-2 text-[12px]">
                <span className={filled ? "min-w-0 flex-1 truncate" : "min-w-0 flex-1 truncate text-faint"}>
                  {filled?.name ?? t("bike.emptySlot")}
                </span>
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
              </div>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
