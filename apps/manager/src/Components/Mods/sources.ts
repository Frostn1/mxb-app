import type { ModSort, ModType } from "@frost/shared/api/mods";
import type {
  HubSort,
  ModSummary,
  ShopCategory,
  ShopMod,
  ShopPrice,
  ShopSort,
} from "@frost/shared/types";
import type { TKey } from "@/i18n";

/**
 * The three catalogues the Mods screen shows, and the one shape its grid renders.
 *
 * mxb-mods.com, MXB Hub and mxbikes-shop are three answers to one question — "where do I get
 * this mod" — so the screen treats the store as a *filter*, not as a place you navigate to.
 * That only works if the three listings can sit in one grid, which is what `MergedMod` is for.
 */

/** What the source filter offers. `all` merges whichever of the three are reachable. */
export type ModSource = "all" | "mods" | "hub" | "shop";

/** The two paid catalogues. Both are gated on `caps.shop`; only the shop can also be absent
 *  from a build (no credential — see `shopCatalogAvailable`). */
export type StoreId = "hub" | "shop";

export const SOURCE_LABELS: Record<ModSource, TKey> = {
  all: "mods.sourceAll",
  mods: "mods.sourceMods",
  hub: "nav.hub",
  shop: "nav.shop",
};

/**
 * One listing, whichever catalogue produced it.
 *
 * Deliberately *not* a union of `ModSummary | ShopMod`: the grid would then branch on every
 * field it touches. What the card needs is the same four facts from all three — art, name,
 * who made it, when it last changed — plus a price where there is one.
 *
 * There is no size here, and that is not an omission. Neither store publishes a file size:
 * `ShopMod`, `HubMod` and the Rust structs behind them carry none. A size is known only once
 * a transfer is running, and afterwards from the download history and the library ledger.
 */
export interface MergedMod {
  /** Unique across the merged grid — the two stores number their products independently. */
  key: string;
  source: Exclude<ModSource, "all">;
  id: number;
  title: string;
  image: string | null;
  author: string | null;
  /** Unix seconds, or null where the catalogue didn't say. */
  updated: number | null;
  /** Store items only. A mxb-mods listing is free by definition, which the card says in words. */
  price: ShopPrice | null;
  /** The listing's own page on the site that sold it. Null when the URL failed origin checks. */
  url: string | null;
  /** mxb-mods only — what the grid needs to open its page and queue an install. */
  mod?: ModSummary;
}

export function fromModSummary(m: ModSummary): MergedMod {
  const ts = Date.parse(m.date);
  return {
    key: `mods:${m.id}`,
    source: "mods",
    id: m.id,
    title: m.title,
    image: m.image,
    author: m.author ?? null,
    updated: Number.isNaN(ts) ? null : Math.floor(ts / 1000),
    price: null,
    url: m.link,
    mod: m,
  };
}

export function fromStoreMod(store: StoreId, m: ShopMod): MergedMod {
  return {
    key: `${store}:${m.id}`,
    source: store,
    id: m.id,
    title: m.title,
    image: m.image,
    author: m.author,
    updated: m.updated,
    price: m.price,
    url: m.url,
  };
}

/* ── Ordering ──────────────────────────────────────────────────────────────────────── */

/**
 * One order for three catalogues that each name their own.
 *
 * Each source gets the nearest thing it actually supports, and the merged grid re-sorts what
 * comes back in the browser — without that, "All sources" would simply be mxb-mods followed by
 * the two stores, whatever the control said.
 */
export type ModsSort = "newest" | "oldest" | "popular" | "nameAsc" | "priceAsc" | "priceDesc";

export const MODS_SORTS: { value: ModsSort; label: TKey; sources: Exclude<ModSource, "all">[] }[] =
  [
    { value: "newest", label: "browseSort.newest", sources: ["mods", "hub", "shop"] },
    { value: "oldest", label: "browseSort.oldest", sources: ["mods"] },
    { value: "popular", label: "browseSort.popularAll", sources: ["mods", "hub"] },
    { value: "nameAsc", label: "shopSort.nameAsc", sources: ["hub", "shop"] },
    // Price orders are offered only when something on screen has a price.
    { value: "priceAsc", label: "shopSort.priceAsc", sources: ["hub", "shop"] },
    { value: "priceDesc", label: "shopSort.priceDesc", sources: ["hub", "shop"] },
  ];

