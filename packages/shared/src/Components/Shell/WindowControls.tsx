import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { cn } from "../../lib/utils";

const appWindow = getCurrentWindow();

/**
 * macOS draws its own traffic-lights (the mac config uses `titleBarStyle: "Overlay"`), so we
 * render nothing there. Everywhere else the window is frameless (`decorations: false`) and these
 * are the window's only controls.
 */
export const IS_MAC = navigator.userAgent.includes("Mac");

export default function WindowControls({ className }: { className?: string }) {
  if (IS_MAC) return null;
  return (
    <div className={cn("flex h-full", className)}>
      <button
        onClick={() => appWindow.minimize()}
        title="Minimize"
        className="grid h-full w-[42px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-white/[0.06]"
      >
        <Minus className="size-4" />
      </button>
      <button
        onClick={() => appWindow.toggleMaximize()}
        title="Maximize"
        className="grid h-full w-[42px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-white/[0.06]"
      >
        <Square className="size-[12px]" />
      </button>
      <button
        onClick={() => appWindow.close()}
        title="Close"
        className="grid h-full w-[42px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
      >
        <X className="size-4" />
      </button>
    </div>
  );
}
