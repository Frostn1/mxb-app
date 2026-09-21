import Brand from "./Brand";
import { cn } from "@frost/shared/lib/utils";
import WindowControls, { IS_MAC } from "./WindowControls";

/**
 * The chrome for screens that have nothing to navigate: startup, and the setup screen a
 * first run lands on.
 *
 * It exists so a stalled startup can still be dragged and closed — the full rail needs a
 * config before it can render a single nav item, and a window with no controls at all is
 * a window you have to kill from the task manager.
 */
export default function MiniRail() {
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-[52px] flex-none select-none items-center border-b border-border bg-window",
        IS_MAC ? "pl-[82px] pr-4" : "pl-[18px]",
      )}
    >
      <Brand />
      <div data-tauri-drag-region className="flex-1" />
      <WindowControls />
    </div>
  );
}
