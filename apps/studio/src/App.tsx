import { useCallback, useEffect, useMemo, useState } from "react";
import { Toaster } from "sonner";
import { appPlatform, contentLockAvailable, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { cn } from "@frost/shared/lib/utils";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import Rail, { RailButton, type RailEntry } from "./Components/Shell/Rail";
import { ContextSlots, ShellChrome } from "./Components/Shell/ContextBar";
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
  // A tool can say it is showing something that owns the window — the Designer's start
  // screen — and the strip goes with it rather than sitting above it with nothing in it.
  const [bare, setBare] = useState(false);

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
  const chrome = useMemo(() => ({ bare, setBare }), [bare]);

  // The rider rig is MX Bikes only — GP Bikes' has no part bindings — and locking needs the
  // optional local module. A tool that could only ever fail is not offered.
  const entries = useMemo(() => {
    const all: (RailEntry<View> & { when?: boolean })[] = [
      // Two errands, not one list of seven: making something, and locking something you
      // have already made.
      { id: "designer", label: t("nav.designer"), group: "make" },
      { id: "paints", label: t("nav.paints"), group: "make" },
      { id: "rider", label: t("nav.rider"), group: "make", when: game.caps.viewer },
      { id: "pose", label: t("nav.pose"), group: "make", when: game.caps.viewer },
      { id: "track", label: t("nav.track"), group: "make" },
      { id: "protect", label: t("nav.protect"), group: "sell", when: hasLock },
      { id: "secure", label: t("nav.secure"), group: "sell", when: hasLock },
    ];
    return all.filter((e) => e.when !== false);
  }, [t, game.caps.viewer, hasLock]);

  useEffect(() => {
    if (!entries.some((e) => e.id === view) && view !== "settings") setView("designer");
  }, [entries, view]);



  return (
    <ConfigContext.Provider value={ctx}>
      <TrackBuildProvider>
        <ContextSlots.Provider value={slots}>
        <ShellChrome.Provider value={chrome}>
          <div className="flex h-screen bg-background text-foreground">
            <Rail
              entries={entries}
              active={view}
              onPick={setView}
              header={
                <div
                  data-tauri-drag-region
                  className="flex select-none items-center gap-2.5 px-2.5 pt-0.5"
                >
                  {/* The app's own mark, not a lettered plate — the same two-paint snowflake
                      the icon and the installer carry. */}
                  <img
                    src="/logo.svg"
                    alt=""
                    draggable={false}
                    className="size-[28px] flex-none [filter:drop-shadow(0_1px_3px_rgba(0,0,0,0.25))]"
                  />
                  {/* Two lines set as a logotype rather than as a stack: the possessive in
                      the only face in the app that is not Barlow, and the name stepped in
                      under it so the two overlap. Reads as one mark, not two labels. */}
                  <span className="flex min-w-0 flex-col items-start">
                    <span className="font-serif text-[15px] italic leading-none text-muted-foreground">
                      Frost&apos;s
                    </span>
                    <span className="-mt-[5px] ml-[15px] font-serif text-[19px] font-bold italic leading-none tracking-[-0.01em] text-foreground">
                      Studio
                    </span>
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
                className={cn(
                  "relative flex h-[52px] shrink-0 items-center gap-3 border-b border-border px-4",
                  bare && "hidden",
                )}
              >
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
        </ShellChrome.Provider>
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
