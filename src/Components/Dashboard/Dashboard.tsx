import { useCallback, useEffect, useState } from "react";
import Sidebar, { type DashboardView } from "../Shell/Sidebar";
import Library from "../Library/Library";
import Downloads from "../Downloads/Downloads";
import Locker from "../Locker/Locker";
import Presets from "../Presets/Presets";
import Manage from "../Manage/Manage";
import Studio, { type StudioTab } from "../Studio/Studio";
import Browse from "../Browse/Browse";
import Shop from "../Shop/Shop";
import Hub from "../Hub/Hub";
import ModDetail from "../ModDetail/ModDetail";
import DropZone from "../Dropzone/DropZone";
import Settings, { type SectionId } from "../Settings/Settings";
import Tour, { TourContext, TOUR_DONE_KEY } from "../Tour/Tour";
import ReleaseShowcase from "../Showcase/ReleaseShowcase";
import { useReleaseShowcase } from "../Showcase/useReleaseShowcase";
import { InstallProvider } from "../../Context/Install";
import { DownloadsProvider } from "../../Context/Downloads";
import { DropReviewProvider } from "../../Context/DropReview";
import { useConfig } from "../../Context/Config";
import { modTypesFor, setIntroSeen } from "../../api/mods";
import { useModBrowsing } from "../../lib/useModBrowsing";
import { displayName } from "../../lib/mods";
import { track } from "../../lib/analytics";
import type { DownloadRecord, Loadout } from "../../types";

interface DashboardProps {
  /** True while the Welcome slideshow is still up. The tour waits for it to close
   *  so its spotlights land on visible UI, not behind the overlay. */
  welcomeActive?: boolean;
}

const Dashboard = ({ welcomeActive = false }: DashboardProps) => {
  const { config, game } = useConfig();
  const [view, setView] = useState<DashboardView>("browse");
  // A preset handed off from the Presets tab to load in the Rider tab (its
  // "View in Rider" button). Consumed once by the Rider view, then cleared.
  const [riderPreset, setRiderPreset] = useState<Loadout | null>(null);
  const [riderBike, setRiderBike] = useState<string | null>(null);
  // Which Studio sub-view is open. Here rather than inside `Studio` because two things
  // outside it open the Studio *at* a sub-view — Presets' "View in Rider", and the tour.
  const [studioTab, setStudioTab] = useState<StudioTab>("designer");

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
  } = useModBrowsing(showBrowse, game.id);

  // FrostMod installs itself silently on first run (see FrostmodProvider) —
  // no prompt here.

  // Which Settings section to land on, when something sent us there on purpose.
  // Cleared on the way out so a later visit opens where Settings normally opens.
  const [settingsSection, setSettingsSection] = useState<SectionId | undefined>();

  // Which page is open, as the usage counters name it.
  //
  // Derived and counted by an effect rather than inside `navigate`, because plenty of
  // things move the view without going through it — the tour, the release showcase, a
  // download row jumping to the Library — and a page nobody counted is worse than one
  // counted twice. Studio's sub-views are pages in their own right; everything else is
  // one name, so a tab added to the sidebar is counted without touching this.
  const page = view === "studio" ? `view.studio.${studioTab}` : `view.${view}`;
  useEffect(() => {
    track(page);
  }, [page]);

  // Opening a mod's page is a use of the browser, not a page of its own.
  useEffect(() => {
    if (selectedSlug) track("mod.detail");
  }, [selectedSlug]);

  const navigate = useCallback(
    (v: DashboardView, studio?: StudioTab) => {
      setView(v);
      if (studio) setStudioTab(studio);
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
    if (welcomeActive) return;
    if (config.tourDone || localStorage.getItem(TOUR_DONE_KEY) === "1") return;
    startTour();
  }, [welcomeActive, startTour, config.tourDone]);

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

  // Jump from Presets into the Rider tab with a preset loaded, to view it on the model.
  const openInRider = useCallback((lo: Loadout, bike: string) => {
    setRiderPreset(lo);
    // The bike rides along: a loadout names a livery and a model swap but never the bike
    // they belong to, so without this the Rider tab would dress whichever bike it landed on.
    setRiderBike(bike);
    navigate("studio", "rider");
  }, [navigate]);
  const clearRiderPreset = useCallback(() => setRiderPreset(null), []);

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
      {/* Mounted here rather than in `App` so a drop only works once the app is set up —
          there is nowhere to install to before the MX Bikes folder is known. The overlay
          window renders its own tree and deliberately gets no drop target. */}
      <DropZone />
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} studioTab={studioTab} onNavigate={navigate} />
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          {view === "browse" && selectedSlug ? (
            <ModDetail
              slug={selectedSlug}
              modType={modType}
              categoryId={selectedCategoryId ?? modType.categoryId}
              installed={installed}
              onBack={closeMod}
            />
          ) : view === "browse" ? (
            <Browse
              modType={modType}
              modTypes={modTypes}
              listing={listing}
              installed={installed}
              onOpenMod={openMod}
              onChangeType={changeType}
            />
          ) : view === "shop" ? (
            <Shop refreshKey={libraryVersion} />
          ) : view === "hub" ? (
            <Hub refreshKey={libraryVersion} />
          ) : view === "library" ? (
            <Library
              modType={modType}
              onChangeType={changeType}
              refreshKey={libraryVersion}
              onChanged={onInstalled}
              focus={libraryFocus}
              onFocusApplied={clearLibraryFocus}
              onOpenMod={openFoundMod}
            />
          ) : view === "downloads" ? (
            <Downloads
              onOpenMod={openModTarget}
              onShowInLibrary={showInLibrary}
              onOpenShop={() => navigate("shop")}
              onOpenHub={() => navigate("hub")}
            />
          ) : view === "locker" ? (
            <Locker />
          ) : view === "presets" ? (
            <Presets
              onOpenInRider={openInRider}
              onOpenLocker={() => setView("locker")}
              onOpenSettings={() => openSettingsSection("folder")}
            />
          ) : view === "studio" ? (
            <Studio
              tab={studioTab}
              onTab={setStudioTab}
              riderPreset={riderPreset}
              riderBike={riderBike}
              onRiderPresetLoaded={clearRiderPreset}
            />
          ) : view === "manage" ? (
            <Manage />
          ) : (
            <Settings
              initialSection={settingsSection}
              onShowWhatsNew={replayShowcase}
            />
          )}
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
      </InstallProvider>
    </DropReviewProvider>
    </DownloadsProvider>
    </TourContext.Provider>
  );
};

export default Dashboard;
