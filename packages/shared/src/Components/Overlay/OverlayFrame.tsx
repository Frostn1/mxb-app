import { useEffect, type ReactNode } from "react";
import { ExternalLink, X, type LucideIcon } from "lucide-react";
import { useT } from "../../i18n/context";
import type { BaseTKey } from "../../i18n/core";
import { prettyHotkey } from "../../lib/hotkey";
import { usePlatform } from "../../lib/usePlatform";
import { cn } from "../../lib/utils";

export interface FrameTab {
  id: string;
  label: string;
  icon?: LucideIcon;
}

/** The label of a tab the other app offers. Ids come from its overlay spec in Rust. */
export function peerTabLabel(t: (key: BaseTKey) => string, id: string): string {
  const key = `overlay.tab.${id}` as BaseTKey;
  const label = t(key);
  return label === key ? id : label;
}

/**
 * The in-game overlay's panel, in either app: a header with the app's tabs, the linked app's
 * tabs after them, the hotkey hint and the way out. Theme tokens only, so each app's skin
 * applies. The app renders its tab's content as children.
 */
export default function OverlayFrame({
  appName,
  tabs,
  active,
  onTab,
  peerName,
  peerTabs,
  onPeerTab,
  hotkey,
  onOpenMain,
  onClose,
  children,
}: {
  appName: string;
  tabs: FrameTab[];
  active: string;
  onTab: (id: string) => void;
  /** The other app, while linked. */
  peerName?: string;
  peerTabs?: FrameTab[];
  onPeerTab?: (id: string) => void;
  hotkey?: string;
  onOpenMain: () => void;
  onClose: () => void;
  children: ReactNode;
}) {
  const t = useT();
  const isMac = usePlatform() === "macos";

  // Transparent window: the page must not paint a background of its own.
  useEffect(() => {
    document.documentElement.classList.add("overlay-window");
    return () => document.documentElement.classList.remove("overlay-window");
  }, []);

  // Esc is the way out — the same reflex that closes the game's own menus. Also
  // blocks the webview's refresh/find shortcuts, as the main window does.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.code === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if ((e.ctrlKey && (e.code === "KeyF" || e.code === "KeyR")) || e.code === "F5") {
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const tabButton = ({ id, label, icon: Icon }: FrameTab, on: boolean, pick: (id: string) => void) => (
    <button
      key={id}
      onClick={() => pick(id)}
      className={cn(
        "flex cursor-default items-center gap-1.5 whitespace-nowrap rounded-lg px-2.5 py-1.5 text-[12.5px] transition-colors",
        on
          ? "bg-accent font-semibold text-accent-foreground"
          : "font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
      )}
    >
      {Icon && <Icon className="size-3.5" />}
      <span>{label}</span>
    </button>
  );

  return (
    // The margin is what makes this read as an overlay: the game shows through around a
    // floating, rounded panel.
    <div className="h-screen w-screen p-2">
      <div className="flex h-full flex-col overflow-hidden rounded-xl border border-border bg-window/92 text-foreground shadow-2xl backdrop-blur-xl">
        <header
          data-tauri-drag-region
          className="flex h-[42px] flex-none select-none items-center gap-3 border-b border-border pl-4 pr-1.5"
        >
          <span data-tauri-drag-region className="text-[13px] font-bold tracking-[0.2px]">
            {appName}
          </span>

          <nav className="flex min-w-0 items-center gap-0.5 overflow-x-auto">
            {tabs.map((tab) => tabButton(tab, tab.id === active, onTab))}
            {peerName && peerTabs && peerTabs.length > 0 && onPeerTab && (
              <>
                <span className="mx-1.5 h-4 w-px flex-none bg-border" />
                <span className="mr-1 whitespace-nowrap text-[11px] text-muted-foreground">{peerName}</span>
                {peerTabs.map((tab) => tabButton(tab, false, onPeerTab))}
              </>
            )}
          </nav>

          <div data-tauri-drag-region className="ml-auto flex items-center gap-1">
            {hotkey && (
              <span data-tauri-drag-region className="hidden whitespace-nowrap text-[11px] text-muted-foreground lg:inline">
                {t("overlay.toClose", { hotkey: prettyHotkey(hotkey, isMac) })}
              </span>
            )}
            <button
              onClick={onOpenMain}
              title={t("overlay.openMainTitle", { app: appName })}
              className="flex h-8 cursor-default items-center gap-1.5 rounded-lg px-2.5 text-[12.5px] font-medium text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
            >
              <ExternalLink className="size-3.5" />
              <span className="hidden sm:inline">{t("overlay.openMain")}</span>
            </button>
            <button
              onClick={onClose}
              title={t("overlay.closeTitle")}
              className="grid size-8 cursor-default place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
            >
              <X className="size-4" />
            </button>
          </div>
        </header>

        <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background/85">{children}</main>
      </div>
    </div>
  );
}
