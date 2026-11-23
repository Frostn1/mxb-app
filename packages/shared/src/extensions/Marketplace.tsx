import { useCallback, useEffect, useMemo, useRef, useState, type ComponentType, type ReactNode } from "react";
import { toast } from "sonner";
import { ArrowLeft, Download, RefreshCw, Search, Star } from "lucide-react";
import { Button } from "../Components/ui/button";
import { Input } from "../Components/ui/input";
import { useT, type BaseTKey } from "../i18n/context";
import { cn } from "../lib/utils";
import {
  catalogGet,
  catalogInspect,
  catalogInstalled,
  catalogLatest,
  catalogMyRating,
  catalogRate,
  catalogSearch,
  type CatalogCard,
  type CatalogDetail,
  type CatalogTier,
} from "./api";
import ExtensionsPage, { InstallDialog, PERM_LABEL } from "./ExtensionsPage";
import BuildPage from "./BuildPage";
import { cleanLabel, type InspectedPackage } from "./manifest";
import { isPermission } from "./permissions";
import { useExtensions } from "./store";
import { TrustLabel, trustColor } from "./trust";

const CATEGORIES = ["", "themes", "tabs", "integrations", "ai-tools", "game"] as const;
const CATEGORY_LABEL: Record<(typeof CATEGORIES)[number], BaseTKey> = {
  "": "ext.cat.all",
  themes: "ext.cat.themes",
  tabs: "ext.cat.tabs",
  integrations: "ext.cat.integrations",
  "ai-tools": "ext.cat.aiTools",
  game: "ext.cat.game",
};
const SORTS = ["popular", "new", "rating"] as const;
const SORT_LABEL: Record<(typeof SORTS)[number], BaseTKey> = {
  popular: "ext.sort.popular",
  new: "ext.sort.new",
  rating: "ext.sort.rating",
};

type Tab = "browse" | "installed" | "updates" | "build";

/** `1.2.0` > `1.10.0`? Numeric, the way the backend compares. */
export function newer(a: string, b: string): boolean {
  const pa = a.split(/[-+]/)[0].split(".").map(Number);
  const pb = b.split(/[-+]/)[0].split(".").map(Number);
  for (let i = 0; i < 3; i++) if ((pa[i] ?? 0) !== (pb[i] ?? 0)) return (pa[i] ?? 0) > (pb[i] ?? 0);
  return false;
}

function Stars({ value, size = "size-3.5" }: { value: number | null; size?: string }) {
  if (value == null) return null;
  const v = value;
  return (
    <span className="inline-flex items-center" aria-label={value == null ? "" : `${value} / 5`}>
      {[1, 2, 3, 4, 5].map((i) => (
        <Star key={i} className={cn(size, i <= Math.round(v) ? "fill-primary text-primary" : "text-foreground/25")} />
      ))}
    </span>
  );
}

function compact(n: number): string {
  return n >= 1000 ? `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}k` : String(n);
}

/** The letter tile shown when a listing has no screenshot. */
function Tile({ name }: { name: string }) {
  return (
    <div className="flex size-full items-center justify-center bg-foreground/[0.06] text-[28px] font-semibold text-foreground/40">
      {name.slice(0, 1).toUpperCase()}
    </div>
  );
}

function useInstallFlow() {
  const [pkg, setPkg] = useState<InspectedPackage | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const start = useCallback(async (id: string) => {
    setBusy(id);
    try {
      setPkg(await catalogInspect(id));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(null);
    }
  }, []);
  const dialog = pkg ? (
    <InstallDialog
      pkg={pkg}
      onClose={() => setPkg(null)}
      onInstalled={(id) => void catalogInstalled(id).catch(() => {})}
    />
  ) : null;
  return { start, busy, dialog };
}

function ActionButton({
  card,
  installedVersion,
  busy,
  onInstall,
}: {
  card: Pick<CatalogCard, "id" | "version">;
  installedVersion: string | undefined;
  busy: boolean;
  onInstall: () => void;
}) {
  const t = useT();
  if (installedVersion && !newer(card.version, installedVersion)) {
    return (
      <Button size="sm" variant="secondary" disabled>
        {t("ext.installedBadge")}
      </Button>
    );
  }
  return (
    <Button size="sm" disabled={busy} onClick={onInstall}>
      {installedVersion ? <RefreshCw className="size-3.5" /> : <Download className="size-3.5" />}
      {installedVersion ? t("ext.update") : t("ext.install")}
    </Button>
  );
}

