import { useCallback, useEffect, useState } from "react";
import { Bike, Home, Shirt, SlidersHorizontal, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { Toaster } from "@/Components/ui/sonner";
import { TooltipProvider } from "@/Components/ui/tooltip";
import Browse from "../Browse/Browse";
import Locker from "../Locker/Locker";
import ModDetail from "../ModDetail/ModDetail";
import Manage from "../Manage/Manage";
import Presets from "../Presets/Presets";
import { ThemeProvider } from "../../Context/Theme";
import { useI18n } from "../../i18n/context";
import type { TKey } from "../../i18n/core";
import { FrostmodProvider } from "../../Context/Frostmod";
import { ConfigContext } from "../../Context/Config";
import { InstallProvider } from "../../Context/Install";
import { useModBrowsing } from "../../lib/useModBrowsing";
import {
  bikePreviewAvailable,
  getConfig,
  getOverlayState,
  isConfigured,
  overlayHide,
} from "../../api/mods";
import type { Config } from "../../types";

/**
 * The in-game overlay: a compact, frameless panel drawn over MX Bikes and summoned by
 * a global hotkey (registered in `src-tauri/src/overlay.rs`).
 *
 * It reuses Presets, Locker, Browse and Manage unchanged rather than reimplementing them —
 * Presets and the Locker's model swaps are the two things that apply to a *running*
 * game via the loader re-run in `gameproc::refresh_look`, which is the whole reason
 * this window is worth having. Browse rides along so a mod can be queued mid-session, and
 * Manage so the next race can be lined up between sessions without leaving the game.
 *
 * Rider, Library and Settings stay in the main window: they're either GPU-hungry next
 * to a running game or not something you reach for from the pits.
 */

type OverlayTab = "presets" | "locker" | "browse" | "manage";

const TABS: { id: OverlayTab; label: TKey; icon: typeof Home }[] = [
  { id: "presets", label: "nav.presets", icon: Shirt },
  { id: "locker", label: "nav.locker", icon: Bike },
  { id: "browse", label: "nav.browse", icon: Home },
  { id: "manage", label: "nav.manage", icon: SlidersHorizontal },
];

/** Human-readable form of a Tauri accelerator, for the header hint. */
function prettyHotkey(accelerator: string) {
  const isMac = navigator.userAgent.includes("Mac");
  return accelerator
    .split("+")
    .map((part) =>
      part === "CommandOrControl" ? (isMac ? "Cmd" : "Ctrl") : part,
    )
    .join(" + ");
}

export default function Overlay() {
  const { t } = useI18n();
  const [ready, setReady] = useState(false);
  const [config, setConfig] = useState<Config | null>(null);
  const [bikePreview, setBikePreview] = useState(false);
  const [hotkey, setHotkey] = useState("");
  const [tab, setTab] = useState<OverlayTab>("presets");

  const showBrowse = useCallback(() => setTab("browse"), []);
  const {
    modType,
    changeType,
    selectedSlug,
    selectedCategoryId,
    installed,
    onInstalled,
    openMod,
    openModTarget,
    closeMod,
  } = useModBrowsing(showBrowse);

  const reloadConfig = useCallback(async () => {
    setConfig(await getConfig());
  }, []);

  const dismiss = useCallback(() => {
    void overlayHide().catch(() => {});
  }, []);

  useEffect(() => {
    (async () => {
      try {
        bikePreviewAvailable().then(setBikePreview).catch(() => {});
        getOverlayState()
          .then((s) => setHotkey(s.hotkey))
          .catch(() => {});
        if (await isConfigured()) await reloadConfig();
      } catch (err) {
        console.error("Overlay startup failed", err);
      } finally {
        setReady(true);
      }
    })();
  }, [reloadConfig]);

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
        dismiss();
        return;
      }
      if ((e.ctrlKey && (e.code === "KeyF" || e.code === "KeyR")) || e.code === "F5") {
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dismiss]);

  return (
    <ThemeProvider>
      <FrostmodProvider>
        <TooltipProvider delayDuration={300}>
          {/* The margin is what makes this read as an overlay: the game shows
              through around a floating, rounded panel. */}
          <div className="h-screen w-screen p-2">
            <div className="flex h-full flex-col overflow-hidden rounded-xl border border-white/[0.09] bg-window/92 text-foreground shadow-2xl backdrop-blur-xl">
              <header
                data-tauri-drag-region
                className="flex h-[42px] flex-none select-none items-center gap-3 border-b border-white/[0.06] pl-4 pr-1.5"
              >
                <span data-tauri-drag-region className="text-[13px] font-bold tracking-[0.2px]">
                  MXB App
                </span>

                <nav className="flex items-center gap-0.5">
                  {TABS.map(({ id, label, icon: Icon }) => (
                    <button
                      key={id}
                      onClick={() => {
                        setTab(id);
                        closeMod();
                      }}
                      className={cn(
                        "flex cursor-default items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-[12.5px] transition-colors",
                        tab === id
                          ? "bg-accent font-semibold text-accent-foreground"
                          : "font-medium text-muted-foreground hover:bg-foreground/[0.05] hover:text-foreground",
                      )}
                    >
                      <Icon className="size-3.5" />
                      <span>{t(label)}</span>
                    </button>
                  ))}
                </nav>

                <div data-tauri-drag-region className="ml-auto flex items-center gap-1">
                  {hotkey && (
                    <span
                      data-tauri-drag-region
                      className="hidden text-[11px] text-muted-foreground sm:inline"
                    >
                      {t("overlay.toClose", { hotkey: prettyHotkey(hotkey) })}
                    </span>
                  )}
                  <button
                    onClick={dismiss}
                    title={t("overlay.closeTitle")}
                    className="grid size-8 cursor-default place-items-center rounded-lg text-muted-foreground transition-colors hover:bg-foreground/[0.06] hover:text-foreground"
                  >
                    <X className="size-4" />
                  </button>
                </div>
              </header>

              <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background/85">
                {ready &&
                  (config ? (
                    <ConfigContext.Provider value={{ config, reloadConfig, bikePreview }}>
                      <InstallProvider onInstalled={onInstalled} onOpenMod={openModTarget}>
                        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
                          {tab === "presets" ? (
                            <Presets onOpenLocker={() => setTab("locker")} />
                          ) : tab === "locker" ? (
                            <Locker />
                          ) : tab === "manage" ? (
                            <Manage />
                          ) : selectedSlug ? (
                            <ModDetail
                              slug={selectedSlug}
                              modType={modType}
                              categoryId={selectedCategoryId ?? modType.categoryId}
                              installed={installed}
                              onBack={closeMod}
                            />
                          ) : (
                            <Browse
                              modType={modType}
                              installed={installed}
                              onOpenMod={openMod}
                              onChangeType={changeType}
                            />
                          )}
                        </div>
                      </InstallProvider>
                    </ConfigContext.Provider>
                  ) : (
                    // No config means the player never finished first-run setup. That
                    // wizard belongs in the main window, not over a running game.
                    <div className="grid flex-1 place-items-center px-8 text-center text-[13px] text-muted-foreground">
                      {t("overlay.needsSetup")}
                    </div>
                  ))}
              </main>
            </div>
          </div>
          <Toaster />
        </TooltipProvider>
      </FrostmodProvider>
    </ThemeProvider>
  );
}
