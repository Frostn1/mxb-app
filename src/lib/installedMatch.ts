/**
 * Deciding whether a catalog listing is already on disk.
 *
 * The two sides never agree on spelling: a mxb-mods post title is prose — author
 * credits, release tags, "(READ INSTRUCTIONS)" — while the file the author packaged
 * is named whatever they felt like. Comparing them as equal strings badges almost
 * nothing (see issue #26: 0 of 96 bikes), so this widens the comparison in three
 * steps, each one only firing when it can't plausibly be a coincidence.
 */

import { normalizeModName } from "../api/mods";

/** Packaged-file extensions, stripped before a name is read as words. */
const EXT = /\.(pkz|zip|rar|7z|pnt)$/i;

/**
 * Words that carry no identity — they show up in half the catalog, so counting them
 * as agreement makes unrelated mods look alike. Kept out of the token comparison;
 * the whole-string comparison still sees them.
 */
const FILLER = new Set([
  "the", "and", "for", "with", "by", "of", "to", "in", "on",
  // Category noise: nearly every bike post says "bike", every track says "track".
  "mx", "mxb", "mxbikes", "bike", "bikes", "track", "tracks", "rider", "gear",
  "paint", "paints", "livery", "liveries", "skin", "skins", "helmet", "helmets",
  "boot", "boots", "glove", "gloves", "kit", "gfx", "design", "designs",
  // Release/version chatter.
  "read", "instructions", "instruction", "pub", "public", "beta", "wip", "final",
  "release", "released", "rerelease", "re", "update", "updated", "fix", "fixed",
  "remake", "reworked", "rework", "edit", "version", "ver", "hd", "new", "old",
  "rd", "round", "pack", "mod", "final",
]);

/**
 * Model years are written both ways across the catalog — `2026 Yamaha CLUBMX` on the
 * post, `27 FOX Pawtector Gloves` on the next one, and the packaged file picks whichever
 * the author preferred. Folding a plausible model year onto its two-digit form lets those
 * agree. Bounded to 2000–2035 so displacements (`85`, `250`, `450`) are never touched,
 * and years stay distinguishing: `2026` and `2027` still disagree.
 */
function canonicalToken(t: string): string {
  if (t.length === 4 && /^20[0-3][0-9]$/.test(t)) return t.slice(2);
  return t;
}

/**
 * Alphanumeric runs of 2+ chars, filler dropped. `"WDR: SX '26"` → `["wdr", "sx", "26"]`.
 *
 * The extension goes first: tokenizing `livery.pnt` yields a `pnt` no post title can ever
 * contain, and since it counts toward the union it drags down the score of every paint —
 * the very category that was scoring zero.
 */
function significantTokens(s: string): Set<string> {
  const out = new Set<string>();
  for (const t of s.toLowerCase().replace(EXT, "").split(/[^a-z0-9]+/)) {
    if (t.length >= 2 && !FILLER.has(t)) out.add(canonicalToken(t));
  }
  return out;
}

/**
 * Shortest key we'll accept as evidence on its own. Below this, substrings collide by
 * chance — `"sx"` sits inside a third of the bike catalog.
 */
const MIN_CONTAINED_LEN = 6;

/**
 * A filename that reduces to one distinctive word may still stand for the whole mod —
 * `HIDEAWAY_MX.pkz` for the post "HIDEAWAY MX – SC Designs" — but only if that word is
 * long enough to be a name rather than a category. Measured on the live catalog: at this
 * length the author-credit stubs (`Replica`, `Ben89`, `ZD`, `RED`) stay out, while the
 * real one-word mod names come back in.
 */
const MIN_LONE_WORD_LEN = 10;

/**
 * How much of the *combined* vocabulary the two names must share. Union-based (Jaccard)
 * rather than "fraction of the shorter side" on purpose: two liveries for the same bike
 * by the same author share their bike and author tokens and would sail through the latter.
 *
 * Set high, because this is the blunt rule. Measured against the live catalog, a
 * white/blue and a red/white livery of the same bike still score 0.67, and a Washougal
 * and a RedBud kit for the same 2026 Yamaha score 0.60 — names differing only in the word
 * that matters. So this fires on near-identical word sets, and the subset rule below
 * carries the abbreviation cases it now leaves behind.
 */
const MIN_TOKEN_OVERLAP = 0.75;

