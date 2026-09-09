import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, contentLockAvailable, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import Rail, { RailButton, type RailEntry } from "./Components/Shell/Rail";
import { ContextSlots } from "./Components/Shell/ContextBar";
import Settings from "./Components/Settings/Settings";
import Studio, { type StudioTab } from "./Components/Studio/Studio";
import Secure from "./Components/Secure/Secure";
import { TrackBuildProvider } from "./Context/TrackBuild";

/** The rail's own view space: the Studio's six tools, plus two screens of its own. */
type View = StudioTab | "secure" | "settings";

function Shell() {
  const t = useT();
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [view, setView] = useState<View>("designer");
  const [hasLock, setHasLock] = useState(false);
  const [left, setLeft] = useState<HTMLElement | null>(null);
  const [right, setRight] = useState<HTMLElement | null>(null);

  const reloadConfig = useCallback(async () => setConfig(await getConfig()), []);

  useEffect(() => {
    void reloadConfig();
    listGames().then(setGames).catch(() => {});
    contentLockAvailable().then(setHasLock).catch(() => {});
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
      bikePreview: true,
      games,
      game,
      // One title at a time, chosen in the mod manager: the studio follows the config it
      // wrote rather than offering a second switch that could disagree with it.
      switchGame: async () => {},
    }),
    [config, reloadConfig, games, game],
  );
  const slots = useMemo(() => ({ left, right }), [left, right]);

  // The rider rig is MX Bikes only — GP Bikes' has no part bindings — and locking needs the
  // optional local module. A tool that could only ever fail is not offered.
  const entries = useMemo(() => {
    const all: (RailEntry<View> & { when?: boolean })[] = [
      { id: "designer", label: t("nav.designer") },
      { id: "paints", label: t("nav.paints") },
      { id: "rider", label: t("nav.rider"), when: game.caps.viewer },
      { id: "pose", label: t("nav.pose"), when: game.caps.viewer },
      { id: "track", label: t("nav.track") },
      { id: "protect", label: t("nav.protect"), when: hasLock },
      { id: "secure", label: t("nav.secure"), when: hasLock },
    ];
    return all.filter((e) => e.when !== false);
  }, [t, game.caps.viewer, hasLock]);

  useEffect(() => {
    if (!entries.some((e) => e.id === view) && view !== "settings") setView("designer");
  }, [entries, view]);

  const title =
    view === "settings"
      ? t("nav.settings")
      : (entries.find((e) => e.id === view)?.label ?? "");


  return (
    <ConfigContext.Provider value={ctx}>
      <TrackBuildProvider>
        <ContextSlots.Provider value={slots}>
          <div className="flex h-screen bg-background text-foreground">
            <Rail
              entries={entries}
              active={view}
              onPick={setView}
              header={
                <div data-tauri-drag-region className="flex select-none items-center gap-2 px-2.5 pt-1">
                  {/* The app's own mark, not a lettered plate — the same two-paint snowflake
                      the icon and the installer carry. */}
                  <img src="/logo.svg" alt="" className="size-[18px]" draggable={false} />
                  <span className="text-[12.5px] font-semibold tracking-[0.01em] text-foreground">
                    Studio
                  </span>
                </div>
              }
              footer={
                <RailButton
                  label={t("nav.settings")}
                  on={view === "settings"}
                  onClick={() => setView("settings")}
                />
              }
            />

            <div className="flex min-w-0 flex-1 flex-col">
              {/* One strip the mounted tool fills from both ends, rather than a second row of
                  chrome. Draggable, since the rail is the only other place to grab. It is
                  taller than the manager's context bar on purpose: this is the only chrome
                  the Studio has, so it can afford to breathe. */}
              <div
                data-tauri-drag-region
                className="flex h-[52px] shrink-0 items-center gap-3 border-b border-border px-4"
              >
                <span className="shrink-0 select-none text-[13px] font-semibold tracking-[0.01em] text-foreground">
                  {title}
                </span>
                <div ref={setLeft} className="flex min-w-0 flex-1 items-center gap-2" />
                <div ref={setRight} className="flex shrink-0 items-center gap-2" />
              </div>

              <div className="min-h-0 flex-1 bg-canvas">
                {view === "settings" ? (
                  <Settings />
                ) : view === "secure" ? (
                  <Secure />
                ) : (
                  <Studio
                    tab={view as StudioTab}
                    onTab={(tb) => setView(tb)}
                    riderPreset={null}
                    riderBike={null}
                    onRiderPresetLoaded={() => {}}
                  />
                )}
              </div>
            </div>
          </div>
          <Toaster position="bottom-right" theme="light" richColors />
        </ContextSlots.Provider>
      </TrackBuildProvider>
    </ConfigContext.Provider>
  );
}

export default function App() {
  return (
    <I18nProvider>
      <Shell />
    </I18nProvider>
  );
}
