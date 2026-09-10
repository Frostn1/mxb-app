import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Search,
  RefreshCw,
  Loader2,
  Lock,
  Users,
  Signal,
  Copy,
  Plug,
  ServerOff,
  EyeOff,
  Palette,
  Star,
  ChevronUp,
  ChevronDown,
  Globe,
  LayoutGrid,
  List,
  PackageCheck,
  UserCheck,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
import { Segmented } from "@frost/shared/Components/ui/segmented";
import {
  Select,
  SelectTrigger,
  SelectValue,
  SelectContent,
  SelectItem,
} from "@frost/shared/Components/ui/select";
import { ContextBarRight } from "../Shell/ContextBar";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import {
  listMasterServers,
  joinServer,
  serversWithPaintSync,
  installedTrackIds,
  type MasterServer,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { useFavourites } from "@/lib/useFavourites";
import {
  REGION_LABEL_KEY,
  REGION_ORDER,
  canonicalRegion,
  type RegionKey,
} from "@/lib/serverRegion";
import { buildTrackIndex, matchTrack, type TrackIndex } from "@/lib/trackContent";
import { useTrackCatalog } from "@/lib/useTrackCatalog";
import TrackContentAction from "./TrackContentAction";
import ServerCard from "./ServerCard";
import JoinServerDialog from "../Shell/JoinServerDialog";
import ServerDetail from "./ServerDetail";

type ViewMode = "cards" | "list";
const VIEW_KEY = "mxb:serversView:v1";

/**
 * The live MX Bikes server list, read straight from PiBoSo's master server — the same
 * population the in-game WORLD browser shows, with the IP the game never surfaces. Joining a
 * row reuses the existing `-directconnect` launch, so this is the one-click path the address
 * dialog only hinted at.
 *
 * The fetch is one Rust command; everything the master needs (auth, the protocol) lives
 * behind it. A build without the browser, or a master that won't answer, comes back as a
 * plain error string this renders rather than a blank tab.
 *
 * On top of the fetch: any column sorts (click its header to flip), servers can be starred
 * to a favourites-only view, and the host's own `location` text is grouped into a small set
 * of regions for the filter (see `lib/serverRegion`) — three conveniences ported from the
 * standalone browser.
 */

/** Every sortable column. These are the ones upstream's table actually shows. */
type SortMode = "players" | "ping" | "name" | "region" | "track";
type SortDir = "asc" | "desc";

/**
 * Which way a column runs when first clicked. Descending for "more is what I want" (players),
 * ascending for everything else — clicking Players and landing on the empty servers reads as
 * a bug, not a default.
 */
const DEFAULT_DIR: Record<SortMode, SortDir> = {
  players: "desc",
  ping: "asc",
  name: "asc",
  region: "asc",
  track: "asc",
};

const Servers = () => {
  const t = useT();
  const [servers, setServers] = useState<MasterServer[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [query, setQuery] = useState("");
  const [joining, setJoining] = useState<string | null>(null);
  const [joinOpen, setJoinOpen] = useState(false);
  const [detail, setDetail] = useState<MasterServer | null>(null);
  // Spam and cheat-advertising servers are marked by the backend, not dropped, so this can
  // reveal them. Off by default: the whole point is not to have to read past them.
  const [showHidden, setShowHidden] = useState(false);
  // Riders running paint sync, by address. Which rows are worth joining used to mean opening
  // each one and reading its Riders panel — a request per server to answer a question about
  // the list. This is one request for all of them, and it marks the rows.
  const [paintSync, setPaintSync] = useState<Record<string, number>>({});

  // Ported conveniences: sort order, region filter, and a favourites-only view.
  const [sort, setSort] = useState<SortMode>("players");
  const [dir, setDir] = useState<SortDir>(DEFAULT_DIR.players);
  const [region, setRegion] = useState<string>("all");
  const [favesOnly, setFavesOnly] = useState(false);
  // Hide servers running a track the player doesn't have — no point joining one you can't load.
  const [installedOnly, setInstalledOnly] = useState(false);
  // Hide empty servers — an empty one is rarely what someone opening the browser is after.
  const [hideEmpty, setHideEmpty] = useState(false);
  const favs = useFavourites(servers);

  // Picture grid or dense table — a sticky per-machine preference.
  const [view, setView] = useState<ViewMode>(() => {
    try {
      return localStorage.getItem(VIEW_KEY) === "list" ? "list" : "cards";
    } catch {
      return "cards";
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(VIEW_KEY, view);
    } catch {
      // Storage disabled; the choice still holds for this session.
    }
  }, [view]);

  // Which tracks are on disk, so a row can say "you have this" or offer to fetch it. Scanned
  // once on mount; a track that finishes downloading shows as installed on the next refresh.
  const [trackIndex, setTrackIndex] = useState<TrackIndex>(() => buildTrackIndex([]));
  useEffect(() => {
    installedTrackIds()
      .then((tracks) => setTrackIndex(buildTrackIndex(tracks)))
      .catch(() => setTrackIndex(buildTrackIndex([])));
  }, []);
  // Which of the missing tracks can actually be downloaded, for the whole list in one call.
  const catalog = useTrackCatalog(servers, trackIndex);

  // One fetch at a time. Two overlapping ones each sign in to Steam, and the loser's
  // failure used to replace the winner's list with an error.
  const inFlight = useRef(false);
  // What the tab is showing, for the failure path — `load` holds no state of its own.
  const onScreen = useRef<MasterServer[] | null>(null);
  const load = useCallback(() => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    setError(null);
    listMasterServers()
      .then((list) => {
        onScreen.current = list;
        setServers(list);
        // After the list, never with it: the browser has to draw whether or not the control
        // plane answers, and badges arriving a moment later is the right trade for that.
        serversWithPaintSync(
          list.map((s) => ({ name: s.name, address: s.address })),
        )
          .then(setPaintSync)
          .catch(() => setPaintSync({}));
      })
      .catch((e: unknown) => {
        const message = typeof e === "string" ? e : String(e);
        setError(message);
        // A failed refresh is not an empty list: keep what's on screen and say so instead.
        if (onScreen.current?.length) toast.error(message);
        else setServers([]);
      })
      .finally(() => {
        inFlight.current = false;
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // Nothing to un-star means nothing to look at: never sit on an empty favourites view.
  useEffect(() => {
    if (favesOnly && favs.count === 0) setFavesOnly(false);
  }, [favesOnly, favs.count]);

  /** How many rows the filter caught, whether or not they're being shown. */
  const hiddenCount = useMemo(
    () => (servers ?? []).filter((s) => s.hidden).length,
    [servers],
  );

  /** The region buckets actually present, in a fixed order — not the raw host strings. */
  const regions = useMemo(() => {
    const present = new Set<RegionKey>();
    for (const s of servers ?? []) present.add(canonicalRegion(s.location));
    return REGION_ORDER.filter((r) => present.has(r));
  }, [servers]);

  /** Sort by `key`, flipping direction when it's already the active column. */
  const sortBy = useCallback(
    (key: SortMode) => {
      setDir((d) => (sort === key ? (d === "asc" ? "desc" : "asc") : DEFAULT_DIR[key]));
      setSort(key);
    },
    [sort],
  );

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    let list = (servers ?? []).filter((s) => showHidden || !s.hidden);
    if (q) {
      list = list.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          s.track.toLowerCase().includes(q) ||
          s.location.toLowerCase().includes(q) ||
          s.address.toLowerCase().includes(q),
      );
    }
    if (favesOnly) {
      // Starring *is* the filter here: don't also apply the region pill, which would hide a
      // favourite that happens to sit in another region from the one you're browsing.
      list = list.filter((s) => favs.has(s.address));
    } else if (region !== "all") {
      list = list.filter((s) => canonicalRegion(s.location) === region);
    }
    // Content filter, orthogonal to the scope above: drop anything whose track isn't on disk.
    if (installedOnly) {
      list = list.filter((s) => matchTrack(trackIndex, s.track).state !== "missing");
    }
    if (hideEmpty) {
      list = list.filter((s) => s.players > 0);
    }

    const flip = dir === "desc" ? -1 : 1;
    const sorted = [...list];
    sorted.sort((a, b) => {
      // Name breaks every tie, so the order is total and a re-render can't reshuffle rows
      // that compare equal.
      const byName = a.name.localeCompare(b.name);
      switch (sort) {
        case "players":
          return (a.players - b.players) * flip || byName;
        case "ping": {
          const [x, y] = [a.pingMs, b.pingMs];
          if (x === null || y === null) {
            // Unreachable/unmeasured always last, whichever way the column runs.
            if (x === y) return byName;
            return x === null ? 1 : -1;
          }
          return (x - y) * flip || byName;
        }
        case "region":
          return (
            canonicalRegion(a.location).localeCompare(canonicalRegion(b.location)) * flip ||
            byName
          );
        case "track":
          return a.track.localeCompare(b.track) * flip || byName;
        case "name":
        default:
          return byName * flip;
      }
    });
    return sorted;
  }, [servers, query, showHidden, favesOnly, region, installedOnly, hideEmpty, trackIndex, favs, sort, dir]);

  const join = useCallback(
    async (address: string) => {
      if (joining) return;
      setJoining(address);
      try {
        const outcome = await joinServer(address);
        if (outcome === "already_running") {
          toast.info(t("join.alreadyRunning"));
        } else {
          toast.success(t("join.launching", { address }));
        }
      } catch (e) {
        toast.error(typeof e === "string" ? e : t("serverBrowser.joinFailed"));
      } finally {
        setJoining(null);
      }
    },
    [joining, t],
  );

  const copy = useCallback(
    (address: string) => {
      navigator.clipboard
        .writeText(address)
        .then(() => toast.success(t("serverBrowser.copied")))
        .catch(() => {});
    },
    [t],
  );

  /** A clickable column header that shows and flips the sort. */
  const SortHead = ({
    col,
    label,
    className,
  }: {
    col: SortMode;
    label: string;
    className?: string;
  }) => (
    <th className={cn("px-2 py-2.5 font-semibold", className)}>
      <button
        type="button"
        onClick={() => sortBy(col)}
        className="inline-flex items-center gap-1 uppercase tracking-wide hover:text-muted-foreground"
      >
        {label}
        {sort === col &&
          (dir === "asc" ? (
            <ChevronUp className="size-3" />
          ) : (
            <ChevronDown className="size-3" />
          ))}
      </button>
    </th>
  );

  return (
    <div className="flex h-full flex-col">
      <ContextBarRight>
        {servers && servers.length > 0 && (
          <span className="tabular-figures text-[12.5px] text-faint">
            {t("serverBrowser.count", { count: servers.length - (showHidden ? 0 : hiddenCount) })}
          </span>
        )}
        <Segmented<ViewMode>
          size="sm"
          value={view}
          onChange={setView}
          options={[
            { value: "cards", label: <LayoutGrid className="size-3.5" /> },
            { value: "list", label: <List className="size-3.5" /> },
          ]}
        />
        {favs.count > 0 && (
          <button
            type="button"
            onClick={() => setFavesOnly((v) => !v)}
            title={t("serverBrowser.favesOnly")}
            className={cn(
              "flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap border border-input px-2.5 text-[12px]",
              favesOnly ? "bg-card text-muted-foreground" : "text-faint hover:text-muted-foreground",
            )}
          >
            <Star className={cn("size-3.5", favesOnly && "fill-current")} />
            {t("serverBrowser.favesOnly")}
          </button>
        )}
        <button
          type="button"
          onClick={() => setInstalledOnly((v) => !v)}
          title={t("serverBrowser.installedOnlyHelp")}
          className={cn(
            "flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap border border-input px-2.5 text-[12px]",
            installedOnly ? "bg-card text-muted-foreground" : "text-faint hover:text-muted-foreground",
          )}
        >
          <PackageCheck className="size-3.5" />
          {t("serverBrowser.installedOnly")}
        </button>
        <button
          type="button"
          onClick={() => setHideEmpty((v) => !v)}
          title={t("serverBrowser.hideEmptyHelp")}
          className={cn(
            "flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap border border-input px-2.5 text-[12px]",
            hideEmpty ? "bg-card text-muted-foreground" : "text-faint hover:text-muted-foreground",
          )}
        >
          <UserCheck className="size-3.5" />
          {t("serverBrowser.hideEmpty")}
        </button>
        {!favesOnly && regions.length > 1 && (
          <Select value={region} onValueChange={setRegion}>
            <SelectTrigger className="h-7 w-[150px] bg-card text-[12px]">
              <Globe className="size-3.5 text-faint" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">{t("serverBrowser.region.all")}</SelectItem>
              {regions.map((r) => (
                <SelectItem key={r} value={r}>
                  {t(REGION_LABEL_KEY[r])}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        {hiddenCount > 0 && (
          <button
            type="button"
            onClick={() => setShowHidden((v) => !v)}
            title={t("serverBrowser.hiddenHelp")}
            className={cn(
              "flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap border border-input px-2.5 text-[12px]",
              showHidden ? "bg-card text-muted-foreground" : "text-faint hover:text-muted-foreground",
            )}
          >
            <EyeOff className="size-3.5" />
            {showHidden
              ? t("serverBrowser.hideFiltered")
              : t("serverBrowser.hiddenCount", { count: hiddenCount })}
          </button>
        )}
        <div className="flex h-7 w-[220px] items-center gap-2 border border-input bg-card px-2.5">
          <Search className="size-3.5 text-faint" />
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={t("serverBrowser.searchPlaceholder")}
            className="w-full bg-transparent text-[12.5px] placeholder:text-faint focus:outline-none"
          />
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={load}
          disabled={loading}
          title={t("serverBrowser.refresh")}
        >
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
          {t("serverBrowser.refresh")}
        </Button>
        {/* Join by address, for a server the master list doesn't carry. It lived in the
            sidebar next to Play; with the sidebar gone this is where someone looks for
            it — the page that is already about joining servers. */}
        <Button variant="outline" size="sm" onClick={() => setJoinOpen(true)}>
          <Plug className="size-3.5" />
          {t("join.title")}
        </Button>
        <HelpHint title={t("servers.title")} description={t("serverBrowser.help")} />
      </ContextBarRight>

      <JoinServerDialog open={joinOpen} onOpenChange={setJoinOpen} onJoined={load} />
      <ServerDetail
        server={detail}
        onOpenChange={(open) => !open && setDetail(null)}
        onJoin={join}
        joining={joining}
      />

      <div className="min-h-0 flex-1 overflow-y-auto px-7 pb-6">
        {servers === null ? (
          <Centered>
            <Loader2 className="size-5 animate-spin text-faint" />
            <p className="text-[13px] text-faint">{t("serverBrowser.loading")}</p>
          </Centered>
        ) : error && servers.length === 0 ? (
          <Centered>
            <ServerOff className="size-6 text-faint" />
            <p className="max-w-[420px] text-center text-[13px] text-muted-foreground">
              {error}
            </p>
            <Button variant="outline" size="sm" onClick={load}>
              <RefreshCw className="size-3.5" />
              {t("serverBrowser.retry")}
            </Button>
          </Centered>
        ) : shown.length === 0 ? (
          <Centered>
            <ServerOff className="size-6 text-faint" />
            <p className="text-[13px] text-faint">
              {favesOnly ? t("serverBrowser.favesEmpty") : t("serverBrowser.empty")}
            </p>
          </Centered>
        ) : view === "cards" ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
            {shown.map((s, i) => (
              <ServerCard
                key={`${s.address}-${i}`}
                server={s}
                match={matchTrack(trackIndex, s.track)}
                known={catalog.found.get(s.track)}
                pending={catalog.pending.has(s.track)}
                joining={joining === s.address}
                onJoin={() => join(s.address)}
                favourite={favs.has(s.address)}
                onToggleFavourite={() =>
                  favs.toggle({ address: s.address, name: s.name, track: s.track })
                }
              />
            ))}
          </div>
        ) : (
          <div className="overflow-hidden rounded-xl border border-input">
            <table className="w-full border-collapse text-[13px]">
              <thead>
                <tr className="border-b border-input bg-card text-left text-[11.5px] uppercase tracking-wide text-faint">
                  <th className="w-[36px] px-2 py-2.5" />
                  <SortHead col="name" label={t("serverBrowser.name")} className="px-3.5" />
                  <SortHead col="players" label={t("serverBrowser.players")} className="w-[92px]" />
                  <SortHead col="track" label={t("servers.track")} />
                  <SortHead col="region" label={t("serverBrowser.location")} />
                  <SortHead col="ping" label={t("serverBrowser.ping")} className="w-[72px]" />
                  <th className="px-2 py-2.5 font-semibold">{t("serverBrowser.address")}</th>
                  <th className="w-[110px] px-3.5 py-2.5" />
                </tr>
              </thead>
              <tbody>
                {shown.map((s, i) => (
                  <tr
                    key={`${s.address}-${i}`}
                    onClick={() => setDetail(s)}
                    className={cn(
                      "cursor-pointer border-b border-input/60 last:border-0 hover:bg-foreground/[0.03]",
                      // Revealed rows stay legible but visibly demoted, so nobody mistakes one
                      // for an ordinary result they just hadn't scrolled to.
                      s.hidden && "opacity-55",
                    )}
                  >
                    <td className="px-2 py-2.5">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          favs.toggle({ address: s.address, name: s.name, track: s.track });
                        }}
                        title={
                          favs.has(s.address)
                            ? t("serverBrowser.unstar")
                            : t("serverBrowser.star")
                        }
                        className={cn(
                          "inline-flex items-center justify-center rounded p-0.5",
                          favs.has(s.address)
                            ? "text-amber-400"
                            : "text-faint hover:text-muted-foreground",
                        )}
                      >
                        <Star className={cn("size-3.5", favs.has(s.address) && "fill-current")} />
                      </button>
                    </td>
                    <td className="px-3.5 py-2.5">
                      <div className="flex items-center gap-2">
                        {s.passworded && (
                          <Lock
                            className="size-3.5 shrink-0 text-faint"
                            aria-label={t("serverBrowser.passworded")}
                          />
                        )}
                        <span className="truncate font-medium" title={s.name}>
                          {s.name}
                        </span>
                        {s.hidden && (
                          <span
                            className="shrink-0 border border-input px-1.5 py-px text-[10.5px] uppercase tracking-wide text-faint"
                            title={t("serverBrowser.hiddenBecause", { reason: s.hidden })}
                          >
                            {t("serverBrowser.filtered")}
                          </span>
                        )}
                        {(paintSync[s.address] ?? 0) > 0 && (
                          <span
                            className="inline-flex shrink-0 items-center gap-1 border border-success/40 bg-success/10 px-1.5 py-px text-[10.5px] tabular-nums text-success"
                            title={t("serverBrowser.paintSyncHere", {
                              count: paintSync[s.address],
                            })}
                          >
                            <Palette className="size-3" />
                            {paintSync[s.address]}
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-2 py-2.5 tabular-nums text-muted-foreground">
                      <span className="inline-flex items-center gap-1.5">
                        <Users className="size-3.5 text-faint" />
                        {s.players}/{s.maxPlayers}
                      </span>
                    </td>
                    <td className="px-2 py-2.5 text-muted-foreground">
                      <div className="flex items-center gap-2">
                        <span
                          className="block max-w-[200px] truncate"
                          title={[s.track, s.trackLayout].filter(Boolean).join(" — ")}
                        >
                          {s.track || "—"}
                          {s.trackLayout && (
                            <span className="text-faint"> · {s.trackLayout}</span>
                          )}
                        </span>
                        {s.track && matchTrack(trackIndex, s.track).state === "missing" && (
                          // The action opens the row's detail unless the click is stopped here.
                          <span onClick={(e) => e.stopPropagation()}>
                            <TrackContentAction
                              track={s.track}
                              known={catalog.found.get(s.track)}
                              pending={catalog.pending.has(s.track)}
                              compact
                            />
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-2 py-2.5 text-muted-foreground">
                      <span className="block max-w-[140px] truncate" title={s.location}>
                        {s.location || "—"}
                      </span>
                    </td>
                    <td className="px-2 py-2.5 tabular-nums text-muted-foreground">
                      {s.pingMs === null ? (
                        "—"
                      ) : (
                        <span className="inline-flex items-center gap-1.5">
                          <Signal className="size-3.5 text-faint" />
                          {s.pingMs}
                        </span>
                      )}
                    </td>
                    <td className="px-2 py-2.5">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          copy(s.address);
                        }}
                        title={t("serverBrowser.copyAddress")}
                        className="inline-flex items-center gap-1.5 rounded-md px-1.5 py-0.5 font-mono text-[12px] text-muted-foreground hover:bg-foreground/[0.06] hover:text-foreground"
                      >
                        {s.address}
                        <Copy className="size-3 text-faint" />
                      </button>
                    </td>
                    <td className="px-3.5 py-2.5 text-right">
                      <Button
                        size="sm"
                        onClick={(e) => {
                          e.stopPropagation();
                          join(s.address);
                        }}
                        disabled={joining !== null || !s.joinable}
                        title={s.joinable ? undefined : t("serverBrowser.notJoinable")}
                      >
                        {joining === s.address ? (
                          <Loader2 className="size-3.5 animate-spin" />
                        ) : (
                          <Plug className="size-3.5" />
                        )}
                        {t("serverBrowser.join")}
                      </Button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};

/** The three non-list states (loading, error, empty) share this centered column. */
const Centered = ({ children }: { children: React.ReactNode }) => (
  <div className="flex h-full flex-col items-center justify-center gap-3 py-16">
    {children}
  </div>
);

export default Servers;
