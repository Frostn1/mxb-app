import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useT } from "../../i18n/context";
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
  const t = useT();
  return (
    <div
      data-tauri-drag-region
      className={cn(
        "flex h-full select-none items-center justify-between border-b border-white/[0.06] bg-window",
        // Clear the space macOS reserves for its traffic-lights.
        IS_MAC ? "pl-[82px]" : "pl-4",
      )}
    >
      <div data-tauri-drag-region />

      {!IS_MAC && (
        <div className="flex h-full">
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
            className="grid h-full w-[46px] cursor-default place-items-center text-muted-foreground transition-colors hover:bg-[#c4453c] hover:text-white"
          >
            <X className="size-4" />
          </button>
        </div>
      )}
    </div>
  );
}
