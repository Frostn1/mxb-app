/**
 * Groups a server's free-text `location` into a few real regions for the filter.
 *
 * Hosts type anything there (`USA (East)`, `Frankfurt, Germany`, `de`, `redbud mx`), so the
 * filter matches keywords and drops everything unrecognised into `other`.
 */

import type { TKey } from "@/i18n";

export type RegionKey =
  | "na-east"
  | "na-west"
  | "na"
  | "europe"
  | "oceania"
  | "south-america"
  | "asia"
  | "other";

export const REGION_LABEL_KEY: Record<RegionKey, TKey> = {
  "na-east": "serverBrowser.region.naEast",
  "na-west": "serverBrowser.region.naWest",
  na: "serverBrowser.region.na",
  europe: "serverBrowser.region.europe",
  oceania: "serverBrowser.region.oceania",
  "south-america": "serverBrowser.region.southAmerica",
  asia: "serverBrowser.region.asia",
  other: "serverBrowser.region.other",
};

/** Dropdown order, `other` last. */
export const REGION_ORDER: RegionKey[] = [
  "na-east",
  "na-west",
  "na",
  "europe",
  "oceania",
  "south-america",
  "asia",
  "other",
];

/** Whole-word match, so `de` doesn't fire on "meadow". */
function has(text: string, words: string[]): boolean {
  return words.some((w) => new RegExp(`(^|[^a-z])${w}([^a-z]|$)`).test(text));
}

const EUROPE = [
  "europe", "eu", "germany", "deutschland", "de", "frankfurt", "uk", "england", "britain",
  "london", "france", "paris", "netherlands", "amsterdam", "spain", "italy", "poland",
  "hungary", "sweden", "norway", "finland", "denmark", "ireland", "portugal", "austria",
  "switzerland", "belgium", "czech", "romania",
];
const OCEANIA = ["australia", "sydney", "melbourne", "brisbane", "perth", "oceania", "nz", "zealand", "aus"];
const SOUTH_AMERICA = ["brazil", "brasil", "argentina", "chile", "peru", "colombia", "sa"];
const ASIA = ["asia", "japan", "tokyo", "singapore", "korea", "china", "india", "hong", "jp"];
const NA_EAST = [
  "east", "carolina", "virginia", "york", "jersey", "florida", "georgia", "ohio", "michigan",
  "maryland", "pennsylvania", "massachusetts", "toronto", "montreal", "miami", "atlanta",
  "chicago", "dallas", "texas",
];
const NA_WEST = ["west", "california", "oregon", "washington", "seattle", "angeles", "vegas", "phoenix", "denver"];
const NA = ["usa", "us", "united", "states", "america", "american", "canada", "mexico", "mex", "na"];

/** Which region a host's location string belongs to. Coast is checked before country. */
export function canonicalRegion(raw: string): RegionKey {
  const text = raw.toLowerCase().trim();
  if (!text || text === "unknown" || text === "?") return "other";
  if (has(text, EUROPE)) return "europe";
  if (has(text, OCEANIA)) return "oceania";
  if (has(text, ASIA)) return "asia";
  if (has(text, SOUTH_AMERICA)) return "south-america";
  if (has(text, NA_EAST)) return "na-east";
  if (has(text, NA_WEST)) return "na-west";
  if (has(text, NA)) return "na";
  return "other";
}
