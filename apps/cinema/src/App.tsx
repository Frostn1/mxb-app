import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import AppBar from "@frost/shared/Components/Shell/AppBar";
import SigninGate from "@frost/shared/Components/SigninGate/SigninGate";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import type { SessionSummary } from "@/api/cinema";
import Rides from "./Components/Rides";
import Settings from "./Components/Settings";
import Shot from "./Components/Shot";

type View = "rides" | "settings";

function Shell() {
  const t = useT();
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [view, setView] = useState<View>("rides");
  const [ride, setRide] = useState<SessionSummary | null>(null);
  // An export is running: the session and the page stay put until it ends.
  const [busy, setBusy] = useState(false);

  const reloadConfig = useCallback(async () => setConfig(await getConfig()), []);
  useEffect(() => {
    void reloadConfig().catch(() => {});
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
      // The game is chosen in MXB App; Cinema follows the config it wrote.
      switchGame: async () => {},
    }),
    [config, reloadConfig, games, game],
  );

  return (
    <ConfigContext.Provider value={ctx}>
      <div className="flex h-screen flex-col bg-background text-foreground">
        <AppBar
          name="MXB Cinema"
          tabs={[{ id: "rides", label: t("nav.rides") }]}
          active={view === "settings" ? undefined : view}
          onPick={(id) => {
            if (busy) return;
            setView(id as View);
            setRide(null);
          }}
          onSettings={() => !busy && setView("settings")}
          settingsActive={view === "settings"}
          settingsLabel={t("nav.settings")}
          windowLabels={{ minimize: t("window.minimize"), maximize: t("window.maximize"), close: t("window.close") }}
        />
        <main className="min-h-0 flex-1">
          {view === "settings" ? (
            <Settings />
          ) : ride ? (
            <Shot session={ride} onBack={() => !busy && setRide(null)} onBusy={setBusy} />
          ) : (
            <Rides onPick={setRide} />
          )}
        </main>
      </div>
      <ThemedToaster />
    </ConfigContext.Provider>
  );
}

function ThemedToaster() {
  const { resolved } = useTheme();
  return <Toaster position="bottom-right" theme={resolved} richColors />;
}

export default function App() {
  return (
    <ThemeProvider defaultTheme="system" scalable={false}>
      <I18nProvider>
        <Shell />
        {/* The Steam sign-in wall, shown only when the estate gate requires one. */}
        <SigninGate />
      </I18nProvider>
    </ThemeProvider>
  );
}
