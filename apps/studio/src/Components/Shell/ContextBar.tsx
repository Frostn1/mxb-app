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

/**
 * Whether the tool doing the portalling is the one on screen.
 *
 * The Studio hides sub-views rather than unmounting them, so they keep their work — which
 * means every tool anyone has opened is still mounted, and every one of their portals still
 * renders. Without this gate the second tool you visit puts its toolbar in the bar beside
 * the first one's and neither leaves. Default true, so a tool used outside a `Pane` needs
 * no provider.
 */
export const PaneActive = createContext(true);

/**
 * A tool saying it is showing something that owns the window.
 *
 * The Designer's start screen is a page, not a document with chrome around it — and the
 * strip above it held one question mark. A tool sets `bare` while that is true and the shell
 * drops the strip; nothing else in the app has to know why.
 */
/**
 * Work that would be lost by closing the window.
 *
 * A tool registers what it is holding and how to save it; the shell asks on the way out. Kept
 * as a registry rather than a boolean so the answer comes from whichever tool actually has the
 * work, and so "save" means that tool's save rather than something the shell has to know how
 * to do.
 */
export interface UnsavedWork {
  /** True when closing now would lose something. */
  dirty: () => boolean;
  /**
   * Save it. Returns false when it could not — including when it needs the user first, in
   * which case the tool has put them where they need to be and the close should be dropped.
   */
  save: () => Promise<boolean>;
}

export const UnsavedRegistry = createContext<{
  register: (w: UnsavedWork | null) => void;
}>({ register: () => {} });

export const ShellChrome = createContext<{ bare: boolean; setBare: (v: boolean) => void }>({
  bare: false,
  setBare: () => {},
});

/** Tabs or filters, beside the rail item's own tabs. */
export function ContextBarLeft({ children }: { children: ReactNode }) {
  const { left } = useContext(ContextSlots);
  const on = useContext(PaneActive);
  return left && on ? createPortal(children, left) : null;
}

/** Search, sort, a primary action — pinned right. */
export function ContextBarRight({ children }: { children: ReactNode }) {
  const { right } = useContext(ContextSlots);
  const on = useContext(PaneActive);
  return right && on ? createPortal(children, right) : null;
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
