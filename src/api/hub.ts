import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type {
  HubCategory,
  HubModDetail,
  HubPage,
  HubSort,
} from "../types";
import type { ShopItem } from "./mods";
import type { TKey } from "../i18n/core";

/**
 * MXB Hub — `shop.mxb-hub.com`, the community marketplace `mxbhub.com` redirects to.
 *
 * Kept out of `api/shop.ts` (mxbikes-shop) and `api/mods.ts` (mxb-mods + install + library)
 * because it is a third store with its own session, not a variant of either. What it *does*
 * share is the shape of its answers: a Hub listing is a `ShopMod` with a slug, and a Hub
 * purchase is a `ShopItem`, so the grid, the price tag, the purchase card and the detail page
 * are the shop's components used as they are.
 *
 * Two halves, gated differently. Browsing needs nothing — the store's WooCommerce API is
 * public, so unlike the shop's catalog there is no build-time credential and no reason to ever
 * hide this half. Installing needs the user's own sign-in, and nothing else.
 */

/** Matches `PER_PAGE` in `src-tauri/src/mods/hub.rs`. */
export const HUB_PAGE_SIZE = 24;

/**
 * The orders the store honours. Checked against the live API rather than taken from the
 * WooCommerce docs, which list a `relevance` this install rejects outright.
 *
 * There is no "recently updated": the Store API orders by publish date only. Rather than offer
 * two entries that sort identically — the mistake `SHOP_SORTS` documents avoiding — there is
 * one, and it says what it does.
 */
export const HUB_SORTS: { value: HubSort; label: TKey }[] = [
  { value: "newest", label: "hubSort.newest" },
  { value: "popular", label: "hubSort.popular" },
  { value: "priceAsc", label: "hubSort.priceAsc" },
  { value: "priceDesc", label: "hubSort.priceDesc" },
  { value: "nameAsc", label: "hubSort.nameAsc" },
];

export function hubSearch(
  query: string,
  categoryId: number | null,
  page: number,
  sort: HubSort,
  onSaleOnly: boolean,
): Promise<HubPage> {
  return invoke<HubPage>("hub_search", {
    query,
    categoryId,
    page,
    sort,
    onSaleOnly,
  });
}

export function hubCategories(): Promise<HubCategory[]> {
  return invoke<HubCategory[]>("hub_categories");
}

export function hubDetail(id: number): Promise<HubModDetail> {
  return invoke<HubModDetail>("hub_detail", { id });
}

/** Open the store's sign-in page in a window of its own. Resolves once the window is up — the
 *  sign-in itself finishes later, and arrives on `onHubAuth`. */
export function hubLogin(): Promise<void> {
  return invoke<void>("hub_login");
}

/** Whether a Hub session is currently held. */
export function hubStatus(): Promise<boolean> {
  return invoke<boolean>("hub_status");
}

/** Sign out, here and on the store. Deliberately does not touch the mxbikes-shop session. */
export function hubLogout(): Promise<void> {
  return invoke<void>("hub_logout");
}

/** Fires when the sign-in window finishes — `true` when a session was captured. */
export function onHubAuth(handler: (ok: boolean) => void) {
  return listen<boolean>("hub-auth", (event) => handler(event.payload));
}

/**
 * What this account owns, with each row's catalog entry alongside it.
 *
 * `listings` is positional against `items`, and `null` where the product has been unlisted.
 * One call rather than the shop's two: a Hub download row links to its product page, so the
 * lookup is exact and cheap enough to do on the same round trip.
 */
export interface HubDownloads {
  items: ShopItem[];
  listings: (import("../types").HubMod | null)[];
}

export function hubMyDownloads(): Promise<HubDownloads> {
  return invoke<HubDownloads>("hub_my_downloads");
}

/** Download a file this account owns and install it where the caller chose. */
export function hubInstall(
  item: ShopItem,
  subpath: string,
  destFolder: string,
): Promise<void> {
  return invoke<void>("hub_install", { item, subpath, destFolder });
}
