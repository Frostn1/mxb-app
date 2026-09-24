import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { toast } from "sonner";
import TopRail from "../Shell/TopRail";
import { ContextSlots } from "../Shell/ContextBar";
import PurchaseWatcher from "../Shell/PurchaseWatcher";
import { isModsView, type DashboardView, type ModsView } from "../Shell/nav";
import RetainedView from "../Shell/RetainedView";
import { parsePluginView, usePlugins } from "@frost/shared/lib/usePlugins";
import Servers from "../Servers/Servers";
import DropZone from "../Dropzone/DropZone";
import RuntimeBanner from "../RuntimeBanner/RuntimeBanner";
import UpdateBanner from "../UpdateBanner/UpdateBanner";
import SecurePrompt from "./SecurePrompt";
import GameIntegrationConsent from "../GameIntegration/GameIntegrationConsent";
import type { SectionId } from "../Settings/Settings";
import { LoadingMark } from "../Shell/LoadingMark";
import Tour, { TourContext, TOUR_DONE_KEY } from "../Tour/Tour";
import GetStarted from "../GetStarted/GetStarted";
import ReleaseShowcase from "../Showcase/ReleaseShowcase";
import { useReleaseShowcase } from "../Showcase/useReleaseShowcase";
import { InstallProvider } from "../../Context/Install";
import { DownloadsProvider } from "../../Context/Downloads";
import { DropReviewProvider } from "../../Context/DropReview";
import { ShareProvider } from "../../Context/Share";
import { useConfig } from "@frost/shared/Context/Config";
import { useT } from "@/i18n";
import { modTypesFor, onDeepLink, setIntroSeen } from "@frost/shared/api/mods";
import { useModBrowsing } from "../../lib/useModBrowsing";
import { displayName } from "@frost/shared/lib/mods";
import { track } from "../../lib/analytics";
import type { DownloadRecord } from "@frost/shared/types";
import { useFrostmod } from "../../Context/FrostmodContext";
import { invoke } from "@tauri-apps/api/core";
import { Mxbmrp3Prompt } from "../Mxbmrp3/Mxbmrp3Suggestion";

const Mods = lazy(() => import("../Mods/Mods"));
const ModDetail = lazy(() => import("../ModDetail/ModDetail"));
const Ranked = lazy(() => import("../Ranked/Ranked"));
const Library = lazy(() => import("../Library/Library"));
const Downloads = lazy(() => import("../Downloads/Downloads"));
const Locker = lazy(() => import("../Locker/Locker"));
const Presets = lazy(() => import("../Presets/Presets"));
const Manage = lazy(() => import("../Manage/Manage"));
const Settings = lazy(() => import("../Settings/Settings"));

interface DashboardProps {
  /** True while the Welcome slideshow is still up. The tour waits for it to close
   *  so its spotlights land on visible UI, not behind the overlay. */
  welcomeActive?: boolean;
}

