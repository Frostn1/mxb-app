import { useCallback, useEffect, useState } from "react";
import Sidebar, { type DashboardView } from "../Shell/Sidebar";
import Library from "../Library/Library";
import Locker from "../Locker/Locker";
import Presets from "../Presets/Presets";
import Manage from "../Manage/Manage";
import Servers from "../Servers/Servers";
import RiderStudio from "../Rider/RiderStudio";
import Browse from "../Browse/Browse";
import Shop from "../Shop/Shop";
import ModDetail from "../ModDetail/ModDetail";
import DropZone from "../Dropzone/DropZone";
import Settings, { type SectionId } from "../Settings/Settings";
import Tour, { TourContext, TOUR_DONE_KEY } from "../Tour/Tour";
import ReleaseShowcase from "../Showcase/ReleaseShowcase";
import { useReleaseShowcase } from "../Showcase/useReleaseShowcase";
import { InstallProvider } from "../../Context/Install";
import { DropReviewProvider } from "../../Context/DropReview";
import { useConfig } from "../../Context/Config";
import { setIntroSeen } from "../../api/mods";
import { useModBrowsing } from "../../lib/useModBrowsing";
import type { Loadout } from "../../types";

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

  const showBrowse = useCallback(() => setView("browse"), []);
  const {
    modType,
    modTypes,
    changeType,
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
    if (welcomeActive) return;
    if (config.tourDone || localStorage.getItem(TOUR_DONE_KEY) === "1") return;
    startTour();
  }, [welcomeActive, startTour, config.tourDone]);

  // Jump from Presets into the Rider tab with a preset loaded, to view it on the model.
  const openInRider = useCallback((lo: Loadout) => {
    setRiderPreset(lo);
    navigate("rider");
  }, [navigate]);
  const clearRiderPreset = useCallback(() => setRiderPreset(null), []);

  return (
    <TourContext.Provider value={{ startTour }}>
    <InstallProvider onInstalled={onInstalled} onOpenMod={openModTarget}>
      {/* Wraps the views because two of them stage plans: a drop anywhere in the window, and
          the Shop's purchases grid. Both finish in the one review sheet this renders. */}
      <DropReviewProvider onInstalled={onInstalled}>
      {/* Mounted here rather than in `App` so a drop only works once the app is set up —
          there is nowhere to install to before the MX Bikes folder is known. The overlay
          window renders its own tree and deliberately gets no drop target. */}
      <DropZone />
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} onNavigate={navigate} />
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
              installed={installed}
              onOpenMod={openMod}
              onChangeType={changeType}
            />
          ) : view === "shop" ? (
            <Shop refreshKey={libraryVersion} />
          ) : view === "library" ? (
            <Library
              modType={modType}
              onChangeType={changeType}
              refreshKey={libraryVersion}
              onChanged={onInstalled}
            />
          ) : view === "locker" ? (
            <Locker />
          ) : view === "presets" ? (
            <Presets
              onOpenInRider={openInRider}
              onOpenLocker={() => setView("locker")}
              onOpenSettings={() => openSettingsSection("folder")}
            />
          ) : view === "rider" ? (
            <RiderStudio initialLoadout={riderPreset} onLoaded={clearRiderPreset} />
          ) : view === "servers" ? (
            <Servers />
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
      </DropReviewProvider>
    </InstallProvider>
    </TourContext.Provider>
  );
};

export default Dashboard;
