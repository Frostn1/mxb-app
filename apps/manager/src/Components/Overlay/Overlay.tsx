import { useCallback, useEffect, useLayoutEffect, useMemo, useState } from "react";
import { Bike, Home, Shirt, SlidersHorizontal } from "lucide-react";
import { Toaster } from "@frost/shared/Components/ui/sonner";
import { TooltipProvider } from "@frost/shared/Components/ui/tooltip";
import OverlayFrame, { peerTabLabel } from "@frost/shared/Components/Overlay/OverlayFrame";
import Browse from "../Browse/Browse";
import Locker from "../Locker/Locker";
import ModDetail from "../ModDetail/ModDetail";
import Manage from "../Manage/Manage";
import Presets from "../Presets/Presets";
import { ThemeProvider } from "@frost/shared/Context/Theme";
import { useI18n, APP_NAME } from "@/i18n";
import { setAmbientVars, type TKey } from "@/i18n";
import { FrostmodProvider } from "../../Context/Frostmod";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { InstallProvider } from "../../Context/Install";
import { DownloadsProvider } from "../../Context/Downloads";
import { DropReviewProvider } from "../../Context/DropReview";
import { useModBrowsing } from "../../lib/useModBrowsing";
import { bikePreviewAvailable, listGames, getConfig, isConfigured } from "@frost/shared/api/mods";
import {
  getOverlayPeer,
  getOverlayState,
  initialOverlayTab,
  onOverlayPeer,
  onOverlayTab,
  overlayHandoff,
  overlayHide,
  overlayOpenMain,
  type OverlayPeer,
} from "@frost/shared/api/overlay";
import type { Config, GameCaps, GameInfo } from "@frost/shared/types";

/**
 * The in-game overlay: a compact, frameless panel drawn over MX Bikes and summoned by
 * a global hotkey (registered in `src-tauri/src/overlay.rs`).
 *
 * It reuses Presets, Locker, Browse and Manage unchanged rather than reimplementing them —
 * Presets and the Locker's model swaps are the two things that apply to a *running*
 * game via the loader re-run in `gameproc::refresh_look`, which is the whole reason
 * this window is worth having. When MXB Coach runs too, its tabs follow ours, and picking
 * one hands the screen to Coach's own overlay.
 */

type OverlayTab = "presets" | "locker" | "browse" | "manage";

/** `cap` gates a tab on the active game supporting it — same rule as the sidebar's nav,
 *  so the overlay can't offer a view the main window hides. */
const TABS: {
  id: OverlayTab;
  label: TKey;
  icon: typeof Home;
  cap?: keyof GameCaps;
}[] = [
  { id: "presets", label: "nav.presets", icon: Shirt },
  // The Locker is the 3D preview; GP Bikes' meshes need their own part bindings first.
  { id: "locker", label: "nav.locker", icon: Bike, cap: "viewer" },
  { id: "browse", label: "nav.browse", icon: Home },
  { id: "manage", label: "nav.manage", icon: SlidersHorizontal, cap: "manage" },
];

const isTab = (id: string | null): id is OverlayTab => TABS.some((t) => t.id === id);

