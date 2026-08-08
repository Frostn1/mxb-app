import { useCallback, useEffect, useState } from "react";
import {
  DEFAULT_MOD_TYPE,
  MOD_TYPES,
  scanLibrary,
  type ModType,
} from "../api/mods";
import {
  EMPTY_INSTALLED_INDEX,
  buildInstalledIndex,
  type InstalledIndex,
} from "./installedMatch";
import type { ModTarget } from "../Context/Install";

/**
 * The state Browse + ModDetail need to work together: which mod type is showing,
 * which mod is open, and what's already on disk.
 *
 * Shared by the main window's Dashboard and the in-game overlay, which both put the
 * same two components on screen and would otherwise re-derive this identically.
 */
export function useModBrowsing(onOpenedFromElsewhere?: () => void) {
  const [modType, setModType] = useState<ModType>(DEFAULT_MOD_TYPE);
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  // The browse category the opened mod was found under (drives livery routing).
  const [selectedCategoryId, setSelectedCategoryId] = useState<number | null>(null);
  // Bumped after an install so the library re-scans.
  const [libraryVersion, setLibraryVersion] = useState(0);
  // What's on disk for the active type, as a fuzzy lookup (for "in library" badges).
  const [installed, setInstalled] = useState<InstalledIndex>(EMPTY_INSTALLED_INDEX);

  // The full library scan, not `getInstalledMods` — that one sees `.pkz` files only, so
  // extracted track folders and every `.pnt` paint/livery counted as "not installed".
  useEffect(() => {
    let cancelled = false;
    scanLibrary(modType.installSubpath)
      .then((entries) => {
        if (cancelled) return;
        setInstalled(buildInstalledIndex(entries.map((e) => e.name)));
      })
      .catch(() => !cancelled && setInstalled(EMPTY_INSTALLED_INDEX));
    return () => {
      cancelled = true;
    };
  }, [modType, libraryVersion]);

  const onInstalled = useCallback(() => setLibraryVersion((v) => v + 1), []);

  const openMod = useCallback((slug: string, categoryId: number) => {
    setSelectedSlug(slug);
    setSelectedCategoryId(categoryId);
  }, []);

  const closeMod = useCallback(() => setSelectedSlug(null), []);

  // Jump straight to a mod's detail page from anywhere (the failed-install banner) —
  // restores the mod type its install targeted, so Browse and the detail page agree on
  // folders and livery routing. The caller navigates to wherever Browse lives.
  const openModTarget = useCallback(
    ({ slug, subpath, categoryId }: ModTarget) => {
      const type = MOD_TYPES.find((t) => t.installSubpath === subpath);
      if (type) setModType(type);
      setSelectedCategoryId(categoryId ?? type?.categoryId ?? null);
      setSelectedSlug(slug);
      onOpenedFromElsewhere?.();
    },
    [onOpenedFromElsewhere],
  );

  const changeType = useCallback((t: ModType) => {
    setModType(t);
    setSelectedSlug(null);
  }, []);

  return {
    modType,
    changeType,
    selectedSlug,
    selectedCategoryId,
    installed,
    libraryVersion,
    onInstalled,
    openMod,
    openModTarget,
    closeMod,
  };
}
