import { useCallback, useEffect, useState } from "react";
import Sidebar, { type DashboardView } from "../Shell/Sidebar";
import Library from "../Library/Library";
import Locker from "../Locker/Locker";
import Presets from "../Presets/Presets";
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

  return (
    <InstallProvider onInstalled={onInstalled}>
      <div className="flex h-full min-h-0">
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
            <Presets />
          ) : (
            <Settings />
          )}
        </div>
      </div>
    </InstallProvider>
  );
};

export default Dashboard;
