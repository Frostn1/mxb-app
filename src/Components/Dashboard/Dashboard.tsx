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
import { InstallProvider } from "../../Context/Install";
import {
  DEFAULT_MOD_TYPE,
  getInstalledMods,
  normalizeModName,
  type ModType,
} from "../../api/mods";
import type { Loadout } from "../../types";

const Dashboard = () => {
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

  // Jump from Presets into the Rider tab with a preset loaded, to view it on the model.
  const openInRider = useCallback((lo: Loadout) => {
    setRiderPreset(lo);
    navigate("rider");
  }, [navigate]);
  const clearRiderPreset = useCallback(() => setRiderPreset(null), []);

  return (
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
    </InstallProvider>
  );
};

export default Dashboard;
