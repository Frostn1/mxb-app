import { useCallback, useEffect, useState } from "react";
import TitleBar from "./Components/TitleBar/TitleBar";
import Dashboard from "./Components/Dashboard/Dashboard";
import Setup from "./Components/Setup/Setup";
import Welcome from "./Components/Welcome/Welcome";
import LooseSwapPrompt from "./Components/Locker/LooseSwapPrompt";
import { ThemeProvider } from "./Context/Theme";
import { FrostmodProvider } from "./Context/Frostmod";
import { ConfigContext, MXB_FALLBACK } from "./Context/Config";
import { toast } from "sonner";
import { Toaster } from "@/Components/ui/sonner";
import { TooltipProvider } from "@/Components/ui/tooltip";
import {
  bikePreviewAvailable,
  getConfig,
  isConfigured,
  listGames,
  onOverlayFullscreenBlocked,
  setActiveGame,
  setIntroSeen,
} from "./api/mods";
import { TOUR_DONE_KEY } from "./Components/Tour/Tour";
import { useI18n } from "./i18n/context";
import { UpdateProvider } from "./Context/Update";
import UpdateBanner from "./Components/UpdateBanner/UpdateBanner";
import type { Config, GameId, GameInfo } from "./types";

/**
 * Bumped when the intro tour changes enough to warrant showing it again.
 *
 * The saved config (`welcomeSeen`) is the real record — this only covers the window
 * before one exists, i.e. while the setup screen is still up. Keeping it in the
 * webview's storage alone is what made the intro replay after that storage was
 * cleared.
 */
const WELCOME_SEEN_KEY = "mxb:welcomeSeen:v1";

const App = () => {
  const { t } = useI18n();
  const [ready, setReady] = useState(false);
  const [config, setConfig] = useState<Config | null>(null);
  // Whether this build can decode real bike geometry (optional local module). Fixed
  // at build time, so fetch it once at startup.
  const [bikePreview, setBikePreview] = useState(false);
  // Static per build, so fetched once at startup alongside `bikePreview`.
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [welcomeDismissed, setWelcomeDismissed] = useState(
    () => localStorage.getItem(WELCOME_SEEN_KEY) === "1",
  );
  const showWelcome = !welcomeDismissed && !config?.welcomeSeen;

  const dismissWelcome = useCallback(() => {
    localStorage.setItem(WELCOME_SEEN_KEY, "1");
    setWelcomeDismissed(true);
    // No-ops when there's no config yet (dismissed from over the setup screen); the
    // flag above holds until setup writes one, and the effect below persists it then.
    void setIntroSeen({ welcome: true }).catch(() => {});
  }, []);

  const reloadConfig = useCallback(async () => {
    setConfig(await getConfig());
  }, []);

  const switchGame = useCallback(async (id: GameId) => {
    // The backend returns the resulting config, so a game whose folders it couldn't
    // detect comes back with a blank `modsPath` — which renders the setup screen,
    // exactly as it would on a first run.
    setConfig(await setActiveGame(id));
  }, []);

  const activeGame =
    games.find((g) => g.id === (config?.activeGame ?? "mxb")) ?? MXB_FALLBACK;

  // Carry the old webview-only flags into the config once, so an existing install
  // doesn't get shown the intro again the first time that storage is lost.
  useEffect(() => {
    if (!config) return;
    const welcome =
      !config.welcomeSeen && localStorage.getItem(WELCOME_SEEN_KEY) === "1";
    const tour = !config.tourDone && localStorage.getItem(TOUR_DONE_KEY) === "1";
    if (welcome || tour) void setIntroSeen({ welcome, tour }).catch(() => {});
  }, [config]);

  useEffect(() => {
    (async () => {
      try {
        bikePreviewAvailable().then(setBikePreview).catch(() => {});
        listGames()
          .then((g) => {
            if (g.length) setGames(g);
          })
          .catch(() => {});
        if (await isConfigured()) await reloadConfig();
      } catch (err) {
        console.error("Startup failed", err);
      } finally {
        setReady(true);
      }
    })();
  }, [reloadConfig]);

  // The overlay can't draw over a game in exclusive fullscreen, and it can't say so
  // itself — it's behind the game. The message waits here, where the player lands the
  // moment they alt-tab to find out why nothing happened.
  useEffect(() => {
    const unlisten = onOverlayFullscreenBlocked(() =>
      toast.warning(t("overlay.fullscreenBlocked"), {
        description: t("overlay.fullscreenBlockedDesc"),
      }),
    );
    return () => {
      void unlisten.then((off) => off()).catch(() => {});
    };
  }, [t]);

  // Block the webview's browser refresh/find shortcuts.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (
        (e.ctrlKey && (e.code === "KeyF" || e.code === "KeyR")) ||
        e.code === "F5"
      ) {
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  return (
    <ThemeProvider>
      <FrostmodProvider>
        <TooltipProvider delayDuration={300}>
          <UpdateProvider>
            {/* Flexbox column (not grid rows) so WKWebView on macOS gives `main`
                its full height — WebKit won't resolve a child's `height:100%`
                against a `1fr` grid track, which collapsed the app to content
                height when a view's content was short. */}
            <div className="flex h-screen flex-col overflow-hidden">
              <div className="h-[42px] flex-none">
                <TitleBar />
              </div>
              <UpdateBanner />
              <main className="flex min-h-0 flex-1 flex-col overflow-hidden bg-background text-foreground">
                {ready && (
                  // The provider wraps *both* branches, not just the dashboard: the setup
                  // screen carries the game switcher, and someone who switched to a game
                  // they haven't installed needs a way back without restarting the app.
                  <ConfigContext.Provider
                    value={{
                      config: config ?? { modsPath: "" },
                      reloadConfig,
                      bikePreview,
                      games,
                      game: activeGame,
                      switchGame,
                    }}
                  >
                    {/* A config with no mods folder means setup hasn't finished — either a
                        first run, or a switch to a game we couldn't locate.

                        The dashboard is keyed by game: switching titles changes every
                        folder it reads, and its views fetch on mount. Remounting is what
                        makes "switch game" mean "start over here" rather than leaving the
                        previous game's library and Manage list on screen. */}
                    {config?.modsPath ? (
                      <Dashboard key={activeGame.id} welcomeActive={showWelcome} />
                    ) : (
                      <Setup onComplete={reloadConfig} game={activeGame} />
                    )}
                  </ConfigContext.Provider>
                )}
              </main>
            </div>
            <Toaster />
            {ready && showWelcome && <Welcome onDone={dismissWelcome} />}
            {/* Offer to register loose model-swap folders once the app is set up and the
                intro tour is out of the way. */}
            {ready && config && !showWelcome && <LooseSwapPrompt />}
          </UpdateProvider>
        </TooltipProvider>
      </FrostmodProvider>
    </ThemeProvider>
  );
};

export default App;
