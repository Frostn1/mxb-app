/**
 * Mods a player wants but hasn't taken yet.
 *
 * Per machine, like the library's stars and the server favorites, so `localStorage` rather
 * than the app config. Unlike those it keeps more than an id: a wishlist has to render months
 * later, offline, for a post that may have been edited or pulled — so the title, author and
 * picture are copied in at the moment it is added rather than fetched again.
 *
 * The list is read by two screens at once (the mod page's toggle and the Library's panel), and
 * a `storage` event never fires in the tab that wrote it, so the subscribers below are what
 * keeps the two in step.
 */
import { useCallback, useSyncExternalStore } from "react";

const KEY = "mxb:wishlist:v1";

/** Which catalog a wishlisted mod came from — it decides where "Open" goes. */
export type WishSource = "browse" | "shop" | "hub";

export interface WishItem {
  /** `<source>:<slug>` — the same mod on two stores is two entries, because it is. */
  id: string;
  source: WishSource;
  /** What the source calls it: a Browse slug, or the store's product name. */
  slug: string;
  title: string;
  author?: string;
  image?: string;
  /** Unix ms, so the list can lead with what was wanted most recently. */
  addedAt: number;
}

function read(): WishItem[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (w): w is WishItem =>
        !!w && typeof w === "object" && typeof (w as WishItem).id === "string",
    );
  } catch {
    return [];
  }
}

/** Cached so `useSyncExternalStore` gets a stable snapshot between writes — re-parsing on
 *  every render would hand React a new array each time and never stop re-rendering. */
let snapshot: WishItem[] = read();
const listeners = new Set<() => void>();

function write(next: WishItem[]) {
  snapshot = next;
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // Storage disabled; the list still holds for this session.
  }
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export const wishId = (source: WishSource, slug: string) => `${source}:${slug}`;

export interface Wishlist {
  items: WishItem[];
  has: (id: string) => boolean;
  /** Adds, or removes if it's already there. Newest first. */
  toggle: (item: Omit<WishItem, "addedAt">) => void;
  remove: (id: string) => void;
}

export function useWishlist(): Wishlist {
  const items = useSyncExternalStore(subscribe, () => snapshot);

  const toggle = useCallback((item: Omit<WishItem, "addedAt">) => {
    const without = snapshot.filter((w) => w.id !== item.id);
    write(
      without.length === snapshot.length
        ? [{ ...item, addedAt: Date.now() }, ...snapshot]
        : without,
    );
  }, []);

  const remove = useCallback((id: string) => {
    write(snapshot.filter((w) => w.id !== id));
  }, []);

  return {
    items,
    has: (id: string) => items.some((w) => w.id === id),
    toggle,
    remove,
  };
}
