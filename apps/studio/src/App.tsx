import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Toaster } from "sonner";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@frost/shared/Components/ui/alert-dialog";
import { Button } from "@frost/shared/Components/ui/button";
import { appPlatform, getConfig, listGames } from "@frost/shared/api/mods";
import type { Config, GameInfo } from "@frost/shared/types";
import { cn } from "@frost/shared/lib/utils";
import { ConfigContext, MXB_FALLBACK } from "@frost/shared/Context/Config";
import { ThemeProvider, useTheme } from "@frost/shared/Context/Theme";
import { I18nProvider, setAmbientVars, useT } from "@/i18n";
import Rail, {
  RailBrand,
  RailButton,
  type RailEntry,
} from "@frost/shared/Components/Shell/Rail";
import TitleBar from "./Components/Shell/TitleBar";
import {
  ContextSlots,
  ShellChrome,
  UnsavedRegistry,
  type UnsavedWork,
} from "./Components/Shell/ContextBar";
import Settings from "./Components/Settings/Settings";
import Studio, { type StudioTab } from "./Components/Studio/Studio";
import { track } from "./lib/analytics";
import { TrackBuildProvider } from "./Context/TrackBuild";
import { UpdateProvider } from "./Context/Update";
import UpdateBanner from "./Components/Shell/UpdateBanner";
import SigninGate from "@frost/shared/Components/SigninGate/SigninGate";
import SurveyPrompt from "@frost/shared/Components/Survey/SurveyPrompt";
import { parsePluginView, usePlugins } from "@frost/shared/lib/usePlugins";
import { initialView } from "@frost/shared/api/mods";
import { toast } from "sonner";

/**
 * The rail's own view space: the Studio's tools, Settings, and whatever a plugin adds.
 *
 * The template literal is how a paid plugin gets a rail row without this union naming it:
 * its panels are addressed `plugin:<plugin id>/<panel id>`, exactly as they were in the mod
 * manager before the panels moved here.
 */
type View = StudioTab | "settings" | `plugin:${string}`;

