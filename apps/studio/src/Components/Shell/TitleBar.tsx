import { Fragment } from "react";
import { Settings } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import type { RailEntry } from "@frost/shared/Components/Shell/Rail";
import WindowControls, { IS_MAC } from "@frost/shared/Components/Shell/WindowControls";

/**
 * Studio chrome in the same silhouette as MXB App: plate wordmark, horizontal destinations,
 * active underline and window actions in one 52px rail.
 *
 * No File menu here any more: everything in it acts on the Editor's paint, so it lives in the
 * Editor's own toolbar (`Designer/EditorFileMenu.tsx`) and its shortcuts only answer there.
 * The tabs split into two groups with a thin rule between them — the bike (Editor, Bike) and
 * everything around it.
 */
export default function TitleBar<T extends string>({
  entries,
  active,
  onPick,
  settingsLabel,
  onSettings,
}: {
  entries: RailEntry<T>[];
  active: T | "settings";
  onPick: (id: T) => void;
  settingsLabel: string;
  onSettings: () => void;
}) {
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-[52px] flex-none select-none items-center border-b border-border bg-window",
        IS_MAC ? "pl-[82px] pr-4" : "pl-[18px]",
      )}
    >
      {/* Coach's own wordmark: a headline and "by mxbsecure" under it, not a badge-plus-label
          pair borrowed from the App's number-plate chrome. */}
      <div data-tauri-drag-region className="flex shrink-0 flex-col justify-center leading-none">
        <span className="headline text-[16px] leading-none">Studio</span>
        <span className="mt-0.5 text-[10px] leading-none text-muted-foreground">by mxbsecure</span>
      </div>

      <nav className="ml-6 flex h-full min-w-0 items-stretch gap-[22px] overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        {entries.map((entry, index) => {
          const on = active === entry.id;
          const divided = index > 0 && entry.group !== entries[index - 1].group;
          return (
            <Fragment key={entry.id}>
              {divided && <span className="my-auto h-5 w-px shrink-0 bg-border" />}
              <button
                onClick={() => onPick(entry.id)}
                aria-current={on ? "page" : undefined}
                className={cn(
                  "relative flex shrink-0 cursor-default items-center font-cond text-[13px] font-semibold tracking-[-0.03em] transition-colors",
                  on ? "text-foreground" : "text-muted-foreground hover:text-foreground",
                )}
              >
                {entry.label}
                {on && (
                  <span className="u-skew absolute inset-x-[-3px] bottom-0 h-[3px] bg-primary" />
                )}
              </button>
            </Fragment>
          );
        })}
      </nav>

      <div data-tauri-drag-region className="min-w-4 flex-1" />
      <button
        onClick={onSettings}
        title={settingsLabel}
        aria-label={settingsLabel}
        className={cn(
          "grid size-[30px] shrink-0 cursor-default place-items-center text-muted-foreground transition-colors hover:text-foreground",
          active === "settings" && "bg-popover text-foreground",
        )}
      >
        <Settings className="size-4" />
      </button>
      <WindowControls className="ml-4" />
    </div>
  );
}
