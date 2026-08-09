/**
 * The signed-in "All My Downloads" page: things a user already bought on
 * mxbikes-shop.com, shown as a grid and installable from here.
 *
 * Distinct from `ShopCatalog` in this same folder, which browses the store's public catalog
 * and cannot install anything. The two are sub-tabs of one Shop view (`Shop.tsx`).
 *
 * Two joins make this more than a list of links:
 *
 *  - **The catalog**, for artwork, author and the store page. The scraped purchases page
 *    carries a product name and a download URL and nothing else, so without this every card
 *    is a grey placeholder. Matched by product name in Rust.
 *  - **The library**, for the "Installed" badge, via the same fuzzy comparison Browse uses.
 *    Scanned across every mod folder, not just tracks: the shop sells bikes and gear too.
 *
 * Installing downloads the file and hands it to the shared review sheet, so a purchase lands
 * where its contents say it belongs and shows its collisions first — see `Context/DropReview`.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Store, LogOut, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import {
  onInstallProgress,
  onShopAuth,
  shopLogin,
  shopLogout,
  shopMyDownloads,
  shopStage,
  shopStatus,
  scanLibrary,
  modTypesFor,
  type ShopItem,
} from "../../api/mods";
import { shopMatchCatalog } from "../../api/shop";
import type { ShopMod } from "../../types";
import { buildInstalledIndex } from "../../lib/installedMatch";
import { useDropReview } from "../../Context/DropReview";
import { useConfig } from "../../Context/Config";
import { useT } from "../../i18n/context";
import PurchaseCard, { type Purchase } from "./PurchaseCard";
import { Button } from "@/Components/ui/button";
import { Skeleton } from "@/Components/ui/skeleton";

interface MyDownloadsProps {
  /** Bumped after any install so the "Installed" badges re-scan. */
  refreshKey: number;
}

