import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import Rail, { RailBrand, RailButton, type RailEntry } from "@frost/shared/Components/Shell/Rail";
import TitleBar from "@frost/shared/Components/Shell/TitleBar";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import Sessions from "./Components/Sessions/Sessions";
import Settings from "./Components/Settings/Settings";

type View = "sessions" | "settings";

function Shell() {
  const t = useT();
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [view, setView] = useState<View>("sessions");

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
            header={<RailBrand top="MXB" name="Coach" />}
            footer={
              <RailButton
                label={t("nav.settings")}
                on={view === "settings"}
                onClick={() => setView("settings")}
              />
            }
          />
          <main className="min-w-0 flex-1">
            {view === "settings" ? <Settings /> : <Sessions onSettings={() => setView("settings")} />}
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
        <Shell />
      </I18nProvider>
    </ThemeProvider>
  );
}
