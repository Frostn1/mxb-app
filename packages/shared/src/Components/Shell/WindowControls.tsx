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

export interface WindowLabels {
  minimize: string;
  maximize: string;
  close: string;
}

const EN: WindowLabels = { minimize: "Minimize", maximize: "Maximize", close: "Close" };

/** `wide` is the app title bar's size (`AppBar`); the studio's thin bar keeps the compact one. */
export default function WindowControls({
  className,
  labels = EN,
  wide,
}: {
  className?: string;
  labels?: WindowLabels;
  wide?: boolean;
}) {
  if (IS_MAC) return null;
  const btn = cn(
    "grid h-full cursor-default place-items-center text-muted-foreground transition-colors",
    wide ? "w-[46px] hover:bg-foreground/[0.06]" : "w-[42px] hover:bg-white/[0.06]",
  );
  return (
    <div className={cn("flex h-full", className)}>
      <button onClick={() => appWindow.minimize()} title={labels.minimize} className={btn}>
        <Minus className="size-4" />
      </button>
      <button onClick={() => appWindow.toggleMaximize()} title={labels.maximize} className={btn}>
        <Square className={wide ? "size-[13px]" : "size-[12px]"} />
      </button>
      <button
        onClick={() => appWindow.close()}
        title={labels.close}
        className={cn(btn, "hover:bg-destructive hover:text-destructive-foreground")}
      >
        <X className="size-4" />
      </button>
    </div>
  );
}
