import { createContext, useContext } from "react";
import type { Loadout, RiderPart } from "../../types";
import type { Scans } from "../../lib/presets";

/**
 * The kit the Rider and Pose tabs both show.
 *
 * One rider, one bike, one set of show-on-model toggles, held above both sub-views. They used
 * to keep their own copies and only ever agreed when a preset was handed to them at the same
 * moment — so a look composed in Rider was not the look Pose stood in a position.
 */
export interface RiderKitValue {
  /** What's installed, for the pickers. Null until the first scan lands. */
  scans: Scans | null;
  /** Every bike a paint or profile knows about. */
  bikes: string[];
  loadout: Loadout;
  setLoadout: (next: Loadout | ((prev: Loadout) => Loadout)) => void;
  /** Write one slot, which is all the pickers ever do. */
  setSlot: (key: keyof Loadout, value: string) => void;
  /** The bike the paint and model-swap slots are read against. Not part of a loadout. */
  bike: string;
  setBike: (bike: string) => void;
  /** Gear toggled off the model — a preview choice, never part of the kit that gets saved. */
  hidden: RiderPart["part"][];
  toggleHidden: (part: RiderPart["part"]) => void;
  /** Re-read the mods folder. */
  reload: () => Promise<void>;
  /** What went wrong reading it, if anything. */
  error: string | null;
}

// Component-free, like `Context/Config.ts` — the context identity has to survive Fast Refresh
// or a hot update throws "used outside its provider" halfway through a session.
export const RiderKitContext = createContext<RiderKitValue | null>(null);

export function useRiderKit(): RiderKitValue {
  const ctx = useContext(RiderKitContext);
  if (!ctx) throw new Error("useRiderKit must be used within RiderKitProvider");
  return ctx;
}
