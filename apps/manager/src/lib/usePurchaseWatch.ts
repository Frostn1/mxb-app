/**
 * Watch a store for a purchase that just happened, and install it.
 *
 * Neither store has a webhook, a callback or a cheap "what's new" endpoint, so a purchase has
 * never been *detected* by this app — someone had to open the Purchases tab and press Refresh.
 * The one thing the app does know is the moment it hands a product page to the browser, which
 * is where a purchase starts. So: take a snapshot of what the account owns as the browser
 * opens, then look again every couple of seconds for about two minutes, and install whatever
 * is in the second list and not the first.
 *
 * Modelled on `useSteamLink`, which polls the control plane the same way after opening a
 * browser for the same reason: the app is waiting on something happening outside it.
 *
 * The two stores are read by different means and neither is instant:
 *
 *  - **mxbikes-shop.com** sits behind a Cloudflare managed challenge that refuses our HTTP
 *    client, so its purchases page is read out of a hidden WebView parked on it. A fresh read
 *    means navigating that window again — seconds, sometimes more.
 *  - **MXB Hub** is ordinary HTML over the same client every other download uses, and answers
 *    quickly.
 *
 * Because each look is awaited before the next wait starts, a slow store simply polls less
 * often. The two minutes is wall-clock, so a store that takes ten seconds a read gets ten
 * looks rather than forty-eight.
 */

import { useCallback, useEffect, useRef } from "react";
import { toast } from "sonner";
import {
  buildDestinations,
  purchaseModTypes,
  resolveInitialFolder,
  scanLibrary,
  shopMyDownloads,
  type ModType,
  type ShopItem,
} from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { onStoreVisit, shopMatchCatalog, type StoreId } from "@/api/shop";
import { hubMyDownloads, type HubItem } from "@/api/hub";
import { readAutoInstall, writeStoreCount } from "@/lib/autoInstall";
import { useInstall } from "@/Context/Install";
import { useT } from "@/i18n";

/** Between looks. The same beat `useSteamLink` waits on its own browser round trip. */
const POLL_MS = 2500;

/** How long a store stays watched after the browser opened on it. Long enough for a checkout
 *  and a card, short enough that a page opened out of curiosity stops costing anything. */
const WINDOW_MS = 120_000;

/**
 * More new products than this and the diff is not to be trusted — most likely the snapshot was
 * taken against a signed-out or half-loaded page. Installing forty mods nobody asked for is a
 * far worse failure than saying so and letting the Purchases tab handle it.
 */
const MAX_AUTO = 5;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** One purchased file, in the shape both stores agree on, with its catalog categories. */
interface Owned {
  /** The product it belongs to — several files can share one. */
  product: string;
  item: ShopItem | HubItem;
  /** Category names from the store's catalog, for working out which mods/ folder it goes in. */
  categories: string[];
}

/** Which mod type a purchase is, read off its catalog categories. The same fold the Purchases
 *  grids use — kept identical deliberately, so an automatic install lands where pressing
 *  Install would have put it. */
function typeFor(categories: string[], types: ModType[]): ModType {
  const names = categories.join(" ").toLowerCase();
  return (
    types.find((mt) => names.includes(mt.id)) ??
    types.find((mt) => mt.id === "track" && names.includes("track")) ??
    types[0]
  );
}

