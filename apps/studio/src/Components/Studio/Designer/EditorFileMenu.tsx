import { useEffect, useRef } from "react";
import { ChevronDown } from "lucide-react";
import { IS_MAC } from "@frost/shared/Components/Shell/WindowControls";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
} from "@frost/shared/Components/ui/dropdown-menu";

/**
 * The Editor's File menu and its shortcuts.
 *
 * This used to sit in the window's top bar, beside the tabs, and every shortcut was bound for
 * the whole window. Everything in it acts on the Editor's paint, though: Ctrl+S on the Bike tab
 * saved a paint you could not see. So the menu lives in the Editor's own toolbar and the keys
 * only answer while the Editor is the tab on screen.
 *
 * Off macOS, that is. There the native menu bar owns the same accelerators
 * (`app_menu` in `src-tauri/src/main.rs`) and its items arrive as the `menu` event, which the
 * Editor gates on the same flag.
 */
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

/** Binds the File shortcuts while `active`; each one calls `run` with the item's id. */
export function useEditorShortcuts(active: boolean, run: (id: string) => void) {
  const runRef = useRef(run);
  runRef.current = run;
  useEffect(() => {
    if (IS_MAC || !active) return;
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey) return;
      const hit = SHORTCUTS.find(
        (shortcut) =>
          shortcut.key === event.key.toLowerCase() && !!shortcut.shift === event.shiftKey,
      );
      if (!hit) return;
      event.preventDefault();
      runRef.current(hit.id);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [active]);
}

function Item({
  id,
  label,
  accel,
  run,
}: {
  id: string;
  label: string;
  accel: string;
  run: (id: string) => void;
}) {
  return (
    <DropdownMenuItem onSelect={() => run(id)}>
      {label}
      <span className="ml-auto pl-8 font-mono text-[10px] text-muted-foreground/60">{accel}</span>
    </DropdownMenuItem>
  );
}

/** The File menu, as the first control in the Editor's toolbar. */
export default function EditorFileMenu({ run }: { run: (id: string) => void }) {
  const mod = IS_MAC ? "⌘" : "Ctrl+";
  return (
    <DropdownMenu>
      <DropdownMenuTrigger className="flex flex-none cursor-default items-center gap-0.5 rounded-md py-1 pl-2 pr-1.5 text-[12.5px] text-muted-foreground outline-none transition-colors hover:bg-foreground/[0.06] hover:text-foreground data-[state=open]:bg-foreground/[0.06] data-[state=open]:text-foreground">
        File
        <ChevronDown className="size-3.5 opacity-60" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="min-w-[15rem]">
        <Item run={run} id="new-paint" label="New Paint" accel={`${mod}N`} />
        <Item run={run} id="open-paint" label="Open a Paint…" accel={`${mod}O`} />
        <Item run={run} id="open-psd" label="Open a Photoshop File…" accel={`${mod}Shift+O`} />
        <DropdownMenuSeparator />
        <Item run={run} id="add-sheet" label="Add a Sheet" accel={`${mod}Shift+N`} />
        <Item
          run={run}
          id="sheet-from-image"
          label="Add a Sheet from an Image…"
          accel={`${mod}I`}
        />
        <DropdownMenuSeparator />
        <Item run={run} id="save" label="Save Paint" accel={`${mod}S`} />
        <Item run={run} id="export-psd" label="Export PSD…" accel={`${mod}E`} />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
