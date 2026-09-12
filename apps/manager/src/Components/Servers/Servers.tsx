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
  UserCheck,
} from "lucide-react";
import { toast } from "sonner";
import { cn } from "@frost/shared/lib/utils";
import { Button } from "@frost/shared/Components/ui/button";
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
  type MasterServer,
} from "@frost/shared/api/mods";
import { useT } from "@/i18n";
import { useFavorites } from "@/lib/useFavorites";
import { REGION_LABEL_KEY, REGION_ORDER, canonicalRegion, type RegionKey } from "@/lib/serverRegion";
import JoinServerDialog from "../Shell/JoinServerDialog";
import ServerDetail from "./ServerDetail";

type SortMode = "players" | "ping" | "name" | "region" | "track";
type SortDir = "asc" | "desc";

/** The direction a column runs on its first click: most players first, everything else A–Z. */
const DEFAULT_DIR: Record<SortMode, SortDir> = {
  players: "desc",
  ping: "asc",
  name: "asc",
  region: "asc",
  track: "asc",
};

const HIDE_EMPTY_KEY = "mxb:serversHideEmpty:v1";

function readFlag(key: string, fallback: boolean): boolean {
  try {
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

/**
 * The live MX Bikes server list, read straight from PiBoSo's master server — the same
 * population the in-game WORLD browser shows, with the IP the game never surfaces. Joining a
 * row reuses the existing `-directconnect` launch, so this is the one-click path the address
 * dialog only hinted at.
 *
 * The fetch is one Rust command; everything the master needs (auth, the protocol) lives
 * behind it. A build without the browser, or a master that won't answer, comes back as a
 * plain error string this renders rather than a blank tab.
 */
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

  const [sort, setSort] = useState<SortMode>("players");
  const [dir, setDir] = useState<SortDir>(DEFAULT_DIR.players);
  const [region, setRegion] = useState<RegionKey | "all">("all");
  const [favesOnly, setFavesOnly] = useState(false);
  // On by default and remembered: an empty server is rarely what anyone opens the list for.
  const [hideEmpty, setHideEmpty] = useState(() => readFlag(HIDE_EMPTY_KEY, true));
  useEffect(() => {
    try {
      localStorage.setItem(HIDE_EMPTY_KEY, hideEmpty ? "1" : "0");
    } catch {
      // Storage disabled; the choice still holds for this session.
    }
  }, [hideEmpty]);
  const favs = useFavorites();

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

  // Never sit on an empty favorites view after the last star is removed.
  useEffect(() => {
    if (favesOnly && favs.count === 0) setFavesOnly(false);
  }, [favesOnly, favs.count]);

  /** How many rows the filter caught, whether or not they're being shown. */
  const hiddenCount = useMemo(
    () => (servers ?? []).filter((s) => s.hidden).length,
    [servers],
  );

  /** The regions actually present, so the dropdown never offers an empty one. */
  const regions = useMemo(() => {
    const present = new Set((servers ?? []).map((s) => canonicalRegion(s.location)));
    return REGION_ORDER.filter((r) => present.has(r));
  }, [servers]);

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
    // Favorites ignore the region, so a starred server abroad still shows.
    if (favesOnly) list = list.filter((s) => favs.has(s.address));
    else if (region !== "all") list = list.filter((s) => canonicalRegion(s.location) === region);
    if (hideEmpty) list = list.filter((s) => s.players > 0);

    const flip = dir === "desc" ? -1 : 1;
    return [...list].sort((a, b) => {
      // Name breaks ties, so rows that compare equal can't reshuffle between renders.
      const byName = a.name.localeCompare(b.name);
      switch (sort) {
        case "players":
          return (a.players - b.players) * flip || byName;
        case "ping":
          // Unmeasured servers go last whichever way the column runs.
          if (a.pingMs === null || b.pingMs === null) {
            return a.pingMs === b.pingMs ? byName : a.pingMs === null ? 1 : -1;
          }
          return (a.pingMs - b.pingMs) * flip || byName;
        case "region":
          return (
            canonicalRegion(a.location).localeCompare(canonicalRegion(b.location)) * flip ||
            byName
          );
        case "track":
          return a.track.localeCompare(b.track) * flip || byName;
        case "name":
          return byName * flip;
      }
    });
  }, [servers, query, showHidden, favesOnly, favs, region, hideEmpty, sort, dir]);

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

  const head = (col: SortMode, label: string, className?: string) => (
    <SortHead col={col} label={label} sort={sort} dir={dir} onSort={sortBy} className={className} />
  );

  return (
    <div className="flex h-full flex-col">
      <ContextBarRight>
        {servers && servers.length > 0 && (
          <span className="tabular-figures text-[12.5px] text-faint">
            {t("serverBrowser.count", { count: servers.length - (showHidden ? 0 : hiddenCount) })}
          </span>
        )}
        {favs.count > 0 && (
          <ToggleChip on={favesOnly} onClick={() => setFavesOnly((v) => !v)}>
            <Star className={cn("size-3.5", favesOnly && "fill-current")} />
            {t("serverBrowser.favesOnly")}
          </ToggleChip>
        )}
        <ToggleChip
          on={hideEmpty}
          onClick={() => setHideEmpty((v) => !v)}
          title={t("serverBrowser.hideEmptyHelp")}
        >
          <UserCheck className="size-3.5" />
          {t("serverBrowser.hideEmpty")}
        </ToggleChip>
        {!favesOnly && regions.length > 1 && (
          <Select value={region} onValueChange={(v) => setRegion(v as RegionKey | "all")}>
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
          <ToggleChip
            on={showHidden}
            onClick={() => setShowHidden((v) => !v)}
            title={t("serverBrowser.hiddenHelp")}
          >
            <EyeOff className="size-3.5" />
            {showHidden
              ? t("serverBrowser.hideFiltered")
              : t("serverBrowser.hiddenCount", { count: hiddenCount })}
          </ToggleChip>
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
        ) : (
          <div className="overflow-hidden rounded-xl border border-input">
            <table className="w-full border-collapse text-[13px]">
              <thead>
                <tr className="border-b border-input bg-card text-left text-[11.5px] uppercase tracking-wide text-faint">
                  <th className="w-[36px] py-2.5 pl-3.5" />
                  {head("name", t("serverBrowser.name"), "px-2")}
                  {head("players", t("serverBrowser.players"), "w-[92px] px-2")}
                  {head("track", t("servers.track"), "px-2")}
                  {head("region", t("serverBrowser.location"), "px-2")}
                  {head("ping", t("serverBrowser.ping"), "w-[72px] px-2")}
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
                    <td className="py-2.5 pl-3.5">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          favs.toggle(s.address);
                        }}
                        title={
                          favs.has(s.address) ? t("serverBrowser.unstar") : t("serverBrowser.star")
                        }
                        aria-label={
                          favs.has(s.address) ? t("serverBrowser.unstar") : t("serverBrowser.star")
                        }
                        aria-pressed={favs.has(s.address)}
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
                    <td className="px-2 py-2.5">
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
                      <span
                        className="block max-w-[220px] truncate"
                        title={[s.track, s.trackLayout].filter(Boolean).join(" — ")}
                      >
                        {s.track || "—"}
                        {s.trackLayout && (
                          <span className="text-faint"> · {s.trackLayout}</span>
                        )}
                      </span>
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

/** A column header that sorts on click and flips on a second click. */
const SortHead = ({
  col,
  label,
  sort,
  dir,
  onSort,
  className,
}: {
  col: SortMode;
  label: string;
  sort: SortMode;
  dir: SortDir;
  onSort: (col: SortMode) => void;
  className?: string;
}) => (
  <th
    className={cn("py-2.5 font-semibold", className)}
    aria-sort={sort === col ? (dir === "asc" ? "ascending" : "descending") : undefined}
  >
    <button
      type="button"
      onClick={() => onSort(col)}
      className="inline-flex items-center gap-1 uppercase tracking-wide hover:text-muted-foreground"
    >
      {label}
      {sort === col &&
        (dir === "asc" ? <ChevronUp className="size-3" /> : <ChevronDown className="size-3" />)}
    </button>
  </th>
);

/** The on/off chips in the context bar. */
const ToggleChip = ({
  on,
  onClick,
  title,
  children,
}: {
  on: boolean;
  onClick: () => void;
  title?: string;
  children: React.ReactNode;
}) => (
  <button
    type="button"
    onClick={onClick}
    title={title}
    aria-pressed={on}
    className={cn(
      "flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap border border-input px-2.5 text-[12px]",
      on ? "bg-card text-muted-foreground" : "text-faint hover:text-muted-foreground",
    )}
  >
    {children}
  </button>
);

/** The three non-list states (loading, error, empty) share this centered column. */
const Centered = ({ children }: { children: React.ReactNode }) => (
  <div className="flex h-full flex-col items-center justify-center gap-3 py-16">
    {children}
  </div>
);

export default Servers;
