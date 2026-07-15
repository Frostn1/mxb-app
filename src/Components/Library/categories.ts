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
  Package,
  type LucideIcon,
} from "lucide-react";
import type { LibraryCategory } from "../../types";

/** Human label per library category (section headers + detail "Type"). */
export const CATEGORY_LABEL: Record<string, string> = {
  track: "Track",
  bike: "Bike",
  bikePaint: "Livery",
  bikeModelSwap: "Model swap",
  helmet: "Helmet",
  helmetPaint: "Helmet paint",
  goggles: "Goggles",
  boots: "Boots",
  bootPaint: "Boot paint",
  protection: "Protection",
  protectionPaint: "Protection paint",
  gloves: "Gloves",
  outfit: "Outfit / kit",
  misc: "Other",
};

/** Section header label per category, used when grouping the Rider tab. */
export const SECTION_LABEL: Record<string, string> = {
  ...CATEGORY_LABEL,
  bikePaint: "Liveries",
  bikeModelSwap: "Model swaps",
  helmet: "Helmets",
  helmetPaint: "Helmet paints",
  boots: "Boots",
  bootPaint: "Boot paints",
  protection: "Protection",
  protectionPaint: "Protection paints",
  gloves: "Gloves",
  outfit: "Outfit / kit",
};

export const CATEGORY_ICON: Record<string, LucideIcon> = {
  track: Mountain,
  bike: Bike,
  bikePaint: Palette,
  bikeModelSwap: Shuffle,
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
