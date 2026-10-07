import type { ModType } from "@frost/shared/api/mods";
import type { LibraryEntry } from "@frost/shared/types";

/**
 * What the grid actually shows for a type.
 *
 * Bikes keeps liveries and model swaps out of its own list — they belong to a bike, not
 * beside it — so a count that included them would never match what is on screen.
 */
export function countable(
  entries: LibraryEntry[],
  modType: ModType,
  liveries = false,
): LibraryEntry[] {
  return modType.id === "bikes" && !liveries
    ? entries.filter((e) => e.category !== "bikePaint" && e.category !== "bikeModelSwap")
    : entries;
}
