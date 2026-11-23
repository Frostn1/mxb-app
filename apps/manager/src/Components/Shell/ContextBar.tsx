import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
  type Ref,
  Fragment,
} from "react";
import { createPortal } from "react-dom";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@/i18n";
import type { DashboardView, RailItem } from "./nav";

/**
 * The two ends of the context bar, handed to whichever screen is mounted.
 *
 * A screen's own toolbar belongs in this row, not in a third row of its own — that stacking
 * is what made the old shell feel like a dashboard. Screens fill the ends by portalling
 * into them, so nothing has to be lifted into `Dashboard` state and re-rendered from there.
 */
export interface ContextSlotTargets {
  left: HTMLElement | null;
  right: HTMLElement | null;
}

export const ContextSlots = createContext<ContextSlotTargets>({
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
        "relative flex cursor-default items-center font-cond text-[12.5px] font-semibold tracking-[-0.02em] transition-colors",
        active ? "text-foreground" : "text-muted-foreground hover:text-foreground",
      )}
    >
      {children}
      {active && <span className="mxb-tab-marker absolute inset-x-[-3px] bottom-0 h-[2px] rounded-full bg-primary" />}
    </button>
  );
}

interface ContextBarProps {
  item?: RailItem;
  view: DashboardView;
  onNavigate: (view: DashboardView) => void;
  leftRef: Ref<HTMLDivElement>;
  rightRef: Ref<HTMLDivElement>;
}

function setRef(ref: Ref<HTMLDivElement>, element: HTMLDivElement | null) {
  if (typeof ref === "function") ref(element);
  else if (ref) (ref as { current: HTMLDivElement | null }).current = element;
}

/**
 * The row under the rail: the active rail item's tabs, then whatever the screen adds.
 *
 * Hidden when there are no tabs or screen-provided controls, because an empty band reads as
 * broken. The portal target elements stay mounted even while the bar itself is hidden.
 */
export default function ContextBar({
  item,
  view,
    onNavigate,
  leftRef,
  rightRef,
}: ContextBarProps) {
  const t = useT();
  const tabs = item?.tabs ?? [];
  const leftElement = useRef<HTMLDivElement | null>(null);
  const rightElement = useRef<HTMLDivElement | null>(null);
  const [slotsHaveContent, setSlotsHaveContent] = useState(false);
  const setLeftElement = useCallback(
    (element: HTMLDivElement | null) => {
      leftElement.current = element;
      setRef(leftRef, element);
    },
    [leftRef],
  );
  const setRightElement = useCallback(
    (element: HTMLDivElement | null) => {
      rightElement.current = element;
      setRef(rightRef, element);
    },
    [rightRef],
  );

  useEffect(() => {
    const update = () => {
      setSlotsHaveContent(
        Boolean(leftElement.current?.hasChildNodes() || rightElement.current?.hasChildNodes()),
      );
    };
    const observer = new MutationObserver(update);
    if (leftElement.current) observer.observe(leftElement.current, { childList: true });
    if (rightElement.current) observer.observe(rightElement.current, { childList: true });
    update();
    return () => observer.disconnect();
  }, []);

  return (
    <div
      className={cn(
        "h-11 flex-none items-stretch gap-[22px] border-b border-border bg-window px-7",
        tabs.length <= 1 && !slotsHaveContent ? "hidden" : "flex",
      )}
    >
      {/* A lone tab is a label, not a choice. */}
      {tabs.length > 1 &&
        tabs.map((tab) => (
          <Fragment key={tab.view}>
            {tab.dividerBefore && (
              <span aria-hidden className="w-px self-center bg-border" style={{ height: 20 }} />
            )}
            <ContextTab active={view === tab.view} onSelect={() => onNavigate(tab.view)}>
              {tab.rawLabel ?? t(tab.label)}
            </ContextTab>
          </Fragment>
        ))}
      {/* `min-w-0` so a screen's filters are what gives when the row is short. Without it
          the row's only flexible items were the right-hand actions, which meant a loaded
          screen compressed its own buttons — labels wrapping inside them — rather than
          trimming anything a person could do without. */}
      <div
        ref={setLeftElement}
        className={cn(
          "flex min-w-0 items-stretch gap-[22px]",
          // Only divide when there is something on both sides of the line.
          tabs.length > 1 && "ctx-divider",
        )}
      />
      <div className="min-w-[12px] flex-1" />
      <div
        ref={setRightElement}
        className="flex min-w-0 items-center gap-3 self-center"
      />
    </div>
  );
}
