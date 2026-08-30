/**
 * The signed-in half of MXB Hub: what this account owns, installable from here.
 *
 * The sibling of `Shop/MyDownloads` — same grid, same cards, same install flow — over a store
 * that behaves entirely differently underneath. Three things fall away as a result:
 *
 *  - **No hidden WebView.** MXB Hub isn't behind Cloudflare, so the page is fetched by an HTTP
 *    client. There is nothing holding a stale DOM, so Refresh is just another fetch and there
 *    is no `reload` flag to thread through it.
 *  - **One request, not two.** A Hub download row links to its product page, so Rust resolves
 *    the catalog entries by slug on the same call — no separate match step, and no name-fold.
 *  - **No credential gate.** Nothing here depends on a build-time secret; every build can sign
 *    in and install.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ArrowUpDown, Check, LogOut, RefreshCw, Search, Store } from "lucide-react";
import { toast } from "sonner";
import {
  hubCategories,
  hubDetail,
  hubLogin,
  hubLogout,
  hubMyDownloads,
  hubStatus,
  onHubAuth,
} from "../../api/hub";
import {
  buildDestinations,
  destStorageKey,
  modTypesFor,
  resolveInitialFolder,
  scanLibrary,
  shopInstalledMap,
  type DestOption,
  type ModType,
  type ShopItem,
} from "../../api/mods";
import { PURCHASE_SORTS, type PurchaseSort } from "../../api/shop";
import type { HubCategory, HubMod } from "../../types";
import { buildInstalledIndex } from "../../lib/installedMatch";
import { useInstall } from "../../Context/Install";
import { useConfig } from "../../Context/Config";
import { useT } from "../../i18n/context";
import PurchaseCard, { type Purchase } from "../Shop/PurchaseCard";
import ShopDetail from "../Shop/ShopDetail";
import CategoryPill from "../Shop/CategoryPill";
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

/** The pill for purchases the catalog no longer lists. */
const OTHER_CATEGORY = -1;

interface HubPurchasesProps {
  /** Bumped after any install so the "Installed" badges re-scan. */
  refreshKey: number;
}

export default function HubPurchases({ refreshKey }: HubPurchasesProps) {
  const t = useT();
  const { game } = useConfig();
  const { activeFor, startHubInstall } = useInstall();

  const [loggedIn, setLoggedIn] = useState<boolean | null>(null);
  const [items, setItems] = useState<ShopItem[]>([]);
  const [listings, setListings] = useState<Record<string, HubMod | null>>({});
  const [installedNames, setInstalledNames] = useState<string[]>([]);
  /** Product → the folders its install recorded. Exact, where the fuzzy match only guesses. */
  const [installRecord, setInstallRecord] = useState<Record<string, string[]>>({});
  /** Bumped by an install started here, which the parent's `refreshKey` never sees. */
  const [installedBump, setInstalledBump] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Toolbar — all client-side; every purchase is already in memory.
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [category, setCategory] = useState<number | null>(null);
  const [tree, setTree] = useState<HubCategory[]>([]);
  const [sort, setSort] = useState<PurchaseSort>("nameAsc");
  const [notInstalledOnly, setNotInstalledOnly] = useState(false);
  const [openProduct, setOpenProduct] = useState<string | null>(null);

  const tRef = useRef(t);
  tRef.current = t;

  const loadDownloads = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const { items: rows, listings: found } = await hubMyDownloads();
      setItems(rows);
      // Positional, so the map is built here rather than matched again by name.
      const byProduct: Record<string, HubMod | null> = {};
      rows.forEach((row, i) => {
        byProduct[row.product] = found[i] ?? byProduct[row.product] ?? null;
      });
      setListings(byProduct);
    } catch (e) {
      const message = String(e);
      setError(message);
      // A dead cookie surfaces as an auth error — drop back to signed-out rather than
      // leaving a grid that will fail the same way on every retry.
      if (/sign in|session/i.test(message)) setLoggedIn(false);
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial status probe.
  useEffect(() => {
    let cancelled = false;
    hubStatus()
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

  // Sign-in window completion.
  useEffect(() => {
    const unlisten = onHubAuth((ok) => {
      if (ok) {
        setLoggedIn(true);
        toast.success(tRef.current("hub.signedIn"));
        void loadDownloads();
      } else {
        toast.error(tRef.current("hub.sessionFailed"));
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadDownloads]);

  // Same 350 ms debounce every other grid uses.
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim().toLowerCase()), 350);
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(() => {
    let cancelled = false;
    hubCategories()
      .then((cats) => !cancelled && setTree(cats))
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // "Installed" badges, across every mod folder — the Hub sells paints, gear and modelswaps,
  // so scanning one subfolder would leave most of it unbadged.
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
      installed:
        (installRecord[product] ?? []).some((f) =>
          onDisk.has(f.toLowerCase().replace(/\.(pkz|zip|rar|7z|pnt)$/, "")),
        ) || fuzzy.has(product),
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
    switch (sort) {
      case "notInstalled":
        return list.sort(
          (a, b) =>
            Number(a.installed) - Number(b.installed) || a.product.localeCompare(b.product),
        );
      // "Recently purchased" is offered by the shared sort list and cannot be honoured here:
      // WooCommerce's downloads table publishes no order date. It falls back to alphabetical
      // rather than to an arbitrary order that would look like a date and not be one.
      default:
        return list.sort((a, b) => a.product.localeCompare(b.product));
    }
  }, [purchases, debounced, category, sort, notInstalledOnly, rootsFor]);

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

  const [pending, setPending] = useState<{
    purchase: Purchase;
    file: ShopItem;
    modType: ModType;
    destOptions: DestOption[];
    suggestions: string[];
    folderCounts: Map<string, number>;
    initialFolder: string;
  } | null>(null);
  /** Set instead of `pending` when the thing is already installed — confirm first. */
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
      startHubInstall({
        slug: file.slug,
        title: purchase.product,
        subpath: modType.installSubpath,
        destFolder,
        item: file,
      });
      setInstalledBump((n) => n + 1);
    },
    [pending, game, startHubInstall],
  );

  const logout = useCallback(async () => {
    await hubLogout();
    setLoggedIn(false);
    setItems([]);
    setListings({});
    setError(null);
    setOpenProduct(null);
  }, []);

  /**
   * The purchase whose detail is open. Resolved from the live list rather than held as an
   * object, so the page keeps up with an install finishing underneath it.
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
          <h2 className="text-[15px] font-semibold">{t("hub.signInTitle")}</h2>
          <p className="text-[12.5px] text-muted-foreground">{t("hub.signInBody")}</p>
        </div>
        <Button onClick={() => void hubLogin()}>
          <Store className="size-4" /> {t("hub.signIn")}
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

      <AlertDialog open={confirming !== null} onOpenChange={(o) => !o && setConfirming(null)}>
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
          load={hubDetail}
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
            : t("hub.myDownloads")}
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
                {/* "Recently purchased" is dropped rather than shown and ignored: this store
                    publishes no purchase date, and an order that pretends to be one is worse
                    than an option that isn't there. */}
                {PURCHASE_SORTS.filter((s) => s.value !== "recentlyPurchased").map((s) => (
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
            {t("hub.empty")}
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
                onOpen={p.listing ? () => setOpenProduct(p.product) : undefined}
              />
            ))}
          </div>
        )}
      </div>

      {dialogs}
    </div>
  );
}