const Dashboard = ({ welcomeActive = false }: DashboardProps) => {
  const { config, game, games } = useConfig();
  const { integrationChoice } = useFrostmod();
  const t = useT();
  // Opens on Online. Riding with other people is what the app is opened for most often, and
  // the server list is the one screen that is worth nothing five minutes later — a mod list
  // is the same whenever you get to it.
  const [view, setView] = useState<DashboardView>("servers");

  const showBrowse = useCallback(() => setView("browse"), []);
  const {
    modType,
    modTypes,
    changeType,
    listing,
    selectedSlug,
    selectedCategoryId,
    installed,
    libraryVersion,
    onInstalled,
    openMod,
    openModTarget,
    closeMod,
  } = useModBrowsing(showBrowse, game.id, isModsView(view));

  // Which Settings section to land on, when something sent us there on purpose.
  // Cleared on the way out so a later visit opens where Settings normally opens.
  const [settingsSection, setSettingsSection] = useState<SectionId | undefined>();

  // Paid plugins running this session. A plugin that fails to mount says so once and is
  // then dropped: the app is a mod manager first, and a broken add-on must not take it down.
  //
  // Only the ones that asked for this window. A plugin's panels are a creator's tool and
  // open in Frost's Studio unless its manifest says otherwise — Settings → Plugins is where
  // this app still buys, installs and updates them, with a button that opens the other one.
  const plugins = usePlugins("manager", (id, message) =>
    toast.error(`${id}: ${message}`),
  );
  // The panel on screen, when the current view addresses one. A view naming a plugin that
  // is no longer mounted — a licence that lapsed mid-session — falls through to the
  // built-in pages rather than rendering a blank frame.
  const pluginPanel = (() => {
    const ref = parsePluginView(view);
    if (!ref) return null;
    const p = plugins.find((x) => x.manifest.id === ref.plugin);
    return p?.panels.find((panel) => panel.id === ref.panel) ?? null;
  })();
  // Which page is open, as the usage counters name it.
  //
  // Derived and counted by an effect rather than inside `navigate`, because plenty of
  // things move the view without going through it — the tour, the release showcase, a
  // download row jumping to the Library — and a page nobody counted is worse than one
  // counted twice. Every view is one name, so a tab added to the sidebar is counted
  // without touching this.
  const page = view.startsWith("plugin:")
    ? "view.plugin"   // one bucket: naming each panel would be unbounded cardinality
    : `view.${view}`;
  useEffect(() => {
    track(page);
  }, [page]);

  // Opening a mod's page is a use of the browser, not a page of its own.
  useEffect(() => {
    if (selectedSlug) track("mod.detail");
  }, [selectedSlug]);

  const navigate = useCallback(
    (v: DashboardView) => {
      setView(v);
      closeMod();
      if (v !== "settings") setSettingsSection(undefined);
    },
    [closeMod],
  );

  // "What's new" after an update. Mounted here rather than in App so the showcase can
  // hand someone straight to the Settings section for the feature it just described —
  // this is where the view lives.
  const { release, dismiss: dismissShowcase, replay: replayShowcase } =
    useReleaseShowcase();
  const openSettingsSection = useCallback(
    (section: SectionId) => {
      setSettingsSection(section);
      navigate("settings");
    },
    [navigate],
  );

  // First-run interactive tour — shown once, gated on a localStorage flag. Also
  // re-triggerable from Settings via the TourContext below.
  const [tourRun, setTourRun] = useState(false);
  const startTour = useCallback(() => {
    setView("browse");
    closeMod();
    setTourRun(true);
  }, [closeMod]);
  const endTour = useCallback(() => {
    localStorage.setItem(TOUR_DONE_KEY, "1");
    // The config is the durable record — localStorage above is just the immediate
    // gate, and it doesn't survive the webview's storage being cleared.
    void setIntroSeen({ tour: true }).catch(() => {});
    setTourRun(false);
    navigate("browse");
  }, [navigate]);

  // Auto-start the first-run tour once — but only after the Welcome slideshow has
  // been dismissed, otherwise its spotlights sit behind the overlay and show nothing.
  useEffect(() => {
    if (welcomeActive || integrationChoice === null) return;
    if (config.tourDone || localStorage.getItem(TOUR_DONE_KEY) === "1") return;
    startTour();
  }, [welcomeActive, integrationChoice, startTour, config.tourDone]);

  // The first-run bar, after the tour rather than before it: it hands someone to Browse,
  // which means nothing until the tour has said what Browse is. It shows itself only to an
  // install with no bikes — the component decides that — and settles for good once closed.
  const [getStartedShut, setGetStartedShut] = useState(false);
  const finishGetStarted = useCallback(() => {
    setGetStartedShut(true);
    void setIntroSeen({ getStarted: true }).catch(() => {});
  }, []);
  const browseFor = useCallback(
    (id: string) => {
      const target = modTypes.find((mt) => mt.id === id);
      if (target) changeType(target);
      navigate("browse");
    },
    [modTypes, changeType, navigate],
  );

  // Jump from a download row to the mod it installed: the right library tab, searched for
  // by name. A fresh object each time so repeating the same jump still re-applies it.
  const [libraryFocus, setLibraryFocus] = useState<{ name: string } | null>(null);
  const showInLibrary = useCallback(
    (record: DownloadRecord) => {
      const target = modTypesFor(game.id).find(
        (mt) => mt.installSubpath === record.subpath,
      );
      if (target) changeType(target);
      setLibraryFocus({ name: displayName(record.title) });
      navigate("library");
    },
    [game.id, changeType, navigate],
  );
  const clearLibraryFocus = useCallback(() => setLibraryFocus(null), []);

  // The other direction: from a mod the Library only *remembers* to the catalog page it
  // could be downloaded from again. The category comes from the tab being browsed, the same
  // fallback the detail view uses when nothing more specific is known.
  const openFoundMod = useCallback(
    (slug: string) => {
      openMod(slug, modType.categoryId);
      navigate("browse");
    },
    [openMod, modType.categoryId, navigate],
  );

  // The server an `mxb://server` link named, handed to the Servers tab. A fresh object
  // every time, so the same address shared twice still opens the dialog.
  const [serverLink, setServerLink] = useState<{ address: string } | null>(null);

  // Links shared between players: a server address, or a mod's page. Both only ever open
  // the screen with the fields filled in — the backend has already checked the link, and
  // joining and installing stay behind the buttons they always were.
  useEffect(() => {
    const pending = onDeepLink((link) => {
      if (link.kind === "server") {
        setServerLink({ address: link.address });
        navigate("servers");
        return;
      }
      if (link.kind !== "mod") return;
      // The two catalogs don't share slugs or category ids, so a GP Bikes link opened
      // while MX Bikes is active would look up the wrong site and come back empty. Say so
      // rather than showing an empty page: switching games is one click away in the rail.
      if (link.game !== game.id) {
        const other = games.find((g) => g.id === link.game);
        toast.info(t("deepLink.otherGame", { game: other?.display ?? link.game }));
        return;
      }
      const type = modTypesFor(game.id).find((mt) => mt.id === link.modType);
      if (!type) return;
      openModTarget({
        slug: link.slug,
        subpath: type.installSubpath,
        categoryId: link.category ?? type.categoryId,
      });
    });
    return () => {
      void pending.then((off) => off()).catch(() => {});
    };
  }, [game.id, games, navigate, openModTarget, t]);

  const [ctxLeft, setCtxLeft] = useState<HTMLDivElement | null>(null);
  const [ctxRight, setCtxRight] = useState<HTMLDivElement | null>(null);
  const ctxSlots = useMemo(() => ({ left: ctxLeft, right: ctxRight }), [ctxLeft, ctxRight]);

  // A retained Mods screen needs its last valid source view while another rail item is open;
  // handing it `servers` just because it is hidden would reset the source it is preserving.
  const lastModsView = useRef<ModsView>("browse");
  if (isModsView(view)) lastModsView.current = view;
  // Same for detail: closing it hides the retained subtree, but its required slug remains the
  // last real one until another mod is opened.
  const lastDetailSlug = useRef("");
  if (selectedSlug) lastDetailSlug.current = selectedSlug;

  const builtInsActive = pluginPanel === null;
  const modsActive = builtInsActive && isModsView(view) && !selectedSlug;
  const detailActive = builtInsActive && isModsView(view) && Boolean(selectedSlug);

  useEffect(() => {
    void invoke("set_server_browser_active", {
      active: builtInsActive && view === "servers",
    }).catch(() => {});
  }, [builtInsActive, view]);

  return (
    <TourContext.Provider value={{ startTour }}>
    {/* Outside the installers: both of them write to the history, and the sidebar reads it. */}
    <DownloadsProvider>
    {/* Above the installer, not below it: a *download* stages a plan too now — a pack like
        the OEM bikes arrives as fifty-five mods in one archive — so `InstallProvider` has to
        be able to hand one over. It wraps the views for the same reason it always did: a drop
        anywhere in the window and the Shop's purchases grid both finish in this one sheet. */}
    <DropReviewProvider onInstalled={onInstalled}>
    <InstallProvider onInstalled={onInstalled} onOpenMod={openModTarget}>
      {/* Owns the share/import dialogs for every screen that lists installed content, and
          watches for a share code pasted into the window. */}
      <ShareProvider onImported={onInstalled}>
      {/* Mounted here rather than in `App` so a drop only works once the app is set up —
          there is nowhere to install to before the MX Bikes folder is known. The overlay
          window renders its own tree and deliberately gets no drop target. */}
      <DropZone />
      {/* Nothing on screen: it watches a store the app just opened in the browser and queues
          whatever turns up as a new purchase. Here because it needs the install queue and has
          to outlive every view a store link can be clicked from. */}
      <PurchaseWatcher />
      <GameIntegrationConsent paused={welcomeActive} />
      <SecurePrompt onOpenSettings={openSettingsSection} />
      <TopRail
        view={view}
        plugins={plugins}
        onNavigate={navigate}
        leftRef={setCtxLeft}
        rightRef={setCtxRight}
      />
      <RuntimeBanner />
      <UpdateBanner />
      {/* An offer, not a problem, so it waits for the intro and the tour and sits below both bars. */}
      <Mxbmrp3Prompt paused={welcomeActive || tourRun} className="border-b border-border px-3 py-2" />
      {/* Waits for the intro and the tour so it isn't competing with them for attention,
          then stays put: its steps open Browse, and a panel that closed on the first click
          would strand a new player on a search page. */}
      {!welcomeActive && !tourRun && !getStartedShut && !config.getStartedDone && (
        <GetStarted
          onDone={finishGetStarted}
          onBrowse={browseFor}
          onOpenMod={openModTarget}
          refreshKey={libraryVersion}
        />
      )}
      <div className="flex min-h-0 flex-1">
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden pt-3">
          {pluginPanel && (
            <ContextSlots.Provider value={ctxSlots}>
              <pluginPanel.component />
            </ContextSlots.Provider>
          )}
          <Suspense
            fallback={
              <div className="flex h-full items-center justify-center">
                <LoadingMark />
              </div>
            }
          >
          <RetainedView active={detailActive} slots={ctxSlots}>
            <ModDetail
              slug={selectedSlug ?? lastDetailSlug.current}
              modType={modType}
              categoryId={selectedCategoryId ?? modType.categoryId}
              installed={installed}
              onChanged={onInstalled}
              onBack={closeMod}
            />
          </RetainedView>
          <RetainedView active={modsActive} slots={ctxSlots}>
            <Mods
              view={lastModsView.current}
              modType={modType}
              modTypes={modTypes}
              listing={listing}
              installed={installed}
              refreshKey={libraryVersion}
              onOpenMod={openMod}
              onChangeType={changeType}
            />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "servers"} slots={ctxSlots}>
            <Servers link={serverLink} />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "ranked"} slots={ctxSlots}>
            <Ranked onFindServers={() => setView("servers")} />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "library"} slots={ctxSlots}>
            <Library
              modType={modType}
              onChangeType={changeType}
              refreshKey={libraryVersion}
              onChanged={onInstalled}
              focus={libraryFocus}
              onFocusApplied={clearLibraryFocus}
              onOpenMod={openFoundMod}
              onOpenStore={navigate}
            />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "downloads"} slots={ctxSlots}>
            <Downloads
              onOpenMod={openModTarget}
              onShowInLibrary={showInLibrary}
              onOpenShop={() => navigate("shop")}
              onOpenHub={() => navigate("hub")}
            />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "locker"} slots={ctxSlots}>
            <Locker />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "presets"} slots={ctxSlots}>
            <Presets
              onOpenLocker={() => setView("locker")}
              onOpenSettings={() => openSettingsSection("folder")}
            />
          </RetainedView>
          <RetainedView active={builtInsActive && view === "manage"} slots={ctxSlots}>
            <Manage />
          </RetainedView>
          <RetainedView
            active={
              builtInsActive &&
              (view === "settings" || (view.startsWith("plugin:") && pluginPanel === null))
            }
            slots={ctxSlots}
          >
            <Settings
              initialSection={settingsSection}
              onShowWhatsNew={replayShowcase}
            />
          </RetainedView>
          </Suspense>
        </div>
      </div>
      {tourRun && <Tour navigate={navigate} onDone={endTour} />}
      {/* Never over the intro: a first run gets Welcome and the tour, and an update
          landing mid-tour would spotlight UI behind a modal. */}
      {release && !tourRun && !welcomeActive && (
        <ReleaseShowcase
          release={release}
          onDone={dismissShowcase}
          onOpenSettings={openSettingsSection}
        />
      )}
      </ShareProvider>
      </InstallProvider>
    </DropReviewProvider>
    </DownloadsProvider>
    </TourContext.Provider>
  );
};

export default Dashboard;
