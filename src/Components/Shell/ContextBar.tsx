import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { useT } from "../../i18n/context";
import type { StudioTab } from "../Studio/Studio";
import type { DashboardView, RailItem } from "./nav";

interface ContextBarProps {
  item?: RailItem;
  view: DashboardView;
  studioTab: StudioTab;
  onNavigate: (view: DashboardView, studio?: StudioTab) => void;
  /** Per-screen controls — search, sort, a primary action — pinned to the right. */
  right?: ReactNode;
}

/**
 * The row under the rail: the active rail item's tabs, and whatever the screen wants on
 * the right.
 *
 * A rail item with one tab (or none) still gets the bar when it has `right` content to
 * show, but renders no tabs — a lone tab is a label, not a choice.
 */
export default function ContextBar({ item, view, studioTab, onNavigate, right }: ContextBarProps) {
  const t = useT();
  const tabs = item?.tabs ?? [];
  if (tabs.length < 2 && !right) return null;

  return (
    <div className="flex h-11 flex-none items-stretch gap-[22px] border-b border-border bg-window px-7">
      {tabs.length > 1 &&
        tabs.map((tab) => {
          // Studio's tabs all share `view: "studio"`, so the active one is the tab whose
          // sub-view is showing rather than the one whose view matches.
          const on = tab.studio
            ? view === "studio" && studioTab === tab.studio
            : view === tab.view;
          return (
            <button
              key={tab.studio ?? tab.view}
              onClick={() => onNavigate(tab.view, tab.studio)}
              className={cn(
                "relative flex cursor-default items-center font-cond text-[12.5px] font-semibold uppercase tracking-[0.16em] transition-colors",
                on ? "text-foreground" : "text-muted-foreground hover:text-foreground",
              )}
            >
              {tab.rawLabel ?? t(tab.label)}
              {on && (
                <span className="u-skew absolute inset-x-[-3px] bottom-0 h-[2px] bg-primary" />
              )}
            </button>
          );
        })}
      {right && (
        <>
          <div className="flex-1" />
          <div className="flex items-center gap-3 self-center">{right}</div>
        </>
      )}
    </div>
  );
}
