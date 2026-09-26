import { Fragment, useEffect } from "react";
import { emit } from "@tauri-apps/api/event";
import { Settings } from "lucide-react";
import { cn } from "@frost/shared/lib/utils";
import type { RailEntry } from "@frost/shared/Components/Shell/Rail";
import WindowControls, { IS_MAC } from "@frost/shared/Components/Shell/WindowControls";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@frost/shared/Components/ui/dropdown-menu";

/** Every item re-emits the event the editor already listens for. */
const fire = (id: string) => void emit("menu", id);

const SHORTCUTS: { key: string; shift?: boolean; id: string }[] = [
  { key: "n", id: "new-paint" },
  { key: "o", id: "open-paint" },
  { key: "o", shift: true, id: "open-psd" },
  { key: "n", shift: true, id: "add-sheet" },
  { key: "i", id: "sheet-from-image" },
  { key: "s", id: "save" },
  { key: "e", id: "export-psd" },
  { key: "m", id: "toggle-model" },
  { key: "0", id: "reset-view" },
];

function Item({ id, label, accel }: { id: string; label: string; accel: string }) {
  return (
    <DropdownMenuItem onSelect={() => fire(id)}>
      {label}
      <span className="ml-auto pl-8 font-mono text-[10px] text-muted-foreground/60">{accel}</span>
    </DropdownMenuItem>
  );
}

/**
 * Studio chrome in the same silhouette as MXB App: plate wordmark, horizontal destinations,
 * active underline and window actions in one 52px rail. The File menu remains here because
 * the frameless window has no native menu off macOS.
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
  useEffect(() => {
    if (IS_MAC) return;
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const hit = SHORTCUTS.find(
        (shortcut) =>
          shortcut.key === event.key.toLowerCase() && !!shortcut.shift === event.shiftKey,
      );
      if (!hit) return;
      event.preventDefault();
      fire(hit.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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

      <DropdownMenu>
        <DropdownMenuTrigger className="ml-5 h-7 cursor-default border-l border-border px-4 font-cond text-[12px] font-semibold text-muted-foreground outline-none transition-colors hover:text-foreground data-[state=open]:text-foreground">
          File
        </DropdownMenuTrigger>
        <DropdownMenuContent align="start" className="min-w-[15rem]">
          <Item id="new-paint" label="New Paint" accel="Ctrl+N" />
          <Item id="open-paint" label="Open a Paint…" accel="Ctrl+O" />
          <Item id="open-psd" label="Open a Photoshop File…" accel="Ctrl+Shift+O" />
          <DropdownMenuSeparator />
          <Item id="add-sheet" label="Add a Sheet" accel="Ctrl+Shift+N" />
          <Item id="sheet-from-image" label="Add a Sheet from an Image…" accel="Ctrl+I" />
          <DropdownMenuSeparator />
          <Item id="save" label="Save Paint" accel="Ctrl+S" />
          <Item id="export-psd" label="Export PSD…" accel="Ctrl+E" />
        </DropdownMenuContent>
      </DropdownMenu>

      <nav className="ml-2 flex h-full min-w-0 items-stretch gap-[22px] overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
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
