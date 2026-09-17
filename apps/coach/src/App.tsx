import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import Rail, { RailButton, type RailEntry } from "@frost/shared/Components/Shell/Rail";
import TitleBar from "@frost/shared/Components/Shell/TitleBar";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import Sessions from "./Components/Sessions/Sessions";
import Settings from "./Components/Settings/Settings";
import UpdateBanner from "./Components/UpdateBanner";
import { track } from "@/lib/analytics";
import { UpdateProvider } from "./Context/Update";
import SigninGate from "@frost/shared/Components/SigninGate/SigninGate";

type View = "sessions" | "settings";

/** The mxbsecure wordmark over the product name, as the site sets them. Grabs the window. */
function Brand() {
  return (
    <div data-tauri-drag-region className="select-none px-2.5 pt-0.5">
      <div data-tauri-drag-region className="headline text-[12px] text-muted-foreground">
        mxbsecure
      </div>
      <div data-tauri-drag-region className="headline mt-0.5 text-[22px]">
        Coach
      </div>
    </div>
  );
}

function Shell() {
  const t = useT();
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [view, setView] = useState<View>("sessions");
  // Which page is open. Derived and counted by an effect rather than inside the rail's handler,
  // for the reason the manager does the same: plenty of things move the view without going
  // through it. `view.settings` is deliberately the manager's name — it is the same page.
  useEffect(() => {
    track(`view.${view}`);
  }, [view]);

  const reloadConfig = useCallback(async () => setConfig(await getConfig()), []);

  useEffect(() => {
    void reloadConfig();
    listGames().then(setGames).catch(() => {});
    void appPlatform().catch(() => {});
  }, [reloadConfig]);

  const game = useMemo(
    () => games.find((g) => g.id === config.activeGame) ?? games[0] ?? MXB_FALLBACK,
    [games, config.activeGame],
  );
  useEffect(() => setAmbientVars({ game: game.display }), [game.display]);

  const ctx = useMemo(
    () => ({
      config,
      reloadConfig,
      bikePreview: false,
      games,
      game,
      // The game is chosen in MXB App; the coach follows the config it wrote.
      switchGame: async () => {},
    }),
    [config, reloadConfig, games, game],
  );

  const entries: RailEntry<View>[] = [{ id: "sessions", label: t("nav.sessions") }];

  return (
    <ConfigContext.Provider value={ctx}>
      <div className="flex h-screen flex-col bg-background text-foreground">
        <TitleBar />
        <div className="flex min-h-0 flex-1">
          <Rail
            entries={entries}
            active={view}
            onPick={setView}
            header={<Brand />}
            footer={
              <RailButton
                label={t("nav.settings")}
                on={view === "settings"}
                onClick={() => setView("settings")}
              />
            }
          />
          <main className="flex min-w-0 flex-1 flex-col">
            <UpdateBanner />
            <div className="min-h-0 flex-1">
              {view === "settings" ? <Settings /> : <Sessions onSettings={() => setView("settings")} />}
            </div>
          </main>
        </div>
      </div>
      <ThemedToaster />
    </ConfigContext.Provider>
  );
}

/** Toasts in whichever theme the app is in. */
function ThemedToaster() {
  const { resolved } = useTheme();
  return <Toaster position="bottom-right" theme={resolved} richColors />;
}

export default function App() {
  return (
    <ThemeProvider defaultTheme="dark" scalable={false}>
      <I18nProvider>
        <UpdateProvider>
          <Shell />
          {/* The Steam sign-in wall, shown only when the estate gate requires one. */}
          <SigninGate />
        </UpdateProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}