function Shell() {
  const t = useT();
  const [config, setConfig] = useState<Config>({ modsPath: "" });
  const [games, setGames] = useState<GameInfo[]>([MXB_FALLBACK]);
  const [view, setView] = useState<View>("designer");
  // Paid plugins whose panels belong here — the Replay Mod, and anything else bought in MXB
  // App. A plugin that fails to mount says so once and is then dropped: a broken add-on must
  // not take the Studio down with the work somebody has open in it.
  const plugins = usePlugins("studio", (id, message) => toast.error(`${id}: ${message}`));
  // The panel on screen, when the view addresses one. A view naming a plugin that is no
  // longer mounted — a licence that lapsed mid-session — falls through to the built-in tools
  // rather than rendering an empty frame.
  const pluginPanel = (() => {
    const ref = parsePluginView(view);
    if (!ref) return null;
    const p = plugins.find((x) => x.manifest.id === ref.plugin);
    return p?.panels.find((panel) => panel.id === ref.panel) ?? null;
  })();
  // Which tool a creator actually opens. A name and nothing else — see `lib/analytics.ts`.
  useEffect(() => {
    // One bucket for every plugin panel: naming each one would be a counter per thing
    // somebody else shipped, which is unbounded.
    track(view.startsWith("plugin:") ? "view.plugin" : `view.studio.${view}`);
  }, [view]);
  const [left, setLeft] = useState<HTMLElement | null>(null);
  const [right, setRight] = useState<HTMLElement | null>(null);
  // A tool can say it is showing something that owns the window — the Designer's start
  // screen — and the strip goes with it rather than sitting above it with nothing in it.
  const [bare, setBare] = useState(false);
  // What the mounted tools are holding, and how to save it — see `UnsavedRegistry`.
  const work = useRef(new Set<UnsavedWork>());
  const registry = useMemo(
    () => ({
      register: (w: UnsavedWork) => {
        work.current.add(w);
        return () => void work.current.delete(w);
      },
    }),
    [],
  );
  const [asking, setAsking] = useState(false);
  const [saving, setSaving] = useState(false);

  /**
   * Closing the window asks first, once, if there is anything to lose.
   *
   * `close()` from inside the handler would re-enter it, so the answer is remembered and the
   * second pass is let through — the flag is only ever set immediately before closing.
   */
  const leaving = useRef(false);
  useEffect(() => {
    const win = getCurrentWindow();
    const un = win.onCloseRequested((e) => {
      if (leaving.current || ![...work.current].some((w) => w.dirty())) return;
      e.preventDefault();
      setAsking(true);
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const leave = useCallback(async () => {
    leaving.current = true;
    setAsking(false);
    await getCurrentWindow().close();
  }, []);

  const saveAndLeave = useCallback(async () => {
    setSaving(true);
    let ok = true;
    for (const w of work.current) {
      if (!w.dirty()) continue;
      ok = await w.save().catch(() => false);
      if (!ok) break;
    }
    setSaving(false);
    // False means it could not save — usually because it needs a name and has just put the
    // cursor there. Dropping the close is the only sane answer; quitting anyway loses the
    // work the dialog exists to protect.
    if (ok) await leave();
    else setAsking(false);
  }, [leave]);

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

  // The rider rig is MX Bikes only — GP Bikes' has no part bindings. A tool that could only
  // ever fail is not offered.
  const entries = useMemo(() => {
    const all: (RailEntry<View> & { when?: boolean })[] = [
      // Two errands: making something, and checking something already on disk.
      { id: "designer", label: t("nav.designer"), group: "make" },
      // The Paints tab is retired from the rail — the Designer editor covers the same job better.
      // Its tool (PaintStudio) is kept dormant behind the scenes, so the entry can come back.
      { id: "rider", label: t("nav.rider"), group: "make", when: game.caps.viewer },
      { id: "pose", label: t("nav.pose"), group: "make", when: game.caps.viewer },
      { id: "track", label: t("nav.track"), group: "make" },
      // Recording a replay needs an in-game mod, and the Replay Mod is an MX Bikes plugin
      // like FrostMod — so it is gated on the same capability rather than on the game's id.
      { id: "replay", label: t("nav.replay"), group: "make", when: game.caps.frostmod },
      { id: "diagnose", label: t("nav.diagnose"), group: "check" },
      // Whatever a paid plugin contributes, under its own name — the label comes from the
      // plugin, so it is the one string in this rail the app cannot translate.
      ...plugins.flatMap((p) =>
        p.panels.map((panel) => ({
          id: `plugin:${p.manifest.id}/${panel.id}` as View,
          label: panel.label,
          group: "make",
        })),
      ),
    ];
    return all.filter((e) => e.when !== false);
  }, [t, game.caps.viewer, game.caps.frostmod, plugins]);

  useEffect(() => {
    if (!entries.some((e) => e.id === view) && view !== "settings") setView("designer");
  }, [entries, view]);

  /**
   * Open where we were asked to, once.
   *
   * MXB App starts the Studio with `--view replay` when somebody presses **Open in Studio**
   * on its Plugins page. Held in state rather than a ref so the second effect runs again when
   * either half lands: the flag arrives a round trip after the window opens, and a plugin's
   * row only exists once the plugin has mounted, which is later still. A name that never
   * appears — a screen this title doesn't have — is dropped quietly, because a stale flag is
   * not worth an error.
   */
  const [asked, setAsked] = useState<string | null>(null);
  useEffect(() => {
    void initialView()
      .then((v) => setAsked(v))
      .catch(() => {});
  }, []);
  useEffect(() => {
    if (!asked) return;
    // A screen's own name, or a plugin's id — the manager knows which plugin somebody pressed
    // the button for, not which panels it turned out to register, so an id resolves to the
    // first panel the plugin put on the rail.
    const panel = plugins.find((p) => p.manifest.id === asked)?.panels[0];
    const target = entries.some((e) => e.id === asked)
      ? asked
      : panel
        ? `plugin:${asked}/${panel.id}`
        : asked === "settings"
          ? "settings"
          : null;
    if (!target) return;
    setAsked(null);
    setView(target as View);
  }, [asked, entries, plugins]);



  return (
    <ConfigContext.Provider value={ctx}>
      <TrackBuildProvider>
        <ContextSlots.Provider value={slots}>
        <ShellChrome.Provider value={chrome}>
        <UnsavedRegistry.Provider value={registry}>
          <div className="flex h-screen flex-col bg-background text-foreground">
            <TitleBar />
            <div className="flex min-h-0 flex-1">
            <Rail
              entries={entries}
              active={view}
              onPick={setView}
              header={<RailBrand top="Frost's" name="Studio" />}
              footer={
                <RailButton
                  label={t("nav.settings")}
                  on={view === "settings"}
                  onClick={() => setView("settings")}
                />
              }
            />

            <div className="flex min-w-0 flex-1 flex-col">
              {/* Above the strip, so a tool that hides the strip doesn't hide it too. */}
              <UpdateBanner />
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
                ) : pluginPanel ? (
                  <pluginPanel.component />
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
          </div>
          <ThemedToaster />

          <AlertDialog open={asking} onOpenChange={(o) => !o && setAsking(false)}>
            <AlertDialogContent>
              <AlertDialogHeader>
                <AlertDialogTitle>{t("quit.title")}</AlertDialogTitle>
                <AlertDialogDescription>{t("quit.body")}</AlertDialogDescription>
              </AlertDialogHeader>
              <AlertDialogFooter>
                <AlertDialogCancel disabled={saving}>{t("common.cancel")}</AlertDialogCancel>
                {/* `size="sm"` on both: `AlertDialogCancel` and `AlertDialogAction` are
                    `buttonVariants({ size: "sm" })`, so a plain `Button` beside them stands
                    a row of h-9 next to an h-8 and reads as a mistake. */}
                <Button variant="outline" size="sm" disabled={saving} onClick={() => void leave()}>
                  {t("quit.discard")}
                </Button>
                <Button size="sm" disabled={saving} onClick={() => void saveAndLeave()}>
                  {t("quit.save")}
                </Button>
              </AlertDialogFooter>
            </AlertDialogContent>
          </AlertDialog>
        </UnsavedRegistry.Provider>
        </ShellChrome.Provider>
        </ContextSlots.Provider>
      </TrackBuildProvider>
    </ConfigContext.Provider>
  );
}

/** Toasts in whichever theme the app is in, rather than a light card on a dark window. */
function ThemedToaster() {
  const { resolved } = useTheme();
  return <Toaster position="bottom-right" theme={resolved} richColors />;
}

export default function App() {
  return (
    // Light unless the user picks otherwise — the reason is in `studio.css`. No interface
    // scale here, so the provider leaves the webview's zoom alone.
    <ThemeProvider defaultTheme="light" scalable={false}>
      <I18nProvider>
        <UpdateProvider>
          <Shell />
          {/* The Steam sign-in wall, shown only when the estate gate requires one. */}
          <SigninGate />
          {/* The survey prompt. Rare, and on its own schedule — see `crates/core/src/survey.rs`. */}
          <SurveyPrompt />
        </UpdateProvider>
      </I18nProvider>
    </ThemeProvider>
  );
}