/** At least this many shared tokens before overlap is meaningful at all. */
const MIN_SHARED_TOKENS = 2;

/**
 * The other real shape: the filename is an abbreviation of the post title, so every word
 * it has appears in the title but the title says more —
 * `ktm 85 2027 with hgs front and back pipe` ← `KTM_85_2027_HGS.pnt`. Scored as overlap
 * that's only 0.57, because the words the title adds all count against it.
 *
 * Accepted when one side's words are wholly contained in the other's, there are enough of
 * them to be distinctive, and they still account for half the longer name. The half rule
 * is what stops a two-word stub from claiming a much longer title.
 */
const MIN_SUBSET_TOKENS = 3;
const MIN_SUBSET_COVERAGE = 0.5;

function subsetMatch(a: Set<string>, b: Set<string>, shared: number): boolean {
  const [small, large] = a.size <= b.size ? [a, b] : [b, a];
  if (shared !== small.size) return false;
  return small.size >= MIN_SUBSET_TOKENS && small.size >= MIN_SUBSET_COVERAGE * large.size;
}

interface Indexed {
  key: string;
  tokens: Set<string>;
}

export interface InstalledIndex {
  /** How many installed items back this index (0 = nothing scanned / scan failed). */
  readonly size: number;
  /** Whether a catalog title looks like something already on disk. */
  has(title: string): boolean;
}

function jaccard(a: Set<string>, b: Set<string>): { shared: number; score: number } {
  let shared = 0;
  for (const t of a) if (b.has(t)) shared++;
  const union = a.size + b.size - shared;
  return { shared, score: union === 0 ? 0 : shared / union };
}

/**
 * Build a lookup over installed file/folder names (`LibraryEntry.name` — packed
 * `.pkz`, extracted track folders, and `.pnt` paints alike).
 *
 * Candidates are narrowed through an inverted token index, so checking a card costs
 * the handful of entries that share a word with it rather than the whole library.
 */
export function buildInstalledIndex(names: Iterable<string>): InstalledIndex {
  const entries: Indexed[] = [];
  const keys = new Set<string>();
  const byToken = new Map<string, number[]>();

  for (const name of names) {
    const key = normalizeModName(name);
    if (!key) continue;
    keys.add(key);
    const tokens = significantTokens(name);
    const i = entries.push({ key, tokens }) - 1;
    for (const t of tokens) {
      const bucket = byToken.get(t);
      if (bucket) bucket.push(i);
      else byToken.set(t, [i]);
    }
  }

  const has = (title: string): boolean => {
    const key = normalizeModName(title);
    if (!key) return false;
    if (keys.has(key)) return true;

    const tokens = significantTokens(title);
    const candidates = new Set<number>();
    for (const t of tokens) {
      for (const i of byToken.get(t) ?? []) candidates.add(i);
    }

    for (const i of candidates) {
      const entry = entries[i];

      // One name spelled inside the other. The two directions are not equally trustworthy,
      // so they carry different bars:
      //
      // *title inside filename* is the author prefixing their own name to the post title
      //   — `Farm Mx` inside `Feulatracks_Farm_MX`. Reliable even for a one-word title.
      // *filename inside title* is a fragment of the title, which may just be a common
      //   word: a paint called `Replica.pnt` otherwise claimed every "… Replica" post in
      //   the catalog. Needs two distinctive words, or one long enough to be a name.
      const titleInFile = entry.key.includes(key);
      const fileInTitle = key.includes(entry.key);
      if (titleInFile && key.length >= MIN_CONTAINED_LEN && tokens.size >= 1) return true;
      if (
        fileInTitle &&
        entry.key.length >= MIN_CONTAINED_LEN &&
        (entry.tokens.size >= 2 || entry.key.length >= MIN_LONE_WORD_LEN)
      )
        return true;

      const { shared, score } = jaccard(tokens, entry.tokens);
      if (shared >= MIN_SHARED_TOKENS && score >= MIN_TOKEN_OVERLAP) return true;
      if (subsetMatch(tokens, entry.tokens, shared)) return true;
    }
    return false;
  };

  return { size: entries.length, has };
}

/** Stand-in for "nothing scanned yet" — matches nothing. */
export const EMPTY_INSTALLED_INDEX: InstalledIndex = buildInstalledIndex([]);
