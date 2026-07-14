import { Minus, Square, X, Snowflake } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { cn } from "@/lib/utils";

const appWindow = getCurrentWindow();

/**
 * macOS draws its own traffic-lights (and rounds the window) because the mac
 * config uses `titleBarStyle: "Overlay"`. Everywhere else the window is
 * frameless (`decorations: false`), so we render our own controls.
 */
const IS_MAC = navigator.userAgent.includes("Mac");

/** App brand + window controls. The whole bar is a drag region. */
export default function TitleBar() {
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-full select-none items-center justify-between border-b border-white/[0.06] bg-window",
        // Clear the space macOS reserves for its traffic-lights.
        IS_MAC ? "pl-[82px]" : "pl-4",
      )}
    >
      <div className="flex items-center gap-2.5" data-tauri-drag-region>
        <div className="grid size-[18px] place-items-center rounded-[5px] bg-gradient-to-br from-[#9ccfec] to-[#5d8fb0] text-[#0d0f12]">
          <Snowflake className="size-3" strokeWidth={2.5} />
        </div>
        <span className="text-[13px] font-bold tracking-[0.2px]">MXB App</span>
      </div>

      {!IS_MAC && (
        <div className="flex h-full">
          <button
            onClick={() => appWindow.minimize()}
            title="Minimize"
            className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-foreground/[0.06]"
          >
            <Minus className="size-4" />
          </button>
          <button
            onClick={() => appWindow.toggleMaximize()}
            title="Maximize"
            className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-foreground/[0.06]"
          >
            <Square className="size-[13px]" />
          </button>
          <button
            onClick={() => appWindow.close()}
            title="Close"
            className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-[#c4453c] hover:text-white"
          >
            <X className="size-4" />
          </button>
        </div>
      )}
    </div>
  );
}
