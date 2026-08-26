/**
 * Who's funding MXB App on Buy Me a Coffee, and where that list comes from.
 *
 * The list changes between releases, and a credits page that only refreshes when
 * someone ships a build would thank a new supporter weeks late — so the names live in
 * `supporters.json` on `main` and are fetched at runtime. Adding somebody is a one-line
 * commit to that file, not a release.
 *
 * Three layers, in the order they're preferred:
 *   1. the remote manifest, fetched every time the section opens;
 *   2. the last remote answer, cached in webview storage so an offline launch still
 *      shows the real list rather than an empty one;
 *   3. `BUNDLED_SUPPORTERS`, which is what a build ships with when it has never once
 *      reached the network.
 */

/**
 * The Buy Me a Coffee page every button here opens.
 *
 * The handle is the auto-generated one the account was created with, not a word — it's
 * meant to be opened, never typed, so don't "tidy" it. `supportUrl` in the manifest
 * overrides it, which is what to reach for if the page ever moves: a shipped build
 * can't be told about a new link, but the manifest can.
 */
export const SUPPORT_URL = "https://buymeacoffee.com/22ypnh5yfme";

/** Read from `main`, not from a tag: the list has to move faster than releases do. */
export const SUPPORTERS_URL =
  "https://raw.githubusercontent.com/Frostn1/mxb-app/main/supporters.json";

export interface Supporter {
  /** Display name, as they want it shown — not their Buy Me a Coffee account name. */
  name: string;
  /** Membership level, verbatim. Level names are the creator's own words, so they're
   *  never translated; a level nobody is in simply doesn't render. Someone who bought a
   *  one-off coffee has none, and lands in the ungrouped list at the bottom. */
  tier?: string;
  /** ISO date they started supporting, shown as "since Jul 2026" when present. */
  since?: string;
}

export interface SupportersManifest {
  /** Overrides {@link SUPPORT_URL} when set — how a moved page gets fixed without a
   *  release. */
  supportUrl?: string;
  /** Membership levels, best first. Anything not listed here still renders; it just
   *  sorts after the named ones. */
  tiers: string[];
  supporters: Supporter[];
}

/**
 * What a build ships with — the list as it stood when the build was cut.
 *
 * Only ever seen by an install that has never once reached the network; the manifest
 * replaces it wholesale on the first successful fetch. It exists so a fresh offline
 * launch thanks the people who are already there rather than claiming nobody is.
 */
export const BUNDLED_SUPPORTERS: SupportersManifest = {
  tiers: [],
  supporters: [
    { name: "OHTEA - MXB HUB" },
    { name: "HottPie" },
    { name: "Mouk" },
    { name: "SoggySwisher" },
    { name: "LupaHo" },
    { name: "Qwest" },
    { name: "RodaksRevivalYT | Black Rifle" },
    { name: "MintyFlow" },
    { name: "Bøddi" },
    { name: "Kelso" },
  ],
};

const CACHE_KEY = "mxb:supporters:v1";
/** Long enough for a cold CDN, short enough that Settings isn't held open waiting. */
const FETCH_TIMEOUT_MS = 6000;
/** Caps on remote data. Nothing hostile is expected from our own repo, but a file with
 *  a stray million-character name shouldn't be able to wedge the renderer. */
const MAX_SUPPORTERS = 500;
const MAX_NAME_CHARS = 48;
const MAX_TIER_CHARS = 32;

function trimmed(value: unknown, max: number): string | undefined {
  if (typeof value !== "string") return undefined;
  const s = value.trim().slice(0, max);
  return s || undefined;
}

/** Validate whatever the network handed us into something this UI can render. */
export function parseManifest(raw: unknown): SupportersManifest | null {
  if (!raw || typeof raw !== "object") return null;
  const obj = raw as Record<string, unknown>;
  if (!Array.isArray(obj.supporters)) return null;

  const supporters: Supporter[] = [];
  for (const entry of obj.supporters.slice(0, MAX_SUPPORTERS)) {
    // A bare string is allowed in the manifest — most entries are just a name, and
    // `["Alex", "Sam"]` is a nicer thing to hand-edit than a list of one-key objects.
    const name =
      typeof entry === "string"
        ? trimmed(entry, MAX_NAME_CHARS)
        : trimmed((entry as Record<string, unknown>)?.name, MAX_NAME_CHARS);
    if (!name) continue;
    const fields = (typeof entry === "object" && entry ? entry : {}) as Record<
      string,
      unknown
    >;
    supporters.push({
      name,
      tier: trimmed(fields.tier, MAX_TIER_CHARS),
      since: trimmed(fields.since, 32),
    });
  }

  const tiers = Array.isArray(obj.tiers)
    ? obj.tiers
        .map((t) => trimmed(t, MAX_TIER_CHARS))
        .filter((t): t is string => Boolean(t))
    : [];

  return {
    supportUrl: trimmed(obj.supportUrl, 200),
    tiers,
    supporters,
  };
}

export function readCachedManifest(): SupportersManifest | null {
  try {
    const raw = localStorage.getItem(CACHE_KEY);
    return raw ? parseManifest(JSON.parse(raw)) : null;
  } catch {
    return null;
  }
}

function writeCachedManifest(manifest: SupportersManifest) {
  try {
    localStorage.setItem(CACHE_KEY, JSON.stringify(manifest));
  } catch {
    // Storage full or blocked — the list still renders this session, it just won't
    // survive a restart offline. Not worth telling anyone about.
  }
}

/** Fetch the live list. Throws on anything that isn't a usable manifest. */
export async function fetchSupporters(): Promise<SupportersManifest> {
  const abort = new AbortController();
  const timer = setTimeout(() => abort.abort(), FETCH_TIMEOUT_MS);
  try {
    const res = await fetch(SUPPORTERS_URL, {
      signal: abort.signal,
      cache: "no-cache",
    });
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const manifest = parseManifest(await res.json());
    if (!manifest) throw new Error("malformed supporters.json");
    writeCachedManifest(manifest);
    return manifest;
  } finally {
    clearTimeout(timer);
  }
}

export interface TierGroup {
  /** `null` for supporters with no tier of their own. */
  tier: string | null;
  people: Supporter[];
}

/**
 * Bucket supporters by membership level, in the manifest's order.
 *
 * A level the manifest doesn't name still shows up — sorted after the named ones, so
 * renaming one on Buy Me a Coffee degrades to "listed last" rather than "silently
 * dropped". People with no level — a one-off coffee — always come last, under a
 * generic heading.
 *
 * Within a level, people keep the order they're written in. The manifest is hand-edited,
 * so that order is a decision somebody made; re-sorting here would quietly overrule it.
 */
export function groupByTier(manifest: SupportersManifest): TierGroup[] {
  const buckets = new Map<string, Supporter[]>();
  const untiered: Supporter[] = [];
  for (const s of manifest.supporters) {
    if (!s.tier) {
      untiered.push(s);
      continue;
    }
    const bucket = buckets.get(s.tier);
    if (bucket) bucket.push(s);
    else buckets.set(s.tier, [s]);
  }

  const named = manifest.tiers.filter((t) => buckets.has(t));
  const rest = [...buckets.keys()]
    .filter((t) => !manifest.tiers.includes(t))
    .sort((a, b) => a.localeCompare(b));

  const groups: TierGroup[] = [...named, ...rest].map((tier) => ({
    tier,
    people: buckets.get(tier) ?? [],
  }));
  if (untiered.length) groups.push({ tier: null, people: untiered });
  return groups;
}
