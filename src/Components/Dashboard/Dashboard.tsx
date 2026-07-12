import { useCallback, useEffect, useState } from "react";
import "./Dashboard.scss";
import Header, { type DashboardView } from "../Header/Header";
import Library from "../Library/Library";
import Browse from "../Browse/Browse";
import ModDetail from "../ModDetail/ModDetail";
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
  // Bumped after an install so the library re-scans.
  const [libraryVersion, setLibraryVersion] = useState(0);
  // Normalized names of installed mods for the active type (for "in library" badges).
  const [installedNames, setInstalledNames] = useState<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    getInstalledMods(modType.installSubpath)
      .then((folders) => {
        if (cancelled) return;
        const names = new Set<string>();
        for (const f of folders) {
          names.add(normalizeModName(f.name));
          for (const m of f.mods) names.add(normalizeModName(m.name));
        }
        setInstalledNames(names);
      })
      .catch(() => !cancelled && setInstalledNames(new Set()));
    return () => {
      cancelled = true;
    };
  }, [modType, libraryVersion]);

  const onInstalled = useCallback(() => setLibraryVersion((v) => v + 1), []);

  const changeType = useCallback((t: ModType) => {
    setModType(t);
    setSelectedSlug(null);
  }, []);

  return (
    <div id={"dashboard"}>
      <Header
        view={view}
        onNavigate={setView}
        modType={modType}
        onChangeType={changeType}
      />
      <div className={"dashboard-content"}>
        {selectedSlug ? (
          <ModDetail
            slug={selectedSlug}
            modType={modType}
            installedNames={installedNames}
            onBack={() => setSelectedSlug(null)}
            onInstalled={onInstalled}
          />
        ) : view === "browse" ? (
          <Browse
            modType={modType}
            installedNames={installedNames}
            onOpenMod={setSelectedSlug}
          />
        ) : (
          <Library modType={modType} refreshKey={libraryVersion} />
        )}
      </div>
    </div>
  );
};

export default Dashboard;
