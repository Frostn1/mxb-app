/**
 * Whether a purchase installs itself, and what the app last saw on each store.
 *
 * Neither store tells us when someone buys something: there is no webhook, no callback and
 * nothing to poll that is cheaper than reading the purchases page itself. The one moment the
 * app *does* know a purchase may be coming is when it hands a product page to the browser —
 * so that is what starts a watch, and this is the switch that says whether it may.
 *
 * Stored in `localStorage` rather than the config file for the same reason the download and
 * ad-support preferences are: it is a preference about this window's behaviour, nothing in
 * Rust reads it, and it must not add a write to a config the game half of the app shares.
 */

import type { StoreId } from "@/api/shop";

const PREFS_KEY = "frost-auto-install";
const COUNTS_KEY = "frost-store-counts";

/** On by default. Someone who has just paid for a mod wants it installed; the switch is there
 *  for the player who would rather choose the folder every time. */
export const DEFAULT_AUTO_INSTALL = true;

export function readAutoInstall(): boolean {
  try {
    // Anything but an explicit `false` reads as on, so a corrupt value fails toward the
    // default rather than silently turning the feature off.
    return localStorage.getItem(PREFS_KEY) !== "off";
  } catch {
    return DEFAULT_AUTO_INSTALL;
  }
}

export function writeAutoInstall(on: boolean): void {
  try {
    localStorage.setItem(PREFS_KEY, on ? "on" : "off");
  } catch {
    // A browser that refuses storage just means the default (on) next launch.
  }
}

/** How many purchases each store held the last time the app managed to read it. */
export type StoreCounts = Partial<Record<StoreId, number>>;

/**
 * The last count read from each store.
 *
 * Remembered because reading the shop's purchases means driving a hidden WebView through
 * Cloudflare, which takes seconds — far too much to do just so a settings row can show a
 * number. A remembered count is honest ("last time we looked") where a blocking read would
 * leave the row empty for most of the time it is on screen.
 */
export function readStoreCounts(): StoreCounts {
  try {
    const raw = localStorage.getItem(COUNTS_KEY);
    if (!raw) return {};
    const v = JSON.parse(raw) as StoreCounts;
    return v && typeof v === "object" ? v : {};
  } catch {
    return {};
  }
}

export function writeStoreCount(store: StoreId, count: number): void {
  try {
    localStorage.setItem(COUNTS_KEY, JSON.stringify({ ...readStoreCounts(), [store]: count }));
  } catch {
    // Not worth failing a refresh over.
  }
}

/** Forget a store's count — on sign-out, where the number stops being about anyone. */
export function forgetStoreCount(store: StoreId): void {
  try {
    const next = readStoreCounts();
    delete next[store];
    localStorage.setItem(COUNTS_KEY, JSON.stringify(next));
  } catch {
    // Same.
  }
}
