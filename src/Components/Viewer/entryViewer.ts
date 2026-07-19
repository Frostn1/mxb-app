import type { LibraryEntry, LibraryCategory } from "../../types";
import { displayName } from "../../lib/mods";

/** Gear categories that preview on the rider rather than as a bike. */
const RIDER_CATS = new Set<LibraryCategory>([
  "helmet",
  "helmetPaint",
  "goggles",
  "boots",
  "bootPaint",
  "protection",
  "protectionPaint",
  "gloves",
  "outfit",
]);

/** The owner key a model's paints/swaps point back to (their `parent`). */
function ownerKey(entry: LibraryEntry): string {
  return entry.kind === "folder" ? entry.name : displayName(entry.name);
}

type GearSlot = "helmet" | "boots" | "protection";

/** The `ViewerDialog` props that show a library entry in 3D. */
export interface EntryViewerProps {
  mode: "bike" | "rider";
  /** Candidate `.pnt` paints to offer (a paint previews itself; a model gathers its own). */
  paintPaths: string[];
  /** Bike folder / `.pkz` to load real geometry + paints from. */
  modelSource?: string;
  /** An installed gear item to show on the rider. */
  gearSource?: string;
  gearPart?: GearSlot;
  /** A loose gear paint whose own model may not be installed → the stock model slot. */
  stockGearPart?: GearSlot;
}

/**
 * Resolve the `ViewerDialog` props for a library entry, or `null` when it can't
 * be shown in 3D. Single source of truth shared by the library list's quick-view
 * button and the detail view's "View in 3D" — so the two never drift.
 */
export function entryViewerProps(
  entry: LibraryEntry,
  entries: LibraryEntry[],
): EntryViewerProps | null {
  const isPaint = entry.kind === "loose" && /\.pnt$/i.test(entry.name);
  const viewable =
    entry.category === "bike" || RIDER_CATS.has(entry.category) || isPaint;
  if (!viewable) return null;

  // A gear *model* entry (not a paint) previews on the rider in its own slot.
  const gearPart: GearSlot | undefined = isPaint
    ? undefined
    : entry.category === "helmet"
      ? "helmet"
      : entry.category === "boots"
        ? "boots"
        : entry.category === "protection"
          ? "protection"
          : undefined;
  // A loose gear *paint* previews on the game's stock model for that slot.
  const stockGearPart: GearSlot | undefined =
    entry.category === "helmetPaint"
      ? "helmet"
      : entry.category === "bootPaint"
        ? "boots"
        : entry.category === "protectionPaint"
          ? "protection"
          : undefined;
  const paintPaths = isPaint
    ? [entry.path]
    : entries
        .filter(
          (e) =>
            e.parent === ownerKey(entry) &&
            e.kind === "loose" &&
            /\.pnt$/i.test(e.name),
        )
        .map((e) => e.path);

  return {
    mode: RIDER_CATS.has(entry.category) ? "rider" : "bike",
    paintPaths,
    modelSource: entry.category === "bike" ? entry.path : undefined,
    gearSource: gearPart ? entry.path : undefined,
    gearPart,
    stockGearPart,
  };
}
