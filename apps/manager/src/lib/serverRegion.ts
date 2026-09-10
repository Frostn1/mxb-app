/**
 * Making sense of the region a server reports.
 *
 * The field is free text the host typed into their own config, and it shows: across one live
 * list of 604 servers, 371 left it blank and the rest spelled the same handful of places a
 * dozen ways - `USA (East)`, `US East`, `North Carolina`; `Europe (Germany)`, `Europe /
 * Germany`, `Frankfurt, Germany`, `de` - alongside entries that name no place at all
 * (`forest`, `redbud mx`, `123`, `YOUR MOMS`). Listing those raw in a filter gives a dropdown
 * of nonsense with the real regions scattered through it.
 *
 * So the filter groups by {@link canonicalRegion}: a small fixed set of buckets, matched on
 * keywords, with everything unrecognised falling into `other`. The *row* still shows what the
 * host wrote when it means something, because "Europe (Germany)" tells a player more than
 * "Europe" does - {@link regionLabel} is the one that decides that.
 *
 * The upstream `MasterServer` carries the host's text as `location`, so callers pass that in.
 *
 * Deliberately not a geo-IP lookup. That would be a request per server for a field nobody
 * sorts by, and it would still disagree with the host's own label.
 */

import type { TKey } from "@/i18n";

/** The buckets the filter offers. `other` collects blanks and anything unrecognised. */
export type RegionKey =
  | "na-east"
  | "na-west"
  | "na"
  | "europe"
  | "oceania"
  | "south-america"
  | "asia"
  | "other";

/** i18n keys, so the dropdown reads in the user's language rather than the host's. */
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

/** The order the dropdown lists them in - busiest regions first, `other` last. */
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

/** Whole-word match, so `de` (Germany) doesn't fire on "Meadow" and `us` doesn't on "Aussie". */
function has(text: string, ...words: string[]): boolean {
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
// US states and cities hosts actually name, split by coast where the name settles it.
const NA_EAST = [
  "east", "carolina", "virginia", "york", "jersey", "florida", "georgia", "ohio", "michigan",
  "maryland", "pennsylvania", "massachusetts", "toronto", "montreal", "miami", "atlanta",
  "chicago", "dallas", "texas",
];
const NA_WEST = ["west", "california", "oregon", "washington", "seattle", "angeles", "vegas", "phoenix", "denver"];
const NA = ["usa", "us", "united", "states", "america", "american", "canada", "mexico", "mex", "na"];

/**
 * Which bucket a host's region string belongs to.
 *
 * Coast before country, so `USA (East)` lands in `na-east` rather than the generic `na`;
 * Europe before North America, because `EU` would otherwise be caught by nothing and
 * `Germany`'s `de` is checked as a whole word.
 */
export function canonicalRegion(raw: string): RegionKey {
  const text = raw.toLowerCase().trim();
  if (!text || text === "unknown" || text === "?") return "other";
  if (has(text, ...EUROPE)) return "europe";
  if (has(text, ...OCEANIA)) return "oceania";
  if (has(text, ...ASIA)) return "asia";
  if (has(text, ...SOUTH_AMERICA)) return "south-america";
  if (has(text, ...NA_EAST)) return "na-east";
  if (has(text, ...NA_WEST)) return "na-west";
  if (has(text, ...NA)) return "na";
  return "other";
}

/**
 * What to print on a row: the host's own text when it names a place we recognised, and
 * nothing at all when it doesn't.
 *
 * Keeping the raw string is the point - `Europe (Germany)` and `Australia (Sydney)` are more
 * use to a player than the bucket they fall in. But a server whose region is `YOUR MOMS` has
 * told us nothing, and printing it is worse than printing nothing.
 */
export function regionLabel(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;
  return canonicalRegion(trimmed) === "other" ? null : trimmed;
}