export function toModSort(sort: ModsSort): ModSort {
  if (sort === "oldest") return "oldest";
  if (sort === "popular") return "popularAll";
  return "newest";
}

export function toShopSort(sort: ModsSort): ShopSort {
  if (sort === "nameAsc") return "nameAsc";
  if (sort === "priceAsc") return "priceAsc";
  if (sort === "priceDesc") return "priceDesc";
  // The dump carries no publish date, so "newest" and "oldest" both land on `recentlyUpdated`.
  return "recentlyUpdated";
}

export function toHubSort(sort: ModsSort): HubSort {
  if (sort === "popular") return "popular";
  if (sort === "nameAsc") return "nameAsc";
  if (sort === "priceAsc") return "priceAsc";
  if (sort === "priceDesc") return "priceDesc";
  return "newest";
}

/** What an item costs right now, for ordering. Free and unpriced both sort as zero. */
function priceOf(item: MergedMod): number {
  const p = item.price;
  if (!p || p.free) return 0;
  return p.sale ?? p.base ?? 0;
}

/** Interleaves the catalogues, so "All sources" is one list rather than three stacked. */
export function sortMerged(items: MergedMod[], sort: ModsSort): MergedMod[] {
  const out = [...items];
  switch (sort) {
    case "oldest":
      return out.sort((a, b) => (a.updated ?? 0) - (b.updated ?? 0));
    case "nameAsc":
      return out.sort((a, b) => a.title.localeCompare(b.title));
    case "priceAsc":
      return out.sort((a, b) => priceOf(a) - priceOf(b));
    case "priceDesc":
      return out.sort((a, b) => priceOf(b) - priceOf(a));
    // "Popular" has no shared meaning across three sites, so the merge keeps each source's
    // own idea of it and falls back to newest for the interleave.
    default:
      return out.sort((a, b) => (b.updated ?? 0) - (a.updated ?? 0));
  }
}

/* ── Type ↔ store category ─────────────────────────────────────────────────────────── */

/**
 * Which of a store's own categories a browse type means.
 *
 * Matched on the category's slug or name, because the three catalogues number their taxonomies
 * independently and nothing relates the ids. A top-level match wins, because it usually owns
 * the whole subtree the player means. MXB Hub does not make every useful type a root, though:
 * `Tracks` lives beneath `Free Mods`. Falling back to a nested match keeps that real category
 * filterable instead of silently querying the entire store. No match still means the store is
 * queried unnarrowed rather than hidden altogether.
 */
const TYPE_HINTS: Record<string, RegExp> = {
  tracks: /track/i,
  bikes: /bike|liver|sound/i,
  rider: /rider|helmet|gear|boot|glove|suit|protect/i,
  reshade: /reshade|shader|preset/i,
  misc: /misc|plugin|tool/i,
};

export function storeRootFor(
  modType: ModType,
  categories: ShopCategory[],
): ShopCategory | undefined {
  const hint = TYPE_HINTS[modType.id];
  if (!hint) return undefined;
  const matches = (category: ShopCategory) =>
    hint.test(category.slug) || hint.test(category.name);
  return (
    categories.find((category) => category.depth === 0 && matches(category)) ??
    categories.find(matches)
  );
}

/**
 * The real category branches that make up one app-level type.
 *
 * MXB Hub has no single Bikes parent: its bike catalogue is split between setups, blends,
 * modelswaps, paints and source files. Other stores retain their single-root behaviour.
 */
const HUB_TYPE_SLUGS: Partial<Record<string, Set<string>>> = {
  tracks: new Set(["tracks"]),
  bikes: new Set([
    "bike-psds",
    "bike-setups",
    "blends",
    "modelswaps",
    "bike-paints",
    "bike-pnt-creation",
  ]),
  rider: new Set(["gear-psds", "rider-protection", "gear-pnt-creation"]),
  reshade: new Set(),
  misc: new Set(),
};

export function storeRootsFor(
  store: StoreId,
  modType: ModType,
  categories: ShopCategory[],
): ShopCategory[] {
  const hubSlugs = store === "hub" ? HUB_TYPE_SLUGS[modType.id] : undefined;
  if (hubSlugs !== undefined) {
    return categories.filter((category) => hubSlugs.has(category.slug));
  }
  const root = storeRootFor(modType, categories);
  return root ? [root] : [];
}
