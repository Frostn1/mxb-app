import { useCallback, useEffect, useState } from "react";
import { modTypesFor, scanLibrary, type ModType } from "../api/mods";
import type { GameId } from "../types";
import {
  EMPTY_INSTALLED_INDEX,
  buildInstalledIndex,
  type InstalledIndex,
} from "./installedMatch";
import { useModListing } from "./useModListing";
import type { ModTarget } from "../Context/Install";

/**
 * The state Browse + ModDetail need to work together: which mod type is showing,
 * which mod is open, and what's already on disk.
 *
 * Shared by the main window's Dashboard and the in-game overlay, which both put the
 * same two components on screen and would otherwise re-derive this identically.
 *
 * `game` is passed in rather than read from `ConfigContext` because the overlay calls
 * this hook *above* the provider it renders, so a context read there would silently
 * report MX Bikes whatever game was active.
 */
export function useModBrowsing(
  onOpenedFromElsewhere?: () => void,
  game: GameId = "mxb",
) {
  const modTypes = modTypesFor(game);
  const [modType, setModType] = useState<ModType>(modTypes[0]);
  const [selectedSlug, setSelectedSlug] = useState<string | null>(null);
  // The browse category the opened mod was found under (drives livery routing).
  const [selectedCategoryId, setSelectedCategoryId] = useState<number | null>(null);
  // Bumped after an install so the library re-scans.
  const [libraryVersion, setLibraryVersion] = useState(0);
  // What's on disk for the active type, as a fuzzy lookup (for "in library" badges).
  const [installed, setInstalled] = useState<InstalledIndex>(EMPTY_INSTALLED_INDEX);
  // The grid's own state — filters, fetched pages, scroll offset. Held here, above the
  // Browse/ModDetail swap, so opening a mod and coming back lands on the same screen.
  const listing = useModListing(modType);

  // Follow a game switch. The two catalogs don't share category ids, so carrying the old
  // `modType` over would query the new site with the old site's numbers and come back
  // empty. Keeps the equivalent tab where there is one (tracks stays tracks).
  useEffect(() => {
    // Re-derived here rather than closing over `modTypes` so `game` is the only
    // dependency — the two are the same thing, and depending on both is noise.
    const next = modTypesFor(game);
    setModType((current) => next.find((t) => t.id === current.id) ?? next[0]);
    setSelectedSlug(null);
  }, [game]);

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
      const type = modTypes.find((t) => t.installSubpath === subpath);
      if (type) setModType(type);
      setSelectedCategoryId(categoryId ?? type?.categoryId ?? null);
      setSelectedSlug(slug);
      onOpenedFromElsewhere?.();
    },
    [onOpenedFromElsewhere, modTypes],
  );

  const changeType = useCallback((t: ModType) => {
    setModType(t);
    setSelectedSlug(null);
  }, []);

  return {
    modType,
    /** The active game's browse tree — what the type selector offers. */
    modTypes,
    changeType,
    /** Everything the browse grid renders from — see `useModListing`. */
    listing,
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