export default function Overlay() {
  const { t } = useI18n();
  const [ready, setReady] = useState(false);
  const [config, setConfig] = useState<Config | null>(null);
  const [bikePreview, setBikePreview] = useState(false);
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [hotkey, setHotkey] = useState("");
  const [tab, setTab] = useState<OverlayTab>(() => {
    const first = initialOverlayTab();
    return isTab(first) ? first : "presets";
  });
  const [peer, setPeer] = useState<OverlayPeer | null>(null);

  const showBrowse = useCallback(() => setTab("browse"), []);
  const {
    modType,
    modTypes,
    changeType,
    listing,
    selectedSlug,
    selectedCategoryId,
    installed,
    onInstalled,
    openMod,
    openModTarget,
    closeMod,
  } = useModBrowsing(showBrowse, config?.activeGame);

  const reloadConfig = useCallback(async () => {
    setConfig(await getConfig());
  }, []);

  // The overlay is a separate window and doesn't fetch the game table; it only needs the
  // active title's capabilities to gate what it renders.
  const overlayGame =
    games.find((g) => g.id === (config?.activeGame ?? "mxb")) ?? MXB_FALLBACK;

  // Every translated string can say `{{app}}` / `{{game}}` / `{{site}}` instead of naming
  // the product or a title — see `setAmbientVars`. Set as a layout effect so the first paint after a
  // switch already reads right.
  useLayoutEffect(() => {
    setAmbientVars({ app: APP_NAME, game: overlayGame.display, site: overlayGame.catalogDomain });
  }, [overlayGame]);

  const tabs = useMemo(
    () => TABS.filter((t) => !t.cap || overlayGame.caps[t.cap]),
    [overlayGame],
  );

  // A tab this game doesn't offer can't stay selected — the overlay outlives a game
  // switch made in the main window, so the selection can go stale under it.
  useEffect(() => {
    if (!tabs.some((t) => t.id === tab)) setTab(tabs[0].id);
  }, [tabs, tab]);

  // MXB Coach's tabs, while it's linked, and the tab it hands us.
  useEffect(() => {
    getOverlayPeer().then(setPeer).catch(() => {});
    const offPeer = onOverlayPeer(setPeer);
    const offTab = onOverlayTab((id) => {
      if (isTab(id)) setTab(id);
    });
    return () => {
      void offPeer.then((f) => f());
      void offTab.then((f) => f());
    };
  }, []);

  const dismiss = useCallback(() => {
    void overlayHide().catch(() => {});
  }, []);

  // Rider, Library and Settings live only in the main window, so the overlay needs a
  // way out that doesn't drop the player back into the game to go find it.
  const openFullApp = useCallback(() => {
    void overlayOpenMain().catch(() => {});
  }, []);

  useEffect(() => {
    (async () => {
      try {
        bikePreviewAvailable().then(setBikePreview).catch(() => {});
        listGames()
          .then((g) => {
            if (g.length) setGames(g);
          })
          .catch(() => {});
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

  return (
    <ThemeProvider>
      <FrostmodProvider>
        <TooltipProvider delayDuration={300}>
          <OverlayFrame
            appName={APP_NAME}
            tabs={tabs.map(({ id, label, icon }) => ({ id, label: t(label), icon }))}
            active={tab}
            onTab={(id) => {
              if (isTab(id)) setTab(id);
              closeMod();
            }}
            peerName={peer?.app === "coach" ? "MXB Coach" : undefined}
            peerTabs={peer?.tabs.map((id) => ({ id, label: peerTabLabel(t, id) }))}
            onPeerTab={(id) => void overlayHandoff(id).catch(() => {})}
            hotkey={hotkey}
            onOpenMain={openFullApp}
            onClose={dismiss}
          >
            {ready &&
              (config ? (
                <ConfigContext.Provider
                  value={{
                    config,
                    reloadConfig,
                    bikePreview,
                    // The overlay has no game switcher — it shows one game's content,
                    // the one the main window is on — so a single-entry list is right.
                    games: [overlayGame],
                    game: overlayGame,
                    switchGame: async () => {},
                  }}
                >
                  {/* No Downloads page in here, but installs made mid-session still
                      belong in the history the main window shows. */}
                  <DownloadsProvider>
                  {/* Above the installer, as in the Dashboard: a download that turns
                      out to be a pack is handed to this sheet, so `InstallProvider`
                      has to be able to reach it here too. */}
                  <DropReviewProvider onInstalled={onInstalled}>
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
                          modTypes={modTypes}
                          listing={listing}
                          installed={installed}
                          onOpenMod={openMod}
                          onChangeType={changeType}
                        />
                      )}
                    </div>
                  </InstallProvider>
                  </DropReviewProvider>
                  </DownloadsProvider>
                </ConfigContext.Provider>
              ) : (
                // No config means the player never finished first-run setup. That
                // wizard belongs in the main window, not over a running game.
                <div className="grid flex-1 place-items-center px-8 text-center text-[13px] text-muted-foreground">
                  {t("overlay.needsSetup")}
                </div>
              ))}
          </OverlayFrame>
          <Toaster />
        </TooltipProvider>
      </FrostmodProvider>
    </ThemeProvider>
  );
}
