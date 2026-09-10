/**
 * Finding a missing track for download.
 *
 * When a server names a track that isn't on disk, we look for it across the three catalogs the
 * app already knows — mxb-mods (free), MXB Hub, and mxbikes-shop (paid) — in that order, and
 * return the first confident match. Free/owned matches can be queued straight into the install
 * pipeline; a paid match comes back with a price tag and a link to buy, never an auto-purchase.
 *
 * Matching an internal track id (`2026_ARLSX_RD14`) to a prose catalog title ("2026 ARL SX —
 * Round 14") is fuzzy, so it reuses the same token scorer the Library uses to badge installed
 * mods ({@link buildInstalledIndex}): we index the track name and ask whether each catalog
 * title "is" it. Below that bar we return `none` rather than guess, and the UI offers a manual
 * search instead.
 */
import { searchMods } from "@frost/shared/api/mods";
import { hubSearch } from "@/api/hub";
import { shopCatalogSearch, formatPrice } from "@/api/shop";
import type { HubMod, ModSummary, ShopMod } from "@frost/shared/types";
import { buildInstalledIndex } from "./installedMatch";

/** mxb-mods' "Tracks" category — `MOD_TYPES`' tracks `categoryId`. Not 29: that is the
 *  *bikes* category, and searching it for a track name matched liveries or nothing at all. */
const TRACKS_CATEGORY = 22;

export type TrackSource =
  | { kind: "mxbMods"; title: string; mod: ModSummary }
  | { kind: "hub"; title: string; mod: HubMod; price: string | null; free: boolean }
  | { kind: "shop"; title: string; mod: ShopMod; price: string | null }
  | { kind: "none" };

/**
 * The same id with its volatile parts dropped, or `null` when that changes nothing.
 *
 * Catalog search is an AND over the terms (`shop_catalog::select`: "every term must match"),
 * so one word the product spells differently hides it completely. The two that differ most
 * often are the ones with no settled spelling: a round written `RD03` on the server and `RD3`
 * on the post, and a season year the post may not carry at all. Dropping them leaves the part
 * that actually names the track (`ARLMX Lakewood`), which is what a person would have typed.
 *
 * This only widens what gets *retrieved*. Whether a candidate really is this track is still
 * settled by the scorer, which keeps the year and round it just searched without.
 */
function looseTrackQuery(trackId: string): string | null {
  const words = trackQuery(trackId).split(/\s+/).filter(Boolean);
  const kept = words.filter((w) => !/^(?:20[0-3][0-9]|rd[0-9]+|r[0-9]+)$/i.test(w));
  if (kept.length === 0 || kept.length === words.length) return null;
  return kept.join(" ");
}

/** A readable label for the internal track id — the id itself, since that's what hosts and
 *  catalogs both derive from and it's what the player will recognize in a search box. */
export function trackQuery(trackId: string): string {
  // Internal ids are often `Snake_Or-Dashed`; a space-separated form searches far better.
  return trackId.replace(/[_-]+/g, " ").trim();
}

/** Look for a missing track. `locale` is only for formatting any price found. */
export async function resolveMissingTrack(
  trackId: string,
  locale: string,
): Promise<TrackSource> {
  const query = trackQuery(trackId);
  if (!query) return { kind: "none" };

  // The scorer, seeded with the track id so we can ask "does this catalog title match it?".
  // Both the raw id and the spaced query go in, so either spelling can hit.
  const index = buildInstalledIndex([trackId, query]);
  const matches = (title: string) => index.has(title);

  // The exact id first, then the same id without its round and year. The second pass is what
  // reaches a post that spells either of those differently; both are scored the same way, so
  // widening the search never widens what counts as a match.
  const loose = looseTrackQuery(trackId);
  const queries = loose === null ? [query] : [query, loose];

  /** First candidate any of the queries turns up that the scorer accepts. */
  const firstMatch = async <T>(
    search: (q: string) => Promise<T[]>,
    title: (item: T) => string,
  ): Promise<T | null> => {
    for (const q of queries) {
      try {
        const hit = (await search(q)).find((item) => matches(title(item)));
        if (hit) return hit;
      } catch {
        // This catalog is unreachable or refused the query; the next one still gets a turn.
      }
    }
    return null;
  };

  // 1. mxb-mods — free. The best outcome: one click and it's queued.
  {
    const hit = await firstMatch(
      (q) => searchMods(q, TRACKS_CATEGORY, 1),
      (m) => m.title,
    );
    if (hit) return { kind: "mxbMods", title: hit.title, mod: hit };
  }

  // 2. MXB Hub — may be free or paid.
  {
    let currency = "USD";
    const hit = await firstMatch(
      async (q) => {
        const page = await hubSearch(q, null, 1, "newest", false);
        currency = page.currency;
        return page.items;
      },
      (m) => m.title,
    );
    if (hit) {
      const price = hit.price.base;
      const free = !price || price <= 0;
      return {
        kind: "hub",
        title: hit.title,
        mod: hit,
        free,
        price: free ? null : formatPrice(price, currency, locale),
      };
    }
  }

  // 3. mxbikes-shop — paid. We can't download it, but we can show the price and link out.
  //
  // Searched, not looked up by name. This used `shopMatchCatalog`, which is the *exact*
  // title matcher the purchases page needs to map a scraped product name onto its catalog
  // entry, folding case and punctuation and nothing else. A track id almost never spells
  // its product title character for character (`2026_ARLMX_RD03_Lakewood` against
  // "2026 ARLMX RD3 Lakewood"), so that lookup returned null for essentially every track and
  // the paid route never fired at all. A search puts candidates in front of the same scorer
  // the other two catalogs use, which is what decides whether one is really this track.
  {
    let currency = "USD";
    const hit = await firstMatch(
      async (q) => {
        const page = await shopCatalogSearch(q, null, 1, "newest", false);
        currency = page.currency;
        return page.items;
      },
      (m) => m.title,
    );
    if (hit) {
      const price = hit.price.onSale ? hit.price.sale : hit.price.base;
      return {
        kind: "shop",
        title: hit.title,
        mod: hit,
        price: formatPrice(price, currency, locale),
      };
    }
  }

  return { kind: "none" };
}
