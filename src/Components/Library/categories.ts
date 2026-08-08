import {
  Bike,
  Mountain,
  Palette,
  Shuffle,
  HardHat,
  Glasses,
  Footprints,
  Shield,
  Hand,
  Shirt,
  Volume2,
  Package,
  type LucideIcon,
} from "lucide-react";
import type { LibraryCategory } from "../../types";
import type { TKey } from "../../i18n";

/** Translation key per library category (section headers + detail "Type").
 *  Singular — resolved with `t()` at render. */
export const CATEGORY_LABEL: Record<string, TKey> = {
  track: "category.track",
  bike: "category.bike",
  bikePaint: "category.bikePaint",
  bikeModelSwap: "category.bikeModelSwap",
  sound: "category.sound",
  helmet: "category.helmet",
  helmetPaint: "category.helmetPaint",
  goggles: "category.goggles",
  boots: "category.boots",
  bootPaint: "category.bootPaint",
  protection: "category.protection",
  protectionPaint: "category.protectionPaint",
  gloves: "category.gloves",
  outfit: "category.outfit",
  misc: "category.misc",
};

/** Section header key per category, used when grouping the Rider tab. Plural in
 *  English; other languages pluralize their own way inside the string. */
export const SECTION_LABEL: Record<string, TKey> = {
  ...CATEGORY_LABEL,
  bikePaint: "section.bikePaint",
  bikeModelSwap: "section.bikeModelSwap",
  sound: "section.sound",
  helmet: "section.helmet",
  helmetPaint: "section.helmetPaint",
  boots: "section.boots",
  bootPaint: "section.bootPaint",
  protection: "section.protection",
  protectionPaint: "section.protectionPaint",
  gloves: "section.gloves",
  outfit: "section.outfit",
};

export const CATEGORY_ICON: Record<string, LucideIcon> = {
  track: Mountain,
  bike: Bike,
  bikePaint: Palette,
  bikeModelSwap: Shuffle,
  sound: Volume2,
  helmet: HardHat,
  helmetPaint: Palette,
  goggles: Glasses,
  boots: Footprints,
  bootPaint: Palette,
  protection: Shield,
  protectionPaint: Palette,
  gloves: Hand,
  outfit: Shirt,
  misc: Package,
};

export function categoryIcon(category: string): LucideIcon {
  return CATEGORY_ICON[category] ?? Package;
}

/** Order the Rider tab's category sections appear in. */
export const RIDER_SECTION_ORDER: LibraryCategory[] = [
  "helmet",
  "helmetPaint",
  "goggles",
  "boots",
  "bootPaint",
  "protection",
  "protectionPaint",
  "gloves",
  "outfit",
  "misc",
];
