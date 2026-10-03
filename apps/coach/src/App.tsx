import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import AppBar from "@frost/shared/Components/Shell/AppBar";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import Sessions from "./Components/Sessions/Sessions";
import Settings, { type SectionId } from "./Components/Settings/Settings";
import UpdateBanner from "./Components/UpdateBanner";
import CueKeeper from "./CueKeeper";
import { track } from "@/lib/analytics";
import { UpdateProvider } from "./Context/Update";
import SigninGate from "@frost/shared/Components/SigninGate/SigninGate";
import SurveyPrompt from "@frost/shared/Components/Survey/SurveyPrompt";

type View = "sessions" | "settings";

function Shell() {
  const t = useT();
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [view, setView] = useState<View>("sessions");
  const [settingsAt, setSettingsAt] = useState<SectionId>("general");
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

  // Top-level views as tabs; Settings is the gear, not a tab.
  const tabs = [{ id: "sessions", label: t("nav.sessions") }];

  return (
    <ConfigContext.Provider value={ctx}>
      <div className="flex h-screen flex-col bg-background text-foreground">
        <AppBar
          name="MXB Coach"
          tabs={tabs}
          active={view === "settings" ? undefined : view}
          onPick={(id) => setView(id as View)}
          onSettings={() => {
            setSettingsAt("general");
            setView("settings");
          }}
          settingsActive={view === "settings"}
          settingsLabel={t("nav.settings")}
          windowLabels={{
            minimize: t("window.minimize"),
            maximize: t("window.maximize"),
            close: t("window.close"),
          }}
        />
        <div className="flex min-h-0 flex-1">
          <main className="flex min-w-0 flex-1 flex-col">
            <UpdateBanner />
            <div className="min-h-0 flex-1">
              {view === "settings" ? (
                <Settings key={settingsAt} initialSection={settingsAt} />
              ) : (
                <Sessions
                  onSettings={() => {
                    // Only ever asked for when the recorder is missing, so it opens there.
                    setSettingsAt("recording");
                    setView("settings");
                  }}
                />
              )}
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
    <ThemeProvider defaultTheme="system" scalable={false}>
      <I18nProvider>
        <UpdateProvider>
          <Shell />
          {/* Keeps the cue sheet following the rider's laps wherever they are in the app.
              It used to live inside the Live cues panel, so it only ran while that panel was
              on screen — which is why the cues never changed. */}
          <CueKeeper />
          {/* The Steam sign-in wall, shown only when the estate gate requires one. */}
          <SigninGate />
          {/* The survey prompt. Rare, and on its own schedule — see `crates/core/src/survey.rs`. */}
          <SurveyPrompt />
        </UpdateProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}