export default function MyDownloads({ refreshKey }: MyDownloadsProps) {
  const t = useT();
  const { game } = useConfig();
  const { reviewPlan } = useDropReview();

  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  const [items, setItems] = useState<ShopItem[]>([]);
  const [listings, setListings] = useState<Record<string, ShopMod | null>>({});
  const [installedNames, setInstalledNames] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** Slug of the file currently downloading. One at a time — one plan may be staged. */
  const [staging, setStaging] = useState<string | null>(null);
  /** 0–1 for the staging file, or null when the server sent no Content-Length. */
  const [progress, setProgress] = useState<number | null>(null);

  const tRef = useRef(t);
  tRef.current = t;
  const stagingRef = useRef<string | null>(null);
  stagingRef.current = staging;

  const loadDownloads = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await shopMyDownloads();
      setItems(list);

      // Enrichment is a bonus, never a gate: a catalog that's unreachable (or a build with no
      // credential, which answers all-null) must still leave the purchases installable.
      const names = [...new Set(list.map((i) => i.product))];
      try {
        const matched = await shopMatchCatalog(names);
        setListings(Object.fromEntries(names.map((n, i) => [n, matched[i] ?? null])));
      } catch {
        setListings({});
      }
    } catch (e) {
      const message = String(e);
      setError(message);
      // A stale session surfaces as an auth error — drop back to signed-out.
      if (/sign in|session/i.test(message)) setLoggedIn(false);
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial status probe.
  useEffect(() => {
    let cancelled = false;
    shopStatus()
      .then((ok) => {
        if (cancelled) return;
        setLoggedIn(ok);
        if (ok) void loadDownloads();
      })
      .catch(() => !cancelled && setLoggedIn(false));
    return () => {
      cancelled = true;
    };
  }, [loadDownloads]);

  // WebView sign-in completion.
  useEffect(() => {
    const unlisten = onShopAuth((ok) => {
      if (ok) {
        setLoggedIn(true);
        toast.success(tRef.current("shop.signedIn"));
        void loadDownloads();
      } else {
        toast.error(tRef.current("shop.sessionFailed"));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadDownloads]);

  // A purchased track is often hundreds of megabytes, so the card shows how far along it is
  // rather than an unmoving spinner. `install::download` already emits this per slug.
  useEffect(() => {
    const unlisten = onInstallProgress((p) => {
      if (p.stage !== "downloading" || !p.total) return;
      setProgress((cur) => (p.slug === stagingRef.current ? p.received! / p.total! : cur));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // "Installed" badges, across every mod folder — the shop sells tracks, bikes and gear, and
  // scanning only `mods/tracks` (as this view used to) leaves two thirds of it unbadged.
  useEffect(() => {
    let cancelled = false;
    const subpaths = modTypesFor(game.id).map((m) => m.installSubpath);
    Promise.all(subpaths.map((s) => scanLibrary(s).catch(() => [])))
      .then((scans) => {
        if (cancelled) return;
        setInstalledNames(scans.flat().map((e) => e.name));
      })
      .catch(() => !cancelled && setInstalledNames([]));
    return () => {
      cancelled = true;
    };
  }, [refreshKey, loggedIn, game.id]);

  /** Purchases, one entry per product, with the catalog and library joins applied. */
  const purchases = useMemo<Purchase[]>(() => {
    const installed = buildInstalledIndex(installedNames);
    const byProduct = new Map<string, ShopItem[]>();
    for (const item of items) {
      const files = byProduct.get(item.product);
      if (files) files.push(item);
      else byProduct.set(item.product, [item]);
    }
    return [...byProduct].map(([product, files]) => ({
      product,
      files,
      listing: listings[product] ?? null,
      installed: installed.has(product),
    }));
  }, [items, listings, installedNames]);

  const install = useCallback(
    async (file: ShopItem) => {
      setStaging(file.slug);
      setProgress(null);
      try {
        reviewPlan(await shopStage(file));
      } catch (e) {
        const message = String(e);
        toast.error(tRef.current("purchases.downloadFailed", { title: file.title }), {
          description: message,
          duration: Infinity,
        });
        // An expired session is the one failure worth acting on: EDD's file links are
        // time-limited, so a list left open long enough goes stale link by link.
        if (/sign in|session/i.test(message)) setLoggedIn(false);
      } finally {
        setStaging(null);
        setProgress(null);
      }
    },
    [reviewPlan],
  );

  const logout = useCallback(async () => {
    await shopLogout();
    setLoggedIn(false);
    setItems([]);
    setListings({});
    setError(null);
  }, []);

  // Signed-out gate.
  if (loggedIn === false) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-4 px-7 pb-10 text-center">
        <div className="grid size-14 place-items-center rounded-2xl bg-foreground/[0.06] text-foreground/50">
          <Store className="size-7" strokeWidth={1.5} />
        </div>
        <div className="flex max-w-sm flex-col gap-1.5">
          <h2 className="text-[15px] font-semibold">{t("shop.signInTitle")}</h2>
          <p className="text-[12.5px] text-muted-foreground">{t("shop.signInBody")}</p>
        </div>
        <Button onClick={() => void shopLogin()}>
          <Store className="size-4" /> {t("shop.signIn")}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-none items-center gap-2 px-7 pb-3">
        <span className="text-[12.5px] text-muted-foreground">
          {loggedIn && !loading
            ? t("purchases.count", { count: purchases.length })
            : t("shop.myDownloads")}
        </span>
        {loggedIn && (
          <div className="ml-auto flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => void loadDownloads()}
              disabled={loading}
            >
              <RefreshCw className="size-3.5" /> {t("common.refresh")}
            </Button>
            <Button variant="outline" size="sm" onClick={() => void logout()}>
              <LogOut className="size-3.5" /> {t("shop.logOut")}
            </Button>
          </div>
        )}
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
            <p className="select-text text-[13px] text-destructive">
              {t("shop.loadFailed", { error: error.replace(/^Error:\s*/, "") })}
            </p>
            <Button variant="outline" size="sm" onClick={() => void loadDownloads()}>
              {t("common.retry")}
            </Button>
          </div>
        ) : loading || loggedIn === null ? (
          <div className="grid grid-cols-4 gap-3.5">
            {Array.from({ length: 8 }).map((_, i) => (
              <Skeleton key={i} className="aspect-square rounded-xl" />
            ))}
          </div>
        ) : purchases.length === 0 ? (
          <p className="py-20 text-center text-[13px] text-muted-foreground">
            {t("shop.empty")}
          </p>
        ) : (
          <div className="grid grid-cols-4 gap-3.5">
            {purchases.map((p) => (
              <PurchaseCard
                key={p.product}
                purchase={p}
                busy={p.files.some((f) => f.slug === staging)}
                progress={progress}
                disabled={staging !== null}
                onInstall={(file) => void install(file)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
