/**
 * Everything the player owns on the two stores, in one list.
 *
 * The Purchases grids each answer for their own store, which is right where you are shopping
 * and wrong when the question is "what have I bought" — that one has no store in it. So this
 * reads both, joins them against the library the same way those grids do (`groupPurchases`),
 * and hands back one list with the store noted on each row.
 *
 * Reads nothing until it is asked to. Both stores cost a network round trip — the shop's is a
 * hidden WebView navigating a page — and the Library is opened constantly, so `enabled` stays
 * false until the player actually clicks Owned.
 */
import { useCallback, useEffect, useState } from "react";
import {
  modTypesFor,
  shopInstalledMap,
  shopMyDownloads,
  shopStatus,
  type ShopItem,
} from "@frost/shared/api/mods";
import type { HubMod, ShopMod } from "@frost/shared/types";
import { useConfig } from "@frost/shared/Context/Config";
import { shopMatchCatalog, type StoreId } from "@/api/shop";
import { hubMyDownloads, hubStatus, type HubItem } from "@/api/hub";
import { groupPurchases } from "./purchases";
import { scanLibrariesSequentially } from "./scanLibraries";

/** One owned product, flattened to what a list of them needs to show. */
export interface OwnedRow {
  /** Unique across both stores — the same product can be sold on each. */
  key: string;
  store: StoreId;
  product: string;
  author: string | null;
  image: string | null;
  /** How many files the store lists under it. */
  fileCount: number;
  installed: boolean;
}

export interface OwnedState {
  rows: OwnedRow[];
  loading: boolean;
  /** Whether a read has ever finished. Until it has, a count of 0 would be a guess. */
  loaded: boolean;
  /** True once either store has answered as signed in. Neither means there is nothing to
   *  show and the panel says so rather than reporting zero purchases. */
  signedIn: boolean;
  error: string | null;
  reload: () => void;
}

const EMPTY: OwnedRow[] = [];

export function useOwned(enabled: boolean, refreshKey: number): OwnedState {
  const { game } = useConfig();
  const [rows, setRows] = useState<OwnedRow[]>(EMPTY);
  const [loading, setLoading] = useState(false);
  const [signedIn, setSignedIn] = useState(false);
  const [loaded, setLoaded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [bump, setBump] = useState(0);
  const reload = useCallback(() => setBump((n) => n + 1), []);

  useEffect(() => {
    if (!enabled) return;
    let alive = true;
    setLoading(true);
    setError(null);

    const run = async () => {
      const [onShop, onHub] = await Promise.all([
        shopStatus().catch(() => false),
        hubStatus().catch(() => false),
      ]);
      if (!alive) return;
      setSignedIn(onShop || onHub);
      if (!onShop && !onHub) {
        setRows(EMPTY);
        return;
      }

      // The library, once, for both stores' "installed" joins — the stores sell tracks,
      // bikes and gear, so every mod folder is scanned, not just one.
      const subpaths = modTypesFor(game.id).map((m) => m.installSubpath);
      const [scans, record] = await Promise.all([
        scanLibrariesSequentially(subpaths),
        shopInstalledMap().catch(() => ({}) as Record<string, string[]>),
      ]);
      const installedNames = scans.flat().map((e) => e.name);

      const out: OwnedRow[] = [];

      if (onShop) {
        // Never `reload`: that navigates the parked WebView again, which is what the
        // Purchases tab's own Refresh is for. Here the last read is good enough.
        const items = await shopMyDownloads().catch(() => [] as ShopItem[]);
        const names = [...new Set(items.map((i) => i.product))];
        // Artwork is a bonus, never a gate — a build with no catalog credential answers
        // all-null and the rows simply stay plain.
        const matched = await shopMatchCatalog(names).catch(() => names.map(() => null));
        const listings = Object.fromEntries(
          names.map((n, i) => [n, matched[i] ?? null]),
        ) as Record<string, ShopMod | null>;
        for (const p of groupPurchases(items, listings, installedNames, record)) {
          out.push({
            key: `shop:${p.product}`,
            store: "shop",
            product: p.product,
            author: p.listing?.author ?? null,
            image: p.listing?.image ?? null,
            fileCount: p.files.length,
            installed: p.installed,
          });
        }
      }

      if (onHub) {
        const { items, listings: found } = await hubMyDownloads().catch(() => ({
          items: [] as HubItem[],
          listings: [] as (HubMod | null)[],
        }));
        // Positional against `items`, so the map is built here rather than matched by name.
        const listings: Record<string, HubMod | null> = {};
        items.forEach((row, i) => {
          listings[row.product] = found[i] ?? listings[row.product] ?? null;
        });
        for (const p of groupPurchases(items, listings, installedNames, record)) {
          out.push({
            key: `hub:${p.product}`,
            store: "hub",
            product: p.product,
            author: p.listing?.author ?? null,
            image: p.listing?.image ?? null,
            fileCount: p.files.length,
            installed: p.installed,
          });
        }
      }

      if (!alive) return;
      setRows(out.sort((a, b) => a.product.localeCompare(b.product)));
    };

    run()
      .catch((e) => alive && setError(String(e)))
      .finally(() => {
        if (!alive) return;
        setLoading(false);
        setLoaded(true);
      });
    return () => {
      alive = false;
    };
  }, [enabled, refreshKey, bump, game.id]);

  return { rows, loading, loaded, signedIn, error, reload };
}