function Card({ c, installed, busy, onOpen, onInstall }: {
  c: CatalogCard;
  installed: string | undefined;
  busy: boolean;
  onOpen: () => void;
  onInstall: () => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col overflow-hidden rounded-lg border border-border bg-card">
      <button type="button" onClick={onOpen} className="aspect-[16/9] w-full cursor-default overflow-hidden border-b border-border">
        {c.thumbnail ? <img src={c.thumbnail} alt="" className="size-full object-cover" loading="lazy" /> : <Tile name={c.name} />}
      </button>
      <div className="flex flex-1 flex-col gap-1.5 p-3">
        <button type="button" onClick={onOpen} className="cursor-default text-left">
          <div className="truncate text-[13.5px] font-semibold text-foreground">{cleanLabel(c.name, 60)}</div>
          <div className="flex flex-wrap items-center gap-1.5 text-[11.5px] text-muted-foreground">
            <span>{cleanLabel(c.author, 60)}</span>
            <span aria-hidden>·</span>
            <TrustLabel verified={c.verified} suggested={c.suggested} />
          </div>
        </button>
        <p className="line-clamp-2 text-[12px] text-foreground/75">{cleanLabel(c.summary, 160)}</p>
        <div className="mt-auto flex items-center justify-between gap-2 pt-1.5">
          <span className="flex min-w-0 items-center gap-2 whitespace-nowrap text-[11.5px] text-muted-foreground">
            {c.ratings > 0 && c.rating != null && (
              <>
                <Stars value={c.rating} size="size-3" />
                <span>{c.rating.toFixed(1)}</span>
                <span>·</span>
              </>
            )}
            <span>{t("ext.installs", { n: compact(c.installs) })}</span>
          </span>
          <ActionButton card={c} installedVersion={installed} busy={busy} onInstall={onInstall} />
        </div>
      </div>
    </div>
  );
}

