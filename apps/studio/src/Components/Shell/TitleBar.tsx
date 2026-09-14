import { useEffect } from "react";
import { emit } from "@tauri-apps/api/event";
import { cn } from "@frost/shared/lib/utils";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@frost/shared/Components/ui/dropdown-menu";
import WindowControls, { IS_MAC } from "./WindowControls";

/** Every item just re-emits the `menu` event the editor already listens for, so the File menu and
 *  the shortcuts do exactly what the old native menu did — no second code path. */
const fire = (id: string) => void emit("menu", id);

/** Ctrl/Cmd + key → the same `menu` id, so the shortcuts keep working with no native menu.
 *  Off-macOS only: on macOS the native menu bar still carries the accelerators. */
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
      <span className="ml-auto pl-8 text-[11px] text-muted-foreground/60">{accel}</span>
    </DropdownMenuItem>
  );
}

/**
 * The app's own title bar. The window is frameless (`decorations: false`), so the OS title bar and
 * its menu strip are gone; this draws the Frost's Studio mark, a compact File menu, a draggable
 * region and the window buttons. Undo/redo/copy/paste stay with the webview and the editor.
 */
export default function TitleBar() {
  useEffect(() => {
    if (IS_MAC) return; // the native menu bar carries the accelerators on macOS
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey) return;
      const hit = SHORTCUTS.find(
        (s) => s.key === e.key.toLowerCase() && !!s.shift === e.shiftKey,
      );
      if (!hit) return;
      e.preventDefault();
      fire(hit.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-9 flex-none select-none items-center border-b border-white/[0.08] bg-background",
        // Clear the space macOS reserves for its traffic-lights.
        IS_MAC ? "pl-[78px] pr-2" : "pl-3",
      )}
    >
      <div>
        <DropdownMenu>
          <DropdownMenuTrigger className="cursor-default rounded px-2 py-0.5 text-[12.5px] text-muted-foreground outline-none transition-colors hover:bg-white/[0.06] hover:text-foreground data-[state=open]:bg-white/[0.06] data-[state=open]:text-foreground">
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
      </div>

      {/* The rest of the bar is draggable. */}
      <div data-tauri-drag-region className="h-full flex-1" />

      <WindowControls />
    </div>
  );
}
