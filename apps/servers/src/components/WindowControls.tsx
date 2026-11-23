import { Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

export function WindowControls() {
  return (
    <div className="flex h-full">
      <button type="button" onClick={() => void appWindow.minimize()} title="Minimize" className="grid h-full w-11 place-items-center text-muted-foreground hover:bg-foreground/[0.06]"><Minus className="size-4" /></button>
      <button type="button" onClick={() => void appWindow.toggleMaximize()} title="Maximize" className="grid h-full w-11 place-items-center text-muted-foreground hover:bg-foreground/[0.06]"><Square className="size-3.5" /></button>
      <button type="button" onClick={() => void appWindow.close()} title="Close" className="grid h-full w-11 place-items-center text-muted-foreground hover:bg-destructive hover:text-destructive-foreground"><X className="size-4" /></button>
    </div>
  );
}
