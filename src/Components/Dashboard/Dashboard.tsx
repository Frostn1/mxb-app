import { useCallback, useEffect, useRef, useState } from "react";
import { toast } from "sonner";
import Sidebar, { type DashboardView } from "../Shell/Sidebar";
import Library from "../Library/Library";
import Browse from "../Browse/Browse";
import ModDetail from "../ModDetail/ModDetail";
import Settings from "../Settings/Settings";
import { InstallProvider } from "../../Context/Install";
import { useFrostmod } from "../../Context/Frostmod";
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

  // First-run nudge: if FrostMod isn't installed yet, offer to set it up once.
  const { status, install } = useFrostmod();
  const promptedFrostmod = useRef(false);
  useEffect(() => {
    if (status && !status.installed && !promptedFrostmod.current) {
      promptedFrostmod.current = true;
      toast("Set up FrostMod?", {
        description:
          "Install FrostMod so new mods live-reload into the game — no restart.",
        duration: Infinity,
        action: { label: "Install", onClick: () => void install() },
      });
    }
  }, [status, install]);

  const onInstalled = useCallback(() => setLibraryVersion((v) => v + 1), []);

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
        <Sidebar
          view={view}
          onNavigate={navigate}
          libraryCount={installedNames.size}
        />
        <div className="min-h-0 min-w-0 flex-1 overflow-hidden">
          {view === "browse" && selectedSlug ? (
            <ModDetail
              slug={selectedSlug}
              modType={modType}
              installedNames={installedNames}
              onBack={() => setSelectedSlug(null)}
            />
          ) : view === "browse" ? (
            <Browse
              modType={modType}
              installedNames={installedNames}
              onOpenMod={setSelectedSlug}
              onChangeType={changeType}
            />
          ) : view === "library" ? (
            <Library
              modType={modType}
              onChangeType={changeType}
              refreshKey={libraryVersion}
              onChanged={onInstalled}
            />
          ) : (
            <Settings />
          )}
        </div>
      </div>
    </InstallProvider>
  );
};

export default Dashboard;
