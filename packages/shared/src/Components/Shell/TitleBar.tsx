import type { ReactNode } from "react";
import { cn } from "../../lib/utils";
import WindowControls, { IS_MAC } from "./WindowControls";

/**
 * A frameless window's title bar: a draggable strip with the window buttons at the right.
 *
 * The window is `decorations: false`, so the OS title bar is gone and this is the only place
 * to grab it. An app puts its own menus in `children`; the rest of the bar stays draggable.
 */
export default function TitleBar({ children }: { children?: ReactNode }) {
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-9 flex-none select-none items-center border-b border-white/[0.08] bg-background",
        // Clear the space macOS reserves for its traffic-lights.
        IS_MAC ? "pl-[78px] pr-2" : "pl-3",
      )}
    >
      {children}

      {/* The rest of the bar is draggable. */}
      <div data-tauri-drag-region className="h-full flex-1" />

      <WindowControls />
    </div>
  );
}
