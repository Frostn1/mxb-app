import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { cn } from "@frost/shared/lib/utils";
import { useT } from "@frost/shared/i18n/context";

const appWindow = getCurrentWindow();

/**
 * macOS draws its own traffic-lights (and rounds the window) because the mac config uses
 * `titleBarStyle: "Overlay"`. Everywhere else the window is frameless
 * (`decorations: false`), so we render our own controls.
 */
export const IS_MAC = navigator.userAgent.includes("Mac");

export default function WindowControls({ className }: { className?: string }) {
  const t = useT();
  if (IS_MAC) return null;
  return (
    <div className={cn("flex h-full", className)}>
      <button
        onClick={() => appWindow.minimize()}
        title={t("window.minimize")}
        className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-foreground/[0.06]"
      >
        <Minus className="size-4" />
      </button>
      <button
        onClick={() => appWindow.toggleMaximize()}
        title={t("window.maximize")}
        className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-foreground/[0.06]"
      >
        <Square className="size-[13px]" />
      </button>
      <button
        onClick={() => appWindow.close()}
        title={t("window.close")}
        className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-destructive hover:text-destructive-foreground"
      >
        <X className="size-4" />
      </button>
    </div>
  );
}
