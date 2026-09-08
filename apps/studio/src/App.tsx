import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { I18nProvider, setAmbientVars } from "@/i18n";
import { ContextSlots } from "./Components/Shell/ContextBar";
import Studio, { type StudioTab } from "./Components/Studio/Studio";
import { TrackBuildProvider } from "./Context/TrackBuild";

/**
 * Frost's Studio.
 *
 * Flatter than the manager's shell on purpose: there is one screen here, and its sub-views
 * are the tools. No rail of places, because every place is the same place.
 */
export default function App() {
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [tab, setTab] = useState<StudioTab>("designer");
  const [left, setLeft] = useState<HTMLElement | null>(null);
  const [right, setRight] = useState<HTMLElement | null>(null);

  const reloadConfig = useCallback(async () => {
    setConfig(await getConfig());
  }, []);

  useEffect(() => {
    void reloadConfig();
    listGames().then(setGames).catch(() => {});
    // `{{game}}` is in strings the studio shows too, and the overlay's lesson applies here:
    // seed it where the value is known rather than waiting for a provider.
    appPlatform().catch(() => {});
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
      bikePreview: true,
      games,
      game,
      // One title at a time, chosen in the manager: the studio follows the config rather
      // than offering a switch of its own, so the two cannot disagree about which game's
      // folders are live.
      switchGame: async () => {},
    }),
    [config, reloadConfig, games, game],
  );

  const slots = useMemo(() => ({ left, right }), [left, right]);

  return (
    <I18nProvider>
      <ConfigContext.Provider value={ctx}>
        <TrackBuildProvider>
          <ContextSlots.Provider value={slots}>
            <div className="flex h-screen flex-col bg-background text-foreground">
              <div className="flex h-[46px] shrink-0 items-center gap-4 border-b border-border px-4">
                <div className="flex select-none items-center">
                  <span className="u-skew grid h-[26px] place-items-center bg-primary px-2">
                    <span className="u-unskew font-cond text-[16px] font-bold leading-none tracking-[0.04em] text-primary-foreground">
                      FROST
                    </span>
                  </span>
                  <span className="ml-[9px] font-cond text-[16px] font-semibold leading-none tracking-[0.08em] text-muted-foreground">
                    Studio
                  </span>
                </div>
                <div ref={setLeft} className="flex flex-1 items-center gap-5" />
                <div ref={setRight} className="flex items-center gap-2" />
              </div>
              <div className="min-h-0 flex-1">
                <Studio
                  tab={tab}
                  onTab={setTab}
                  riderPreset={null}
                  riderBike={null}
                  onRiderPresetLoaded={() => {}}
                />
              </div>
            </div>
            <Toaster position="bottom-right" theme="dark" richColors />
          </ContextSlots.Provider>
        </TrackBuildProvider>
      </ConfigContext.Provider>
    </I18nProvider>
  );
}
