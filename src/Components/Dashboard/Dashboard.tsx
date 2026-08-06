import { useCallback, useEffect, useState } from "react";
import Sidebar, { type DashboardView } from "../Shell/Sidebar";
import Library from "../Library/Library";
import Locker from "../Locker/Locker";
import Presets from "../Presets/Presets";
import RiderStudio from "../Rider/RiderStudio";
import Browse from "../Browse/Browse";
import Shop from "../Shop/Shop";
import ModDetail from "../ModDetail/ModDetail";
import Settings from "../Settings/Settings";
import Tour, { TourContext, TOUR_DONE_KEY } from "../Tour/Tour";
import { InstallProvider } from "../../Context/Install";
import { useConfig } from "../../Context/Config";
import {
  DEFAULT_MOD_TYPE,
  getInstalledMods,
  normalizeModName,
  setIntroSeen,
  type ModType,
} from "../../api/mods";
import type { Loadout } from "../../types";

interface DashboardProps {
  /** True while the Welcome slideshow is still up. The tour waits for it to close
   *  so its spotlights land on visible UI, not behind the overlay. */
  welcomeActive?: boolean;
}

const Dashboard = ({ welcomeActive = false }: DashboardProps) => {
  const { config } = useConfig();
  const [view, setView] = useState<DashboardView>("browse");
  const [modType, setModType] = useState<ModType>(DEFAULT_MOD_TYPE);
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  // The browse category the opened mod was found under (drives livery routing).
  const [selectedCategoryId, setSelectedCategoryId] = useState<number | null>(null);
  // Bumped after an install so the library re-scans.
  const [libraryVersion, setLibraryVersion] = useState(0);
  // Normalized names of installed mods for the active type (for "in library" badges).
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());
  // A preset handed off from the Presets tab to load in the Rider tab (its
  // "View in Rider" button). Consumed once by the Rider view, then cleared.
  const [riderPreset, setRiderPreset] = useState<Loadout | null>(null);

  useEffect(() => {
    let cancelled = false;
    getInstalledMods(modType.installSubpath)
      .then((installed) => {
        if (cancelled) return;
        setInstalledNames(
          new Set(installed.map((m) => normalizeModName(m.name))),
        );
      })
      .catch(() => !cancelled && setInstalledNames(new Set()));
    return () => {
      cancelled = true;
    };
  }, [modType, libraryVersion]);

  // FrostMod installs itself silently on first run (see FrostmodProvider) —
  // no prompt here.

  const onInstalled = useCallback(() => setLibraryVersion((v) => v + 1), []);

  const openMod = useCallback((slug: string, categoryId: number) => {
    setSelectedSlug(slug);
    setSelectedCategoryId(categoryId);
  }, []);

  const changeType = useCallback((t: ModType) => {
    setModType(t);
    setSelectedSlug(null);
  }, []);

  const navigate = useCallback((v: DashboardView) => {
    setView(v);
    setSelectedSlug(null);
  }, []);

  // First-run interactive tour — shown once, gated on a localStorage flag. Also
  // re-triggerable from Settings via the TourContext below.
  const [tourRun, setTourRun] = useState(false);
  const startTour = useCallback(() => {
    setView("browse");
    setSelectedSlug(null);
    setTourRun(true);
  }, []);
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
    <InstallProvider onInstalled={onInstalled}>
      <div className="flex min-h-0 flex-1">
        <Sidebar view={view} onNavigate={navigate} />
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          {view === "browse" && selectedSlug ? (
            <ModDetail
              slug={selectedSlug}
              modType={modType}
              categoryId={selectedCategoryId ?? modType.categoryId}
              installedNames={installedNames}
              onBack={() => setSelectedSlug(null)}
            />
          ) : view === "browse" ? (
            <Browse
              modType={modType}
              installedNames={installedNames}
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
            <Presets onOpenInRider={openInRider} />
          ) : view === "rider" ? (
            <RiderStudio initialLoadout={riderPreset} onLoaded={clearRiderPreset} />
          ) : (
            <Settings />
          )}
        </div>
      </div>
      {tourRun && <Tour navigate={navigate} onDone={endTour} />}
    </InstallProvider>
    </TourContext.Provider>
  );
};

export default Dashboard;
