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

/** For each paint category, the model category that wears it. A paint's `parent` names
 *  one of these, which is what lets a paint be shown on the thing it was painted for. */
const PAINT_WEARER: Partial<Record<LibraryCategory, LibraryCategory>> = {
  bikePaint: "bike",
  helmetPaint: "helmet",
  // A goggle paint belongs to the helmet whose folder it sits in.
  goggles: "helmet",
  bootPaint: "boots",
  protectionPaint: "protection",
};

/** The installed model a paint belongs to, if it's in the library. */
function wearerOf(
  entry: LibraryEntry,
  entries: LibraryEntry[],
): LibraryEntry | undefined {
  const wearer = PAINT_WEARER[entry.category];
  if (!wearer || !entry.parent) return undefined;
  return entries.find(
    (e) => e.category === wearer && ownerKey(e) === entry.parent,
  );
}

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
  /** Paint to open on, by name — set when a paint was clicked and its model was found,
   *  so the viewer shows that paint rather than whichever one sorts first. */
  initialPaint?: string;
  /** Same, for the goggle picker: a goggle paint selects there, not on the shell. */
  initialGoggles?: string;
}

/**
 * Resolve the `ViewerDialog` props for a library entry, or `null` when it can't
 * be shown in 3D. Single source of truth shared by the library list's quick-view
 * button and the detail view's "View in 3D" — so the two never drift.
 *
 * A paint is shown on the model it was painted for whenever that model is installed:
 * on its own a `.pnt` is a set of textures with nothing to wrap, and the stock body it
 * used to fall back on isn't the shape the paint was drawn against.
 */
export function entryViewerProps(
  entry: LibraryEntry,
  entries: LibraryEntry[],
  bikePreview = true,
): EntryViewerProps | null {
  const isPaint = entry.kind === "loose" && /\.pnt$/i.test(entry.name);
  const viewable =
    entry.category === "bike" || RIDER_CATS.has(entry.category) || isPaint;
  if (!viewable) return null;

  // A bike (or bike paint) previews from a locked archive that only the optional
  // local module can decode. Mirror the `mode` computed below: anything not a rider
  // category renders as a bike. Without the module, offer no bike 3D view at all.
  if (!RIDER_CATS.has(entry.category) && !bikePreview) return null;

  // A paint clicked in the library: show the model that wears it, with it selected.
  // Both pickers resolve by name, so a paint the model doesn't pack (a loose `.pnt`
  // dropped beside a `.pkz`) simply leaves the picker where it was.
  const wearer = isPaint ? wearerOf(entry, entries) : undefined;
  const onWearer = wearer ? entryViewerProps(wearer, entries, bikePreview) : null;
  if (onWearer) {
    return {
      ...onWearer,
      ...(entry.category === "goggles"
        ? { initialGoggles: displayName(entry.name) }
        : { initialPaint: displayName(entry.name) }),
    };
  }

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
  // A loose gear paint whose model isn't installed previews on the game's stock model.
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
