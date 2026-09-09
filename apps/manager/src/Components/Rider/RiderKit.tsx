import { useCallback, useEffect, useMemo, useState } from "react";
import type { Loadout, RiderPart } from "../../types";
import { EMPTY_LOADOUT, loadScans, type Scans } from "../../lib/presets";
import { scanBikeTargets } from "../../api/mods";
import { RiderKitContext, type RiderKitValue } from "./RiderKitContext";

interface RiderKitProviderProps {
  /** A preset handed over by the Presets tab. */
  initialLoadout?: Loadout | null;
  /** The bike that preset was built against — a loadout doesn't name one. */
  initialBike?: string | null;
  onLoaded?: () => void;
  children: React.ReactNode;
}

/**
 * Holds the kit for the Rider and Pose tabs.
 *
 * All of this used to live inside `RiderStudio`, which meant the Pose tab could only ever see
 * a preset that Presets handed it directly — and only if it happened to be mounted at the
 * time, since both sub-views raced for the same handoff. Above them both, a preset seeds the
 * kit once and every edit next door is on screen here too.
 */
export default function RiderKitProvider({
  initialLoadout,
  initialBike,
  onLoaded,
  children,
}: RiderKitProviderProps) {
  const [scans, setScans] = useState<Scans | null>(null);
  const [bikes, setBikes] = useState<string[]>([]);
  const [loadout, setLoadout] = useState<Loadout>(EMPTY_LOADOUT);
  const [bike, setBike] = useState("");
  // Nothing hidden to start — see the note on the toggles in `RiderStudio`.
  const [hidden, setHidden] = useState<RiderPart["part"][]>([]);
  const [error, setError] = useState<string | null>(null);

  const setSlot = useCallback((key: keyof Loadout, value: string) => {
    setLoadout((prev) => ({ ...prev, [key]: value }));
  }, []);

  const toggleHidden = useCallback((part: RiderPart["part"]) => {
    setHidden((prev) =>
      prev.includes(part) ? prev.filter((p) => p !== part) : [...prev, part],
    );
  }, []);

  const reload = useCallback(async () => {
    setError(null);
    try {
      const sc = await loadScans();
      setScans(sc);
      // Kit, gloves and profile goggles are all looked up by rider profile. A preset brings
      // one; a tab opened cold has none, which left those slots empty. Seed the first
      // installed profile unless one is already set.
      setLoadout((prev) =>
        prev.rider || !sc.riderProfiles.length ? prev : { ...prev, rider: sc.riderProfiles[0] },
      );
      // Every bike a paint can be installed for, plus the ids read out of the profiles — the
      // OEM bikes only exist inside the locked archive, so a profile is the only place their
      // id can be found until someone installs a paint for one.
      const bs = await scanBikeTargets().catch(() => [] as string[]);
      setBikes(bs);
      setBike((b) => b || bs[0] || "");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  // A preset from Presets. The handed-over bike wins even before the scan lands — the picker
  // keeps a value it doesn't recognise rather than snapping to the first installed bike.
  useEffect(() => {
    if (!initialLoadout) return;
    setLoadout(initialLoadout);
    if (initialBike) setBike(initialBike);
    onLoaded?.();
  }, [initialLoadout, initialBike, onLoaded]);

  const value = useMemo<RiderKitValue>(
    () => ({
      scans,
      bikes,
      loadout,
      setLoadout,
      setSlot,
      bike,
      setBike,
      hidden,
      toggleHidden,
      reload,
      error,
    }),
    [scans, bikes, loadout, setSlot, bike, hidden, toggleHidden, reload, error],
  );

  return <RiderKitContext.Provider value={value}>{children}</RiderKitContext.Provider>;
}
