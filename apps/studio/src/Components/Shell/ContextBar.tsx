import { createContext, useContext, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { cn } from "@frost/shared/lib/utils";

/**
 * The two ends of the context bar, handed to whichever screen is mounted.
 *
 * A near-copy of the manager's. The two apps' chrome is genuinely different — the manager
 * has a rail of places, the studio a row of tools — so only the portal slots and the tab
 * are shared in shape, and sharing them for real would mean a component parameterised on
 * a nav model neither app would recognise.
 *
 * A screen's own toolbar belongs in this row, not in a third row of its own — that stacking
 * is what made the old shell feel like a dashboard. Screens fill the ends by portalling
 * into them, so nothing has to be lifted into `Dashboard` state and re-rendered from there.
 */
export const ContextSlots = createContext<{ left: HTMLElement | null; right: HTMLElement | null }>({
  left: null,
  right: null,
});

/** Tabs or filters, beside the rail item's own tabs. */
export function ContextBarLeft({ children }: { children: ReactNode }) {
  const { left } = useContext(ContextSlots);
  return left ? createPortal(children, left) : null;
}

/** Search, sort, a primary action — pinned right. */
export function ContextBarRight({ children }: { children: ReactNode }) {
  const { right } = useContext(ContextSlots);
  return right ? createPortal(children, right) : null;
}

/** One tab, so a screen's own tabs are indistinguishable from the rail item's. */
export function ContextTab({
  active,
  onSelect,
  children,
}: {
  active: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      onClick={onSelect}
      className={cn(
        "relative flex cursor-default items-center font-cond text-[12.5px] font-semibold uppercase tracking-[0.16em] transition-colors",
        active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
      {active && <span className="u-skew absolute inset-x-[-3px] bottom-0 h-[2px] bg-primary" />}
    </button>
  );
}
