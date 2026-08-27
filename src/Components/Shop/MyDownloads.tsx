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
import { ArrowUpDown, Check, Search, Store, LogOut, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import {
  onShopAuth,
  shopInstalledMap,
  shopLogin,
  shopLogout,
  shopMyDownloads,
  shopStatus,
  scanLibrary,
  modTypesFor,
  buildDestinations,
  destStorageKey,
  resolveInitialFolder,
  type DestOption,
  type ModType,
  type ShopItem,
} from "../../api/mods";
import {
  PURCHASE_SORTS,
  shopCatalogCategories,
  shopMatchCatalog,
  type PurchaseSort,
} from "../../api/shop";
import type { ShopCategory, ShopMod } from "../../types";
import { buildInstalledIndex } from "../../lib/installedMatch";
import { useInstall } from "../../Context/Install";
import { useConfig } from "../../Context/Config";
import { useT } from "../../i18n/context";
import PurchaseCard, { type Purchase } from "./PurchaseCard";
import InstallDialog, { type InstallChoice } from "../ModDetail/InstallDialog";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/Components/ui/alert-dialog";
import ShopDetail from "./ShopDetail";
import CategoryPill from "./CategoryPill";
import { Button } from "@/Components/ui/button";
import { Skeleton } from "@/Components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/Components/ui/select";
import { cn } from "@/lib/utils";

/** The pill for purchases the catalog doesn't list. */
const OTHER_CATEGORY = -1;

interface MyDownloadsProps {
  /** Bumped after any install so the "Installed" badges re-scan. */
  refreshKey: number;
}

export default function MyDownloads({ refreshKey }: MyDownloadsProps) {
  const t = useT();
  const { game } = useConfig();
  const { activeFor, startShopInstall } = useInstall();

  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  const [items, setItems] = useState<ShopItem[]>([]);
  const [listings, setListings] = useState<Record<string, ShopMod | null>>({});
  const [installedNames, setInstalledNames] = useState<string[]>([]);
  /** Product → the folders its install recorded. Exact, where the fuzzy match only guesses. */
  const [installRecord, setInstallRecord] = useState<Record<string, string[]>>({});
  /** Bumped by a one-click install, which the parent's `refreshKey` never sees. */
  const [installedBump, setInstalledBump] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Toolbar — all client-side; every purchase is already in memory.
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [category, setCategory] = useState<number | null>(null);
  /** The store's category tree, for collapsing a purchase's categories to their roots. */
  const [tree, setTree] = useState<ShopCategory[]>([]);
  const [sort, setSort] = useState<PurchaseSort>("recentlyPurchased");
  const [notInstalledOnly, setNotInstalledOnly] = useState(false);
  /** The product whose detail page is open, if any. */
  const [openProduct, setOpenProduct] = useState<string | null>(null);

  const tRef = useRef(t);
  tRef.current = t;

  // `reload` is passed by the Refresh buttons: the page is read from a hidden WebView that
  // holds whatever it last loaded, so refreshing has to tell it to navigate again.
  const loadDownloads = useCallback(async (reload = false) => {
    setLoading(true);
    setError(null);
    try {
      const list = await shopMyDownloads(reload);
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
        // `reload`, like the Refresh buttons: the page is read out of a hidden window, and a
        // sign-in is precisely the moment its contents stop being about the right person. A
        // read that didn't say so would answer from the page the previous session left behind.
        void loadDownloads(true);
      } else {
        toast.error(tRef.current("shop.sessionFailed"));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadDownloads]);

  // Same 350 ms debounce the catalog and Browse use.
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim().toLowerCase()), 350);
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    let cancelled = false;
    shopCatalogCategories()
      .then((cats) => !cancelled && setTree(cats))
      .catch(() => {});
    return () => {
      cancelled = true;
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
    shopInstalledMap()
      .then((m) => !cancelled && setInstallRecord(m))
      .catch(() => !cancelled && setInstallRecord({}));
    return () => {
      cancelled = true;
    };
  }, [refreshKey, installedBump, loggedIn, game.id]);

  /** Purchases, one entry per product, with the catalog and library joins applied. */
  const purchases = useMemo<Purchase[]>(() => {
    const fuzzy = buildInstalledIndex(installedNames);
    // Both the file name and its stem: the library lists `X.pkz` while an archive that
    // extracts lands in a folder called `X`, and a record may name either.
    const onDisk = new Set<string>();
    for (const n of installedNames) {
      const lower = n.toLowerCase();
      onDisk.add(lower);
      onDisk.add(lower.replace(/\.(pkz|zip|rar|7z|pnt)$/, ""));
    }
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
      // Only believed while the folder it names is still there; fuzzy match as fallback.
      installed:
        (installRecord[product] ?? []).some((f) =>
          onDisk.has(f.toLowerCase().replace(/\.(pkz|zip|rar|7z|pnt)$/, "")),
        ) ||
        fuzzy.has(product),
    }));
  }, [items, listings, installedNames, installRecord]);

  /** Category id → its root. Only roots become pills, so the row can't grow without bound. */
  const rootOf = useMemo(() => {
    const byId = new Map(tree.map((c) => [c.id, c]));
    const roots = new Map<number, number>();
    for (const c of tree) {
      let node = c;
      while (node.parent !== null && byId.has(node.parent)) node = byId.get(node.parent)!;
      roots.set(c.id, node.id);
    }
    return roots;
  }, [tree]);

  const rootsFor = useCallback(
    (p: Purchase) =>
      new Set(
        (p.listing?.categoryIds ?? [])
          .map((id) => rootOf.get(id))
          .filter((r): r is number => r !== undefined),
      ),
    [rootOf],
  );

  /** Root pills, each with how many purchases fall under it. */
  const categories = useMemo(() => {
    const names = new Map(tree.map((c) => [c.id, c.name]));
    const counts = new Map<number, number>();
    for (const p of purchases) {
      for (const r of rootsFor(p)) counts.set(r, (counts.get(r) ?? 0) + 1);
    }
    return [...counts]
      .map(([id, count]) => ({ id, name: names.get(id) ?? "", count }))
      .filter((c) => c.name !== "")
      .sort((a, b) => b.count - a.count || a.name.localeCompare(b.name));
  }, [purchases, tree, rootsFor]);

  /** Whether anything at all is unmatched, which is what earns the "Other" pill its space. */
  const hasUncategorised = useMemo(
    () => purchases.some((p) => rootsFor(p).size === 0),
    [purchases, rootsFor],
  );

  /** What the grid actually shows: the toolbar applied. */
  const shown = useMemo(() => {
    const matches = (p: Purchase) => {
      if (notInstalledOnly && p.installed) return false;
      const roots = rootsFor(p);
      if (category === OTHER_CATEGORY) {
        if (roots.size > 0) return false;
      } else if (category !== null && !roots.has(category)) {
        return false;
      }
      if (!debounced) return true;
      // Author and file labels too — both are real ways people look for something they own.
      const haystack = [
        p.product,
        p.listing?.author ?? "",
        ...p.files.map((f) => `${f.fileLabel} ${f.title}`),
      ]
        .join(" ")
        .toLowerCase();
      return haystack.includes(debounced);
    };

    const list = purchases.filter(matches);
    const newest = (p: Purchase) =>
      Math.max(...p.files.map((f) => Date.parse(f.date) || 0), 0);

    switch (sort) {
      case "nameAsc":
        return list.sort((a, b) => a.product.localeCompare(b.product));
      case "notInstalled":
        // Not-yet-installed first, then alphabetical inside each group, so the list is stable
        // rather than reshuffling as things get installed.
        return list.sort(
          (a, b) =>
            Number(a.installed) - Number(b.installed) || a.product.localeCompare(b.product),
        );
      default:
        return list.sort((a, b) => newest(b) - newest(a));
    }
  }, [purchases, debounced, category, sort, notInstalledOnly, rootsFor]);

  /** Which mods/ subfolder a purchase belongs in, from its catalog category. */
  /** Which mod type a purchase is, from its catalog category. */
  const typeFor = useCallback(
    (p: Purchase): ModType => {
      const names = (p.listing?.categoryNames ?? []).join(" ").toLowerCase();
      const types = modTypesFor(game.id);
      return (
        types.find((mt) => names.includes(mt.id)) ??
        types.find((mt) => mt.id === "track" && names.includes("track")) ??
        types[0]
      );
    },
    [game.id],
  );

  /** The install running for one of a purchase's files, and how far along it is. Several
   *  installs can be in flight now, so this is asked per purchase rather than once. */
  const installOf = useCallback(
    (files: ShopItem[]) => {
      for (const f of files) {
        const it = activeFor(f.slug);
        if (it) return it;
      }
      return null;
    },
    [activeFor],
  );

  const progressOf = useCallback(
    (files: ShopItem[]) => {
      const it = installOf(files);
      return it && it.total ? (it.received ?? 0) / it.total : null;
    },
    [installOf],
  );

  /** The purchase waiting on the destination dialog, with everything the dialog needs. */
  const [pending, setPending] = useState<{
    purchase: Purchase;
    file: ShopItem;
    modType: ModType;
    destOptions: DestOption[];
    suggestions: string[];
    folderCounts: Map<string, number>;
    initialFolder: string;
  } | null>(null);
  /** Set instead of `pending` when the thing is already installed — confirm first, as Browse does. */
  const [confirming, setConfirming] = useState<{ purchase: Purchase; file: ShopItem } | null>(
    null,
  );

  /** Work out where a purchase could go, then let the user choose — Browse's flow exactly. */
  const askWhere = useCallback(
    async (purchase: Purchase, file: ShopItem) => {
      const modType = typeFor(purchase);
      const installedThere = await scanLibrary(modType.installSubpath).catch(() => []);
      const dest = buildDestinations(modType, purchase.product, installedThere);
      const counts = new Map<string, number>();
      for (const it of installedThere) counts.set(it.folder, (counts.get(it.folder) ?? 0) + 1);
      const remembered = localStorage.getItem(destStorageKey(game, modType));
      setPending({
        purchase,
        file,
        modType,
        destOptions: dest.options,
        suggestions: dest.suggestions,
        folderCounts: counts,
        initialFolder:
          remembered ?? resolveInitialFolder(game, modType, dest.options, dest.guess),
      });
    },
    [game, typeFor],
  );

  const install = useCallback(
    (purchase: Purchase, file: ShopItem) => {
      if (purchase.installed) setConfirming({ purchase, file });
      else void askWhere(purchase, file);
    },
    [askWhere],
  );

  /** The dialog answered: remember the folder and queue it like any other install. */
  const confirmInstall = useCallback(
    ({ destFolder }: InstallChoice) => {
      if (!pending) return;
      const { purchase, file, modType } = pending;
      localStorage.setItem(destStorageKey(game, modType), destFolder);
      setPending(null);
      startShopInstall({
        slug: file.slug,
        title: purchase.product,
        subpath: modType.installSubpath,
        destFolder,
        item: file,
      });
      setInstalledBump((n) => n + 1);
    },
    [pending, game, startShopInstall],
  );

  const logout = useCallback(async () => {
    await shopLogout();
    setLoggedIn(false);
    setItems([]);
    setListings({});
    setError(null);
    setOpenProduct(null);
  }, []);

  /**
   * The purchase whose detail is open.
   *
   * Resolved from the live list rather than held as an object, so the page it renders keeps up
   * with an install finishing underneath it. A product that disappears — a refresh dropped it —
   * closes the page by falling back to the grid.
   */
  const open = useMemo(
    () => purchases.find((p) => p.product === openProduct && p.listing) ?? null,
    [purchases, openProduct],
  );

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

  // Mounted by both the grid and the detail view, so installing from either can ask.
  const dialogs = (
    <>
      {pending && (
        <InstallDialog
          open
          onOpenChange={(o) => !o && setPending(null)}
          title={pending.purchase.product}
          image={pending.purchase.listing?.image ?? null}
          modType={pending.modType}
          destOptions={pending.destOptions}
          suggestions={pending.suggestions}
          folderCounts={pending.folderCounts}
          initialFolder={pending.initialFolder}
          onConfirm={confirmInstall}
        />
      )}

      <AlertDialog
        open={confirming !== null}
        onOpenChange={(o) => !o && setConfirming(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>
              {t("browse.reinstallOne", { title: confirming?.purchase.product ?? "" })}
            </AlertDialogTitle>
            <AlertDialogDescription>{t("browse.reinstallOneBody")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("common.cancel")}</AlertDialogCancel>
            <AlertDialogAction
              onClick={() => {
                const next = confirming;
                setConfirming(null);
                if (next) void askWhere(next.purchase, next.file);
              }}
            >
              {t("browse.reinstall")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );

  // Detail takes over the whole pane, exactly as it does in the catalog.
  if (open) {
    return (
      <>
      <ShopDetail
        id={open.listing!.id}
        currency="USD"
        onBack={() => setOpenProduct(null)}
        owned={{
          files: open.files,
          installed: open.installed,
          busy: installOf(open.files) != null,
          progress: progressOf(open.files),
          disabled: false,
          onInstall: (file) => install(open, file),
        }}
      />
      {dialogs}
      </>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex flex-none items-center gap-2 px-7 pb-3">
        <span className="text-[12.5px] text-muted-foreground">
          {loggedIn && !loading
            ? t("purchases.count", { count: shown.length })
            : t("shop.myDownloads")}
        </span>
        {loggedIn && (
          <div className="ml-auto flex items-center gap-2">
            <div className="flex w-[240px] items-center gap-2 rounded-lg border border-input bg-card px-3 py-1.5">
              <Search className="size-3.5 text-faint" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder={t("purchases.searchPlaceholder")}
                className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
              />
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() => void loadDownloads(true)}
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

      {/* Laid out like the catalog's, so the two tabs read as one screen. */}
      {loggedIn && !loading && purchases.length > 0 && (
        <div className="flex flex-none flex-wrap items-center gap-2 px-7 pb-3">
          <CategoryPill
            label={t("shopCatalog.allCategories")}
            on={category === null}
            onClick={() => setCategory(null)}
          />
          {categories.map((c) => (
            <CategoryPill
              key={c.id}
              label={c.name}
              count={c.count}
              on={category === c.id}
              onClick={() => setCategory(c.id)}
            />
          ))}
          {hasUncategorised && (
            <CategoryPill
              label={t("purchases.otherCategory")}
              on={category === OTHER_CATEGORY}
              onClick={() => setCategory(OTHER_CATEGORY)}
            />
          )}

          <div className="ml-auto flex items-center gap-2 self-center">
            <button
              onClick={() => setNotInstalledOnly((v) => !v)}
              className={cn(
                "flex cursor-default items-center gap-1.5 rounded-full px-3 py-[5px] text-[12px] font-medium transition-colors",
                notInstalledOnly
                  ? "bg-emerald-500 font-semibold text-black"
                  : "border border-input text-muted-foreground hover:text-foreground",
              )}
            >
              <Check className="size-3" />
              {t("purchases.notInstalledOnly")}
            </button>
            <ArrowUpDown className="size-3.5 text-faint" />
            <Select value={sort} onValueChange={(v) => setSort(v as PurchaseSort)}>
              <SelectTrigger className="h-8 w-[200px] bg-card">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {PURCHASE_SORTS.map((s) => (
                  <SelectItem key={s.value} value={s.value}>
                    {t(s.label)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
      )}

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {error ? (
          <div className="mx-auto flex max-w-md flex-col items-center gap-3 py-20 text-center">
            <p className="select-text text-[13px] text-destructive">
              {t("shop.loadFailed", { error: error.replace(/^Error:\s*/, "") })}
            </p>
            <Button variant="outline" size="sm" onClick={() => void loadDownloads(true)}>
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
        ) : shown.length === 0 ? (
          // Told apart from "you own nothing" on purpose: one is a filter to clear, the other
          // is a trip to the store.
          <p className="py-20 text-center text-[13px] text-muted-foreground">
            {t("purchases.noMatches")}
          </p>
        ) : (
          <div className="grid grid-cols-4 gap-3.5">
            {shown.map((p) => (
              <PurchaseCard
                key={p.product}
                purchase={p}
                busy={installOf(p.files) != null}
                progress={progressOf(p.files)}
                disabled={false}
                onInstall={(file) => install(p, file)}
                // Only a purchase the catalog lists has a detail page to open.
                onOpen={p.listing ? () => setOpenProduct(p.product) : undefined}
              />
            ))}
          </div>
        )}
      </div>

    </div>
  );
}
