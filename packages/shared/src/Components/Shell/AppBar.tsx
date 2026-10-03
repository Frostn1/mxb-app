import type { ReactNode } from "react";
import { Settings as SettingsIcon } from "lucide-react";
import { cn } from "../../lib/utils";
import Brand from "./Brand";
import WindowControls, { IS_MAC, type WindowLabels } from "./WindowControls";

export interface AppBarTab {
  id: string;
  label: string;
}

interface AppBarProps {
  /** The product name in the wordmark ("MXB App", "MXB Coach"). */
  name: string;
  /** Horizontal text tabs next to the wordmark. Omit for a bar with nothing to navigate. */
  tabs?: AppBarTab[];
  active?: string;
  onPick?: (id: string) => void;
  /** Extra controls just left of the settings gear. */
  right?: ReactNode;
  /** Shows the settings gear when given. */
  onSettings?: () => void;
  settingsActive?: boolean;
  settingsLabel?: string;
  /** The primary button (PLAY), after a divider. */
  action?: ReactNode;
  windowLabels?: WindowLabels;
}

/**
 * The chrome every MXB app wears: wordmark, tabs, settings gear, primary button and the window
 * controls, in one 52px row. MXB App and MXB Coach both render this, so their title bars cannot
 * drift apart — change the look here, not in an app.
 *
 * The whole row is a drag region; interactive children are buttons, and Tauri only drags from
 * elements carrying the attribute.
 */
export default function AppBar({
  name,
  tabs = [],
  active,
  onPick,
  right,
  onSettings,
  settingsActive,
  settingsLabel,
  action,
  windowLabels,
}: AppBarProps) {
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-[52px] flex-none select-none items-center border-b border-border bg-window",
        // Clear the space macOS reserves for its traffic-lights — and pad the trailing
        // edge there too, because mac draws none of our own window controls, so the primary
        // button would otherwise sit flush against the window edge.
        IS_MAC ? "pl-[82px] pr-4" : "pl-[18px]",
      )}
    >
      <Brand name={name} />

      {tabs.length > 0 && (
        <nav className="ml-7 flex h-full items-stretch gap-[22px]">
          {tabs.map((tab) => {
            const on = active === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => onPick?.(tab.id)}
                className={cn(
                  "relative flex cursor-default items-center font-cond text-[14px] font-semibold tracking-[-0.02em] transition-colors",
                  on ? "text-foreground" : "text-muted-foreground hover:text-foreground",
                )}
              >
                {tab.label}
                {on && (
                  <span className="absolute inset-x-[-3px] bottom-0 h-[3px] rounded-full bg-primary" />
                )}
              </button>
            );
          })}
        </nav>
      )}

      <div data-tauri-drag-region className="h-full flex-1" />

      <div className="flex items-center gap-1 text-muted-foreground">
        {right}
        {onSettings && (
          <button
            onClick={onSettings}
            title={settingsLabel}
            aria-label={settingsLabel}
            className={cn(
              "grid size-[30px] cursor-default place-items-center transition-colors hover:text-foreground",
              settingsActive && "bg-popover text-foreground",
            )}
          >
            <SettingsIcon className="size-4" />
          </button>
        )}
      </div>

      {action && (
        <>
          <span className="mx-3.5 h-5 w-px bg-border" />
          {action}
        </>
      )}

      <WindowControls className={action ? "ml-4" : undefined} labels={windowLabels} wide />
    </div>
  );
}
