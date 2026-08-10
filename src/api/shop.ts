import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import type {
  ShopCategory,
  ShopMod,
  ShopModDetail,
  ShopPage,
  ShopSort,
  ShopStatus,
} from "../types";
import type { TKey } from "../i18n/core";

/**
 * The mxbikes-shop.com catalog.
 *
 * Kept out of `api/mods.ts`, which is already the mxb-mods + install + library layer at
 * nearly a thousand lines. Nothing here installs or buys — the catalog is read-only and
 * buying is a link out to the store. Installing something *already* bought is a different
 * path entirely: it needs the user's own session, so it lives with the rest of the install
 * machinery in `api/mods.ts` (`shopMyDownloads`, `shopStage`).
 *
 * The API credential never reaches this file: every call goes through Rust, which holds the
 * token. See `src-tauri/src/shop_credentials.rs`.
 */

/** Matches `PER_PAGE` in `src-tauri/src/mods/shop_catalog.rs`. */
export const SHOP_PAGE_SIZE = 24;

/** Emitted by the backend when a background refresh brought in a newer catalog. */
export const SHOP_CATALOG_UPDATED = "shop-catalog-updated";

/**
 * `newest` is deliberately absent. The catalog carries only an `updated` date — there is no
 * publish date on any item — so "Newest" and "Recently updated" would order identically, and
 * two dropdown entries that do the same thing are worse than one honest one. The backend
 * keeps both variants (`newest` falls back to `updated`) for the day the store adds one.
 */
export const SHOP_SORTS: { value: ShopSort; label: TKey }[] = [
  { value: "recentlyUpdated", label: "shopSort.recentlyUpdated" },
  { value: "priceAsc", label: "shopSort.priceAsc" },
  { value: "priceDesc", label: "shopSort.priceDesc" },
  { value: "onSale", label: "shopSort.onSale" },
  { value: "nameAsc", label: "shopSort.nameAsc" },
];

/**
 * How the Purchases grid can be ordered.
 *
 * Its own list rather than a reuse of `SHOP_SORTS`: price and on-sale are meaningless once you
 * own the thing, and the two orderings that matter most here — when you bought it, and what you
 * haven't installed yet — have nowhere to live in the catalog's list. Sorting happens in the
 * browser; every purchase is already in memory.
 */
export type PurchaseSort = "recentlyPurchased" | "nameAsc" | "notInstalled";

export const PURCHASE_SORTS: { value: PurchaseSort; label: TKey }[] = [
  { value: "recentlyPurchased", label: "purchaseSort.recentlyPurchased" },
  { value: "nameAsc", label: "purchaseSort.nameAsc" },
  { value: "notInstalled", label: "purchaseSort.notInstalled" },
];

/** Whether this build can reach the shop at all. False hides the tab. */
export function shopCatalogAvailable(): Promise<boolean> {
  return invoke<boolean>("shop_catalog_available");
}

/** Cheap and synchronous on the Rust side — it never fetches, so polling it is free. */
export function shopCatalogStatus(): Promise<ShopStatus> {
  return invoke<ShopStatus>("shop_catalog_status");
}

export function shopCatalogCategories(): Promise<ShopCategory[]> {
  return invoke<ShopCategory[]>("shop_catalog_categories");
}

export function shopCatalogSearch(
  query: string,
  categoryId: number | null,
  page: number,
  sort: ShopSort,
  onSaleOnly: boolean,
): Promise<ShopPage> {
  return invoke<ShopPage>("shop_catalog_search", {
    query,
    categoryId,
    page,
    sort,
    onSaleOnly,
  });
}

export function shopCatalogDetail(id: number): Promise<ShopModDetail> {
  return invoke<ShopModDetail>("shop_catalog_detail", { id });
}

/**
 * The catalog entry for each product name, positionally — `null` where the store has nothing
 * by that name.
 *
 * The purchases page is scraped HTML: it gives a product name and a download link and nothing
 * else, so this is what supplies the artwork, author and store link that make the grid worth
 * looking at. Matching is exact (on the same fold the catalog search uses); a build with no
 * catalog credential answers all-`null` and the cards simply stay plain.
 */
export function shopMatchCatalog(names: string[]): Promise<(ShopMod | null)[]> {
  return invoke<(ShopMod | null)[]>("shop_match_catalog", { names });
}

/** Ignores the cache age and any stored ETag — "Refresh" has to mean refresh. */
export function shopCatalogRefresh(): Promise<ShopStatus> {
  return invoke<ShopStatus>("shop_catalog_refresh");
}

/**
 * Open a store page in the user's own browser.
 *
 * Rust already dropped any URL that wasn't https on mxbikes-shop.com, so a non-null `url`
 * has been checked once. This checks again because the cost is a line and the failure mode
 * — handing an arbitrary URL to the OS handler — is not one worth being clever about.
 */
export async function openShopUrl(url: string | null): Promise<void> {
  if (!url) return;
  try {
    const parsed = new URL(url);
    const host = parsed.hostname.toLowerCase();
    const onStore = host === "mxbikes-shop.com" || host.endsWith(".mxbikes-shop.com");
    if (parsed.protocol !== "https:" || !onStore) return;
    await open(url);
  } catch {
    // A malformed URL is not worth a toast; the button simply does nothing.
  }
}

/**
 * Money, in the catalog's currency.
 *
 * `Intl` is given the active UI locale so a German player sees "24,99 $" rather than
 * "$24.99" — the number formatting is theirs, the currency is the store's.
 */
export function formatPrice(
  value: number | null,
  currency: string,
  locale: string,
): string {
  if (value === null || !Number.isFinite(value)) return "";
  try {
    return new Intl.NumberFormat(locale, {
      style: "currency",
      currency: currency || "USD",
    }).format(value);
  } catch {
    // An unknown currency code from the dump shouldn't blank the price.
    return `${value.toFixed(2)} ${currency}`.trim();
  }
}