export function usePurchaseWatch() {
  const t = useT();
  const { game } = useConfig();
  const { startShopInstall, startHubInstall } = useInstall();

  // Everything the loop reads that can change under it, held in refs: the watch outlives any
  // render, and a two-minute loop closing over a stale `game` would install into the wrong
  // title's folders.
  const env = useRef({ t, game, startShopInstall, startHubInstall });
  env.current = { t, game, startShopInstall, startHubInstall };

  /** The deadline of the watch running on each store, if one is. */
  const watching = useRef<Partial<Record<StoreId, { until: number }>>>({});

  /** Read everything the account owns on one store. `null` when it can't be read at all —
   *  signed out, offline, challenged — which is different from owning nothing. */
  const readOwned = useCallback(
    async (store: StoreId, fresh: boolean): Promise<Owned[] | null> => {
      try {
        if (store === "shop") {
          // `fresh` navigates the parked WebView again. Without it the read answers from
          // whatever DOM that window was left holding, which may predate the purchase.
          const items = await shopMyDownloads(fresh);
          return items.map((item) => ({ product: item.product, item, categories: [] }));
        }
        const { items, listings } = await hubMyDownloads();
        return items.map((item, i) => ({
          product: item.product,
          item,
          categories: listings[i]?.categoryNames ?? [],
        }));
      } catch {
        return null;
      }
    },
    [],
  );

  /** Queue one file, worked out exactly as the Install dialog would have defaulted it. */
  const enqueue = useCallback(async (store: StoreId, owned: Owned) => {
    const { game: g, startShopInstall: shop, startHubInstall: hub } = env.current;
    const modType = typeFor(owned.categories, purchaseModTypes(g.id));
    const installedThere = await scanLibrary(modType.installSubpath).catch(() => []);
    const dest = buildDestinations(modType, owned.product, installedThere);
    // `resolveInitialFolder` honours the folder this mod type was last installed to, so an
    // automatic install follows the same habit the dialog would have offered.
    const destFolder = resolveInitialFolder(g, modType, dest.options, dest.guess);
    const common = {
      slug: owned.item.slug,
      title: owned.product,
      subpath: modType.installSubpath,
      destFolder,
    };
    if (store === "shop") shop({ ...common, item: owned.item as ShopItem });
    else hub({ ...common, item: owned.item as HubItem });
  }, []);

  /**
   * Install what arrived.
   *
   * By product, not by file: a product that ships several files (a bike's PRO and AMS builds,
   * say) is a choice, and choosing one of them for someone is worse than telling them it is
   * waiting — both would extract into the same folder and the second would win.
   */
  const install = useCallback(
    async (store: StoreId, fresh: Owned[]) => {
      const { t: tr } = env.current;
      const byProduct = new Map<string, Owned[]>();
      for (const o of fresh) {
        const files = byProduct.get(o.product);
        if (files) files.push(o);
        else byProduct.set(o.product, [o]);
      }
      if (byProduct.size > MAX_AUTO) {
        toast.info(tr("accounts.manyNew"));
        return;
      }
      for (const [product, files] of byProduct) {
        if (files.length > 1) {
          toast.info(tr("accounts.pickFile", { name: product }));
          continue;
        }
        // Enrichment only: without it the mod still installs, it just lands on the mod type's
        // default folder rather than the one its category implies.
        if (store === "shop") {
          try {
            const [listing] = await shopMatchCatalog([product]);
            files[0].categories = listing?.categoryNames ?? [];
          } catch {
            // Leave it uncategorised.
          }
        }
        await enqueue(store, files[0]);
        toast.success(tr("accounts.installing", { name: product }));
      }
    },
    [enqueue],
  );

  const watch = useCallback(
    async (store: StoreId) => {
      // Already watching this store: extend it rather than re-snapshot. Opening a second
      // product page mid-checkout must not make the first purchase look like it was always
      // there.
      const live = watching.current[store];
      if (live) {
        live.until = Date.now() + WINDOW_MS;
        return;
      }

      const before = await readOwned(store, true);
      // Not signed in, or the store wouldn't answer. There is nothing to diff against, and a
      // diff against nothing would call every mod on the account new.
      if (!before) return;
      writeStoreCount(store, new Set(before.map((o) => o.product)).size);
      const seen = new Set(before.map((o) => o.item.slug));

      const state = { until: Date.now() + WINDOW_MS };
      watching.current[store] = state;
      try {
        while (Date.now() < state.until) {
          await sleep(POLL_MS);
          const now = await readOwned(store, true);
          if (!now) continue;
          const fresh = now.filter((o) => !seen.has(o.item.slug));
          if (fresh.length === 0) continue;
          writeStoreCount(store, new Set(now.map((o) => o.product)).size);
          await install(store, fresh);
          return;
        }
      } finally {
        delete watching.current[store];
      }
    },
    [readOwned, install],
  );

  useEffect(() => {
    return onStoreVisit((store) => {
      // Read at the moment of the visit, not when this effect ran: the switch can be turned
      // off in Settings while a store page is open.
      if (!readAutoInstall()) return;
      void watch(store);
    });
  }, [watch]);
}