function Detail({ id, installed, busy, onBack, onInstall }: {
  id: string;
  installed: string | undefined;
  busy: boolean;
  onBack: () => void;
  onInstall: () => void;
}) {
  const t = useT();
  const [d, setD] = useState<CatalogDetail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mine, setMine] = useState<number | null>(null);
  const [shot, setShot] = useState(0);

  useEffect(() => {
    let live = true;
    setD(null);
    catalogGet(id).then((x) => live && setD(x), (e) => live && setError(String(e)));
    catalogMyRating(id).then((r) => live && setMine(r), () => {});
    return () => {
      live = false;
    };
  }, [id]);

  const rate = async (stars: number) => {
    try {
      const r = await catalogRate(id, stars);
      setMine(r.mine);
      setD((x) => (x ? { ...x, rating: r.rating, ratings: r.ratings } : x));
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <div className="mx-auto max-w-[980px] space-y-5">
      <Button size="sm" variant="ghost" onClick={onBack}>
        <ArrowLeft className="size-3.5" />
        {t("ext.back")}
      </Button>
      {error && <p className="text-[12.5px] text-destructive">{error}</p>}
      {d && (
        <>
          <div className="flex flex-wrap items-start justify-between gap-4">
            <div className="min-w-0">
              <h1 className="text-[22px] font-semibold text-foreground">{cleanLabel(d.name, 60)}</h1>
              <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-[12.5px] text-muted-foreground">
                <span>{cleanLabel(d.author, 60)}</span>
                <span aria-hidden>·</span>
                <TrustLabel verified={d.verified} suggested={d.suggested} />
              </div>
              {!d.verified && (
                <p className="mt-1 text-[12.5px]" style={{ color: trustColor("warning") }}>
                  {t("ext.notChecked")}
                </p>
              )}
              <div className="mt-2 flex items-center gap-2 text-[12px] text-muted-foreground">
                {d.ratings > 0 && d.rating != null && (
                  <>
                    <Stars value={d.rating} />
                    <span>{`${d.rating.toFixed(1)} (${d.ratings})`}</span>
                    <span>·</span>
                  </>
                )}
                <span>{t("ext.installs", { n: compact(d.installs) })}</span>
              </div>
            </div>
            <ActionButton card={d} installedVersion={installed} busy={busy} onInstall={onInstall} />
          </div>

          {d.screenshots.length > 0 && (
            <div className="space-y-2">
              <div className="aspect-[16/9] w-full overflow-hidden rounded-lg border border-border bg-card">
                <img src={d.screenshots[shot]} alt="" className="size-full object-contain" />
              </div>
              {d.screenshots.length > 1 && (
                <div className="flex gap-2">
                  {d.screenshots.map((s, i) => (
                    <button
                      key={s}
                      type="button"
                      onClick={() => setShot(i)}
                      className={cn("h-14 w-24 cursor-default overflow-hidden rounded border", i === shot ? "border-primary" : "border-border opacity-70")}
                    >
                      <img src={s} alt="" className="size-full object-cover" />
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}

          <div className="grid gap-6 md:grid-cols-[1fr_280px]">
            <div className="space-y-4">
              <p className="whitespace-pre-line text-[13px] leading-relaxed text-foreground/85">{cleanLabel(d.description, 400) || cleanLabel(d.summary, 160)}</p>
              <div>
                <div className="mb-1.5 text-[11.5px] font-semibold uppercase tracking-wide text-muted-foreground">{t("ext.permissions")}</div>
                {d.permissions.filter(isPermission).length === 0 ? (
                  <p className="text-[12.5px] text-muted-foreground">{t("ext.noPermissions")}</p>
                ) : (
                  <ul className="space-y-1 text-[12.5px] text-foreground/85">
                    {d.permissions.filter(isPermission).map((p) => (
                      <li key={p}>
                        {t(p === "network" && d.network.length === 0 ? "ext.perm.networkNoHosts" : PERM_LABEL[p])}
                        {p === "network" && d.network.length > 0 && (
                          <span className="block text-[11.5px] text-muted-foreground">{d.network.join(", ")}</span>
                        )}
                      </li>
                    ))}
                  </ul>
                )}
              </div>
            </div>
            <dl className="space-y-2 border-t border-border pt-3 text-[12px] md:border-l md:border-t-0 md:pl-5 md:pt-0">
              <Row k={t("ext.version")} v={d.version} />
              <Row k={t("ext.size")} v={`${(d.size / 1024).toFixed(0)} KB`} />
              <Row k={t("ext.updatedOn")} v={new Date(d.updatedAt * 1000).toLocaleDateString()} />
              <Row k={t("ext.categories")} v={d.categories.map((c) => t(CATEGORY_LABEL[c as keyof typeof CATEGORY_LABEL] ?? "ext.cat.all")).join(", ")} />
              {d.tags.length > 0 && <Row k={t("ext.tags")} v={d.tags.join(", ")} />}
              <div className="pt-2">
                <div className="mb-1 text-muted-foreground">{t("ext.yourRating")}</div>
                <span className="inline-flex">
                  {[1, 2, 3, 4, 5].map((i) => (
                    <button key={i} type="button" className="cursor-default p-0.5" aria-label={`${i}`} onClick={() => void rate(i)}>
                      <Star className={cn("size-4", mine && i <= mine ? "fill-primary text-primary" : "text-foreground/30 hover:text-primary")} />
                    </button>
                  ))}
                </span>
              </div>
            </dl>
          </div>
        </>
      )}
    </div>
  );
}

function Row({ k, v }: { k: string; v: string }) {
  return (
    <div className="flex justify-between gap-3">
      <dt className="text-muted-foreground">{k}</dt>
      <dd className="text-right text-foreground/85">{v}</dd>
    </div>
  );
}

/**
 * The Extensions tab: the marketplace, what is installed, and updates. Search runs as you
 * type (debounced); the catalog answers from an index, and the last answer stays on screen
 * while the next one is on its way, so typing never blanks the grid.
 */
export default function Marketplace({ TabsSlot }: { TabsSlot?: ComponentType<{ children: ReactNode }> }) {
  const t = useT();
  const s = useExtensions();
  const [tab, setTab] = useState<Tab>("browse");
  const [q, setQ] = useState("");
  const [category, setCategory] = useState<(typeof CATEGORIES)[number]>("");
  const [sort, setSort] = useState<(typeof SORTS)[number]>("popular");
  const [tier, setTier] = useState<CatalogTier>("");
  const [results, setResults] = useState<CatalogCard[] | null>(null);
  const [suggested, setSuggested] = useState<CatalogCard[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);
  const [latest, setLatest] = useState<CatalogCard[]>([]);
  const flow = useInstallFlow();
  const seq = useRef(0);

  const installed = useMemo(() => new Map(s.installed.map((e) => [e.id, e.version])), [s.installed]);

  useEffect(() => {
    const mine = ++seq.current;
    const timer = window.setTimeout(() => {
      catalogSearch(q, category, sort, 0, tier)
        .then((r) => {
          if (mine === seq.current) {
            setResults(r.extensions);
            setError(null);
          }
        })
        .catch((e) => mine === seq.current && setError(String(e)));
    }, q ? 160 : 0);
    return () => window.clearTimeout(timer);
  }, [q, category, sort, tier]);

  const showSuggested = q === "" && category === "" && tier === "";
  useEffect(() => {
    if (!showSuggested) return;
    void catalogSearch("", "", "popular", 0, "suggested").then((r) => setSuggested(r.extensions), () => setSuggested([]));
  }, [showSuggested]);

  useEffect(() => {
    void catalogLatest().then((r) => setLatest(r.extensions), () => {});
  }, [s.installed]);

  const updates = latest.filter((c) => installed.has(c.id) && newer(c.version, installed.get(c.id)!));

  const tabs: { id: Tab; label: string }[] = [
    { id: "browse", label: t("ext.browse") },
    { id: "installed", label: `${t("ext.installedTab")} ${s.installed.length ? s.installed.length : ""}`.trim() },
    { id: "updates", label: `${t("ext.updatesTab")} ${updates.length ? updates.length : ""}`.trim() },
    { id: "build", label: t("ext.build.tab") },
  ];
  const selectTab = (next: Tab) => {
    setOpen(null);
    setTab(next);
  };
  const tabButtons = (context: boolean) => tabs.map((x) => (
    <button
      key={x.id}
      type="button"
      onClick={() => selectTab(x.id)}
      className={cn(
        context
          ? "relative flex cursor-default items-center font-cond text-[12.5px] font-semibold tracking-[-0.02em] transition-colors"
          : "cursor-default rounded-md px-3 py-1.5 text-[12.5px] font-medium",
        tab === x.id
          ? context ? "text-foreground" : "bg-foreground/[0.10] text-foreground"
          : "text-muted-foreground hover:text-foreground",
      )}
    >
      {x.label}
      {context && tab === x.id && <span className="mxb-tab-marker absolute inset-x-[-3px] bottom-0 h-[2px] rounded-full bg-primary" />}
    </button>
  ));

  return (
    // `pt-1` keeps the search box's ring inside the scroller, which clips at its own edge.
    <div className="h-full overflow-y-auto px-6 pb-10 pt-1">
      {TabsSlot && <TabsSlot>{tabButtons(true)}</TabsSlot>}
      {open ? (
        <Detail
          id={open}
          installed={installed.get(open)}
          busy={flow.busy === open}
          onBack={() => setOpen(null)}
          onInstall={() => void flow.start(open)}
        />
      ) : (
        <div className={cn("mx-auto space-y-4", tab === "build" ? "max-w-[860px]" : "max-w-[1200px]")}>
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 pt-1">
            {!TabsSlot && <div className="flex gap-1">{tabButtons(false)}</div>}
            {tab === "browse" && (
              <>
                <div className="relative ml-auto min-w-[200px] flex-1 sm:max-w-[360px]">
                  <Search className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground" />
                  <Input
                    className="h-8 pl-8"
                    placeholder={t("ext.searchPlaceholder")}
                    value={q}
                    onChange={(e) => setQ(e.target.value)}
                    autoFocus
                  />
                </div>
                <select
                  className="h-8 rounded-md border border-input bg-transparent px-2 text-[12.5px]"
                  value={sort}
                  onChange={(e) => setSort(e.target.value as (typeof SORTS)[number])}
                  aria-label={t("ext.sortBy")}
                >
                  {SORTS.map((x) => (
                    <option key={x} value={x}>
                      {t(SORT_LABEL[x])}
                    </option>
                  ))}
                </select>
              </>
            )}
          </div>

          {tab === "browse" && (
            <>
              <div className="flex flex-wrap items-center gap-1.5">
                <button
                  type="button"
                  onClick={() => setTier(tier === "verified" ? "" : "verified")}
                  className={cn(
                    "cursor-default rounded-full border px-3 py-1 text-[12px]",
                    tier === "verified" ? "border-primary text-foreground" : "border-border text-muted-foreground hover:text-foreground",
                  )}
                >
                  {t("ext.tier.verifiedOnly")}
                </button>
                <span aria-hidden className="mx-1 h-5 w-px bg-border" />
                {CATEGORIES.map((c) => (
                  <button
                    key={c || "all"}
                    type="button"
                    onClick={() => setCategory(c)}
                    className={cn(
                      "cursor-default rounded-full border px-3 py-1 text-[12px]",
                      category === c ? "border-primary text-foreground" : "border-border text-muted-foreground hover:text-foreground",
                    )}
                  >
                    {t(CATEGORY_LABEL[c])}
                  </button>
                ))}
              </div>
              {error && !results && <p className="py-8 text-center text-[12.5px] text-muted-foreground">{t("ext.catalogOffline")}</p>}
              {results && results.length === 0 && <p className="py-8 text-center text-[12.5px] text-muted-foreground">{t("ext.noResults")}</p>}
              {showSuggested && suggested.length > 0 && (
                <section className="space-y-2">
                  <h2 className="text-[13.5px] font-semibold text-foreground">{t("ext.suggested")}</h2>
                  <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-3">
                    {suggested.map((c) => (
                      <Card
                        key={c.id}
                        c={c}
                        installed={installed.get(c.id)}
                        busy={flow.busy === c.id}
                        onOpen={() => setOpen(c.id)}
                        onInstall={() => void flow.start(c.id)}
                      />
                    ))}
                  </div>
                </section>
              )}
              {showSuggested && suggested.length > 0 && <h2 className="text-[13.5px] font-semibold text-foreground">{t("ext.allExtensions")}</h2>}
              <div className="grid grid-cols-[repeat(auto-fill,minmax(250px,1fr))] gap-3">
                {(results ?? []).map((c) => (
                  <Card
                    key={c.id}
                    c={c}
                    installed={installed.get(c.id)}
                    busy={flow.busy === c.id}
                    onOpen={() => setOpen(c.id)}
                    onInstall={() => void flow.start(c.id)}
                  />
                ))}
              </div>
            </>
          )}

          {tab === "installed" && <ExtensionsPage />}

          {tab === "build" && <BuildPage />}

          {tab === "updates" && (
            <div className="border-t border-border">
              {updates.length === 0 ? (
                <p className="py-8 text-center text-[12.5px] text-muted-foreground">{t("ext.noUpdates")}</p>
              ) : (
                updates.map((c) => (
                  <div key={c.id} className="flex items-center justify-between gap-4 border-b border-border py-3">
                    <div>
                      <div className="text-[13.5px] font-semibold">{cleanLabel(c.name, 60)}</div>
                      <div className="text-[12px] text-muted-foreground">
                        {installed.get(c.id)} → {c.version}
                      </div>
                    </div>
                    <Button size="sm" disabled={flow.busy === c.id} onClick={() => void flow.start(c.id)}>
                      <RefreshCw className="size-3.5" />
                      {t("ext.update")}
                    </Button>
                  </div>
                ))
              )}
            </div>
          )}
        </div>
      )}
      {flow.dialog}
    </div>
  );
}
