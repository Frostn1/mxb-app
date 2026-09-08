import { invoke } from "@tauri-apps/api/core";

/** One discipline's standing — the site shows Global, MX and SX side by side. */
export interface RankCard {
  /** "Global", "MX", "SX". */
  discipline: string;
  rank: string;
  /** The short badge, "S1" for Silver 1. */
  badge: string;
  /** The badge spelled out. */
  rankName: string;
  /** The rank's own colour on mxb-ranked, so our card reads the same. */
  color: string;
  mxp: string;
  races: string;
  avgPosition: string;
  wins: string;
  podiums: string;
  wrLaps: string;
  pbLaps: string;
  holeshots: string;
}

/** One row of the last-50 table. */
export interface RaceRow {
  server: string;
  track: string;
  category: string;
  /** "2 / 3" — finishing position over starters. */
  position: string;
  bike: string;
  mxp: string;
  globalMxp: string;
  /** "up", "down", or "" when the page didn't say. */
  mxpDir: string;
  penaltyPoints: string;
  /** As the site writes it — "13 days ago". */
  finished: string;
  /** The absolute timestamp behind that. */
  finishedAt: string;
  resultsUrl: string;
}

export interface RankedProfile {
  guid: string;
  name: string;
  /** Two-letter country from the site's flag, "un" when unset. */
  country: string;
  exp: string;
  memberSince: string;
  /** A–E, the site's behaviour grade. */
  rating: string;
  penaltyPoints: string;
  globalPenaltyPoints: string;
  season: string;
  cards: RankCard[];
  races: RaceRow[];
  /** The rider's own achievement banner on mxb-ranked, absolute. Empty when they have none. */
  banner: string;
  url: string;
}

/** Which GUID the tab will ask about, and where it came from. */
export interface RankedIdentity {
  /** Empty when there is nothing to go on and the player has to type one. */
  guid: string;
  /** `"steam"` derived from the signed-in Steam account, `"manual"` typed by the player. */
  source: "steam" | "manual" | "";
}

export function rankedIdentity(): Promise<RankedIdentity> {
  return invoke<RankedIdentity>("ranked_identity");
}

/**
 * One rider's rank, standings and last 50 races off mxb-ranked.com.
 *
 * No sign-in: the profile is public and server-rendered. Omit `guid` for this machine's own.
 */
export function rankedProfile(guid?: string): Promise<RankedProfile> {
  return invoke<RankedProfile>("ranked_profile", { guid: guid ?? null });
}

/**
 * Remember a hand-entered GUID, or clear it with `""`.
 *
 * Only for a copy of MX Bikes that didn't come from Steam — every Steam copy's GUID is
 * derived. Rejects anything that isn't a GUID rather than storing it.
 */
export function setRankedGuid(guid: string): Promise<void> {
  return invoke<void>("set_ranked_guid", { guid });
}
