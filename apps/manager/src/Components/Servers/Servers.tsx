import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  RefreshCw,
  Loader2,
  Plug,
  ServerOff,
  EyeOff,
  Star,
  ChevronUp,
  ChevronDown,
  Globe,
  ServerCog,
  Unplug,
  UserCheck,
  LayoutGrid,
  List,
  SlidersHorizontal,
  MoreHorizontal,
  Clock,
} from "lucide-react";
import { toast } from "sonner";
import { SearchBox } from "@frost/shared/Components/ui/search-box";
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
import { ContextBarLeft, ContextBarRight } from "../Shell/ContextBar";
import { Popover, PopoverContent, PopoverTrigger } from "@frost/shared/Components/ui/popover";
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
} from "@frost/shared/Components/ui/dropdown-menu";
import HelpHint from "@frost/shared/Components/ui/help-hint";
import {
  listMasterServers,
  cachedMasterServers,
  onServersSwept,
  joinServer,
  closeAndJoin,
  queueJoin,
  serversWithPaintSync,
  serverTrackPreviews,
  serverTrackCatalog,
  guessServerTrack,
  resolveQuickInstall,
  resetServerBrowser,
  modTypesFor,
  type CatalogTrack,
  type MasterServer,
} from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { useInstall } from "../../Context/Install";
import { useT, type TFunc, type TKey } from "@/i18n";
import { useFavorites } from "@/lib/useFavorites";
import { useGameRunning } from "@/lib/useGameRunning";
import { isFull, useServerQueue } from "@/lib/useServerQueue";
import { REGION_LABEL_KEY, REGION_ORDER, canonicalRegion, type RegionKey } from "@/lib/serverRegion";
import JoinServerDialog from "../Shell/JoinServerDialog";
import { guessPicture, useTrackGuesses, warmTracks } from "./trackGuesses";
import ServerDetail, { ServerDetailDialog, ServerDetailEmpty } from "./ServerDetail";
import ServerCard from "./ServerCard";
import ServerRow from "./ServerRow";
import ConnectionCheck from "./ConnectionCheck";
import RegisterServerDialog from "./RegisterServerDialog";

type ViewMode = "tiles" | "list";
const VIEW_KEY = "mxb:serversView:v1";

/** Track art by track id, kept for the app's life so coming back to the tab paints at once.
 *  `""` for a track the player has that carries no picture. */
const ART: Record<string, string> = {};
/** Tracks the library has been asked about. One of these missing from ART isn't installed. */
const ASKED = new Set<string>();
/** What our server knows about tracks the player lacks, kept for the app's life. */
const CATALOG: Record<string, CatalogTrack> = {};
/** When each track was last asked of our server. One it didn't know yet is asked again after
 *  `REASK_MS`, since asking is what gets it looked up. */
const CATALOG_ASKED = new Map<string, number>();
const REASK_MS = 5 * 60 * 1000;

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

/**
 * The next list, keeping the previous object wherever nothing about a server changed.
 *
 * The tiles are memoised on the server object, so a refresh that hands every row a fresh
 * object redraws all of them — two hundred tiles, their art and their badges, to show that
 * four rider counts moved. Comparing by value here means a refresh costs the rows that
 * actually changed and nothing else.
 *
 * Rows are matched by address, and a row that is gone from the new list is gone: this returns
 * the new list's shape, only with the old list's objects in it where they are equal.
 */
function mergeRows(prev: MasterServer[] | null, next: MasterServer[]): MasterServer[] {
  if (!prev?.length) return next;
  const held = new Map(prev.map((s) => [s.address, s]));
  return next.map((s) => {
    const was = held.get(s.address);
    return was && same(was, s) ? was : s;
  });
}

/** Two rows, field by field — the shallow compare the tiles themselves do, one level down. */
function same(a: MasterServer, b: MasterServer): boolean {
  const keys = Object.keys(b) as (keyof MasterServer)[];
  return keys.every((k) => {
    const x = a[k];
    const y = b[k];
    if (Array.isArray(x) && Array.isArray(y)) {
      return x.length === y.length && x.every((v, i) => v === y[i]);
    }
    return x === y;
  });
}

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
interface ServersProps {
  /** A server someone shared, as an `mxb://server?addr=…` link. A fresh object each time,
   *  so the same address arriving twice still opens the dialog. */
  link?: { address: string } | null;
}

const Servers = ({ link }: ServersProps) => {
  const t = useT();
  const [servers, setServers] = useState<MasterServer[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  // When what is on screen was true, and whose list it is. Cleared by the first real sweep;
  // until then the tab says how old the thing it drew is rather than implying it is live.
  const [cached, setCached] = useState<{ asOf: number; source: string } | null>(null);
  const [query, setQuery] = useState("");
  const [joining, setJoining] = useState<string | null>(null);
  const queue = useServerQueue();
  const [joinOpen, setJoinOpen] = useState(false);
  // The address a shared link named, held until the dialog has it. Opening the dialog is
  // as far as a link goes — the game is started by the button, not by the URL.
  const [linkAddress, setLinkAddress] = useState<string | undefined>();
  const [registerOpen, setRegisterOpen] = useState(false);
  // Only offered while the game is up: with nothing running there is no half-open session to
  // close, and the button would be a puzzle rather than a fix.
  const { running: gameRunning } = useGameRunning();
  const [unwedging, setUnwedging] = useState(false);

  // A shared link lands here: open the join dialog on the address it named. The server is
  // very unlikely to be in the registry the dialog lists, which is why the address is
  // handed over rather than the dialog left to find it.
  useEffect(() => {
    if (!link) return;
    setLinkAddress(link.address);
    setJoinOpen(true);
  }, [link]);

  /** Close the game's half-open master session so its own Browse screen works again. */
  const unwedgeBrowser = useCallback(() => {
    setUnwedging(true);
    resetServerBrowser()
      .then((outcome) => {
        if (outcome === "signaled") toast.success(t("serverBrowser.unwedgeSent"));
        else if (outcome === "withheld") toast.error(t("serverBrowser.unwedgeNeedsFrostmod"));
        else if (outcome === "not_running") toast.error(t("serverBrowser.unwedgeNoFrostmod"));
        else toast.error(t("serverBrowser.unwedgeFailed"));
      })
      .catch((e: unknown) => toast.error(typeof e === "string" ? e : String(e)))
      .finally(() => setUnwedging(false));
  }, [t]);
  // The server the pane is showing, held by address rather than by object: the list is
  // replaced every sweep, and a pane pinned to the object a row carried when it was clicked
  // would go on showing rider counts from minutes ago.
  const [selected, setSelected] = useState<string | null>(null);
  const pick = useCallback((s: MasterServer) => setSelected(s.address), []);
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

  const [view, setView] = useState<ViewMode>(() => {
    try {
      return localStorage.getItem(VIEW_KEY) === "tiles" ? "tiles" : "list";
    } catch {
      return "tiles";
    }
  });
  useEffect(() => {
    try {
      localStorage.setItem(VIEW_KEY, view);
    } catch {
      // Storage disabled; the choice still holds for this session.
    }
  }, [view]);
  // Changing view drops the selection. In tiles a picked server is an open dialog, and one
  // appearing because somebody pressed the view toggle would be a surprise.
  useEffect(() => {
    setSelected(null);
  }, [view]);

  // One request for every track in the list, not one per tile. Tracks already drawn aren't
  // asked again; the ones the player lacks are, in case they installed one since.
  const [art, setArt] = useState<Record<string, string>>(() => ({ ...ART }));
  // Bumped when a track is installed from a tile, so its own art replaces the catalogue's.
  const [installed, setInstalled] = useState(0);
  // Asked for either view now: the list's rows carry the art small, and the pane beside them
  // shows it as the hero. It was tiles-only while the list was a table of text.
  useEffect(() => {
    if (!servers?.length) return;
    const tracks = [...new Set(servers.map((s) => s.track).filter((tr) => tr && !(tr in ART)))];
    if (tracks.length === 0) return;
    // Never dropped on a re-run: the next run skips whatever is in ART, so art that landed
    // there without reaching the tiles would stay off them until the app restarted.
    serverTrackPreviews(tracks)
      .then((found) => {
        Object.assign(ART, found);
        for (const tr of tracks) ASKED.add(tr);
        setArt({ ...ART });
      })
      .catch(() => {});
  }, [servers, installed]);

  // Identify every track in the list without waiting to be asked. Opening a server to find
  // out what it is running, and to see a picture of it, is work the list can do itself — and
  // with the answers kept on disk between runs, a settled install asks for nothing at all.
  useEffect(() => {
    if (!servers?.length) return;
    let live = true;
    void warmTracks(
      servers.map((s) => s.track),
      guessServerTrack,
      () => live,
    );
    return () => {
      live = false;
    };
  }, [servers]);

  // The tracks the player lacks, from our server: what they are, their picture, the price.
  const [catalog, setCatalog] = useState<Record<string, CatalogTrack>>(() => ({ ...CATALOG }));
  useEffect(() => {
    if (!servers?.length) return;
    const now = Date.now();
    const tracks = [...new Set(servers.map((s) => s.track))].filter(
      (tr) =>
        tr &&
        ASKED.has(tr) &&
        // `!(tr in art)`, not `!art[tr]`: an empty string means "installed, carries no
        // picture", and asking the store about a track the player already has buys a wrong
        // answer — the name is all it can match on, and a stock track called "forest" came
        // back as somebody else's product. A grey tile is better than the wrong track.
        !(tr in art) &&
        !(tr in CATALOG) &&
        now - (CATALOG_ASKED.get(tr) ?? 0) > REASK_MS,
    );
    if (tracks.length === 0) return;
    for (const tr of tracks) CATALOG_ASKED.set(tr, now);
    serverTrackCatalog(tracks)
      .then((found) => {
        Object.assign(CATALOG, found);
        setCatalog({ ...CATALOG });
      })
      .catch(() => {});
  }, [servers, art]);

  // One fetch at a time. Two overlapping ones each sign in to Steam, and the loser's
  // failure used to replace the winner's list with an error.
  const inFlight = useRef(false);
  // What the tab is showing, for the failure path — `load` holds no state of its own.
  const onScreen = useRef<MasterServer[] | null>(null);
  const load = useCallback(() => {
    if (inFlight.current) return;
    inFlight.current = true;
    setLoading(true);
    // The error is cleared when the next fetch *succeeds*, not when it starts. Clearing it here
    // dropped the failure screen for as long as the retry took — which meant the connection
    // check under it was unmounted, and its results thrown away, every time somebody pressed
    // Try again. The spinner on the button already says a retry is happening.
    listMasterServers()
      .then(({ servers: list, asOf, source }) => {
        // Merged, not replaced: the tiles are memoised, and handing every row a new object
        // would redraw the whole grid to show that four rider counts moved.
        const merged = mergeRows(onScreen.current, list);
        onScreen.current = merged;
        setServers(merged);
        // A list that isn't ours keeps its age on screen. On a machine with no MX Bikes there
        // is never one of our own, so this is what the tab shows from then on.
        setCached(source ? { asOf, source } : null);
        setError(null);
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

  // The app sweeps on its own beat whether or not anybody is on this tab, so a tab left open
  // follows those instead of polling. Merged the same way a refresh is, so a beat that moves
  // four rider counts redraws four tiles rather than the grid.
  //
  // A sweep landing is also the answer to whatever error is on screen: it only arrives when
  // one succeeded.
  useEffect(() => {
    const stop = onServersSwept(({ servers: list, asOf, source }) => {
      const merged = mergeRows(onScreen.current, list);
      onScreen.current = merged;
      setServers(merged);
      setCached(source ? { asOf, source } : null);
      setError(null);
    });
    return () => {
      stop.then((off) => off()).catch(() => {});
    };
  }, []);

  // Something to look at while that runs. The sweep behind `load` is a Steam sign-in, a master
  // login and a datagram to every server that answers, and for those seconds the tab used to
  // be a spinner — every time, for a list that is mostly the same as the last one.
  //
  // Only ever fills an empty screen: a sweep that has already landed is the better answer and
  // must not be replaced by a remembered one that arrives a moment later.
  useEffect(() => {
    let dropped = false;
    cachedMasterServers()
      .then(({ servers: list, asOf, source }) => {
        if (dropped || !list.length || !asOf) return;
        setServers((cur) => {
          if (cur !== null) return cur;
          onScreen.current = list;
          setCached({ asOf, source });
          return list;
        });
      })
      .catch(() => {});
    return () => {
      dropped = true;
    };
  }, []);

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
      // A star is a standing instruction about where a server belongs, so it outranks the
      // column: starring one used to change nothing at all about the order.
      const star = Number(favs.has(b.address)) - Number(favs.has(a.address));
      if (star !== 0) return star;
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

  /** Everything the sweep found, minus what the app itself filtered out — the number the
   *  count compares against. */
  const reachable = (servers?.length ?? 0) - (showHidden ? 0 : hiddenCount);

  /** The server the pane is showing, looked up in the current list every render so it ticks
   *  along with the sweeps instead of freezing at the moment the row was clicked. Found in
   *  the whole list rather than the filtered one: narrowing the search shouldn't empty the
   *  pane on whatever is being read in it. */
  const detail = useMemo(
    () => (servers ?? []).find((s) => s.address === selected) ?? null,
    [servers, selected],
  );

  /** How many filters are narrowing the list, for the trigger that now holds them. */
  const filterCount = useMemo(
    () =>
      (favesOnly ? 1 : 0) +
      (hideEmpty ? 1 : 0) +
      (!favesOnly && region !== "all" ? 1 : 0) +
      (showHidden ? 1 : 0),
    [favesOnly, hideEmpty, region, showHidden],
  );

  const { game } = useConfig();

  /** Close the open game and join with the copy that replaces it. */
  const closeThenJoin = useCallback(
    async (address: string) => {
      setJoining(address);
      try {
        await closeAndJoin(address);
        toast.success(t("join.launching", { address }));
      } catch (e) {
        toast.error(typeof e === "string" ? e : t("serverBrowser.joinFailed"));
      } finally {
        setJoining(null);
      }
    },
    [t],
  );

  const join = useCallback(
    async (address: string) => {
      if (joining) return;
      setJoining(address);
      try {
        const outcome = await joinServer(address);
        if (outcome === "already_running") {
          // The game reads the connect flag only at startup, so an open copy can't be sent
          // anywhere — which used to be the end of it. The way through is to replace the
          // process, and that is worth offering rather than leaving as a fact to act on.
          toast.info(t("join.alreadyRunning", { game: game.display }), {
            duration: 12_000,
            action: {
              label: t("join.closeAndJoin"),
              onClick: () => void closeThenJoin(address),
            },
          });
        } else {
          toast.success(t("join.launching", { address }));
        }
      } catch (e) {
        toast.error(typeof e === "string" ? e : t("serverBrowser.joinFailed"));
      } finally {
        setJoining(null);
      }
    },
    [joining, t, game.display, closeThenJoin],
  );

  // A full server turns you away, so the button gets you in line instead of failing.
  const wait = useCallback(
    async (s: MasterServer) => {
      try {
        await queueJoin(s.address, s.name);
      } catch (e) {
        toast.error(typeof e === "string" ? e : t("queue.joinFailed"));
      }
    },
    [t],
  );

  // Install & join: a free track goes through the install queue, and the server is joined
  // once it lands. Keyed by the mod's slug, since that is all the queue reports by.
  const { startPendingInstall, active } = useInstall();
  const [installing, setInstalling] = useState<Record<string, string>>({});
  // Slugs whose install has been seen running. A finished card left over from an earlier
  // install of the same track must not join the server before this one has even started.
  const started = useRef(new Set<string>());
  const doneInstalling = useCallback((slug: string) => {
    started.current.delete(slug);
    setInstalling((cur) => {
      const rest = { ...cur };
      delete rest[slug];
      return rest;
    });
  }, []);

  const installAndJoin = useCallback(
    (s: MasterServer, product: CatalogTrack) => {
      const slug = product.slug;
      const tracks = modTypesFor(game.id).find((m) => m.id === "tracks");
      if (!slug || !tracks) return;
      setInstalling((cur) => ({ ...cur, [slug]: s.address }));
      startPendingInstall({
        slug,
        title: product.name,
        subpath: tracks.installSubpath,
        resolve: async () => {
          try {
            const res = await resolveQuickInstall(slug, tracks, game, tracks.categoryId);
            if (res.ok) return { ...res.params, categoryId: tracks.categoryId };
            if (res.reason === "blocked") {
              toast.error(t("browse.needsBrowser", { title: res.title }), {
                description: t("browse.needsBrowserDesc", { host: res.host ?? "" }),
              });
            } else if (res.reason === "serverOnly") {
              toast.error(t("browse.serverOnly", { title: res.title }), {
                description: t("browse.serverOnlyDesc"),
              });
            } else {
              toast.error(t("browse.noDownload", { title: res.title }));
            }
          } catch (e) {
            toast.error(t("serverBrowser.installFailed", { title: product.name }), {
              description: String(e),
            });
          }
          doneInstalling(slug);
          return null;
        },
      });
    },
    [game, startPendingInstall, doneInstalling, t],
  );

  useEffect(() => {
    for (const [slug, address] of Object.entries(installing)) {
      const job = active.find((a) => a.slug === slug);
      if (!job) continue;
      const finished = job.stage === "done" || job.stage === "error" || job.stage === "review";
      if (!finished) {
        started.current.add(slug);
        continue;
      }
      if (!started.current.has(slug)) continue;
      doneInstalling(slug);
      // A pack goes to review and an error has its own card; neither is ready to ride.
      if (job.stage !== "done") continue;
      const s = servers?.find((x) => x.address === address);
      if (s?.track) {
        delete ART[s.track];
        ASKED.delete(s.track);
        setInstalled((n) => n + 1);
      }
      if (s && isFull(s)) void wait(s);
      else void join(address);
    }
  }, [active, installing, servers, join, wait, doneInstalling]);

  const installingAt = useMemo(() => new Set(Object.values(installing)), [installing]);

  const copy = useCallback(
    (address: string) => {
      navigator.clipboard
        .writeText(address)
        .then(() => toast.success(t("serverBrowser.copied")))
        .catch(() => {});
    },
    [t],
  );

  /** Everything the pane needs about whichever server is picked. The join decision is the
   *  tile's — Join, Install & join, Buy, Wait in line — so both are handed the same inputs
   *  and land on the same button rather than each working it out their own way. */
  // A picture the detail pane learned. The list had only the installed preview and our own
  // catalogue, so a track that is neither — Fort Red, found on mxb-mods — drew a full hero
  // and an empty row beside it. Reading the same store fixes that the moment it is known.
  useTrackGuesses();
  const pictureFor = (track: string) => art[track] || guessPicture(track) || undefined;

  const detailProps = {
    server: detail,
    art: detail ? pictureFor(detail.track) : undefined,
    missing: !!detail?.track && ASKED.has(detail.track) && !(detail.track in art),
    product: detail ? catalog[detail.track] : undefined,
    installing: !!detail && installingAt.has(detail.address),
    favourite: !!detail && favs.has(detail.address),
    joining,
    busy: joining !== null,
    queue,
    onJoin: join,
    onWait: wait,
    onInstallJoin: installAndJoin,
    onCopy: copy,
    onToggleFavourite: favs.toggle,
  };

  const chip = (col: SortMode, label: string) => (
    <SortChip col={col} label={label} sort={sort} dir={dir} onSort={sortBy} />
  );

  return (
    <div className="flex h-full flex-col">
      {/* Filters on the left, actions on the right — the split the bar was built for.
          All of it used to sit on the right, and since most of it only appears once a list
          has loaded, the row ran out of width at the moment the tab finished loading: every
          button compressed at once and the last one, Add your server, worst of all. */}
      <ContextBarLeft>
        <div className="flex items-center gap-2 self-center">
          <Popover>
            <PopoverTrigger asChild>
              <button
                type="button"
                title={t("serverBrowser.filters")}
                className={cn(
                  "flex h-7 shrink-0 items-center gap-1.5 whitespace-nowrap border border-input px-2.5 text-[12px]",
                  filterCount > 0
                    ? "bg-card text-muted-foreground"
                    : "text-faint hover:text-muted-foreground",
                )}
              >
                <SlidersHorizontal className="size-3.5" />
                {t("serverBrowser.filters")}
                {filterCount > 0 && (
                  <span className="tabular-nums text-primary">{filterCount}</span>
                )}
              </button>
            </PopoverTrigger>
            {/* One trigger rather than five chips: five fit a wide window in English and
                nothing else, and what gave way was always the buttons beside them. */}
            <PopoverContent align="start" className="w-[236px] p-2">
              <div className="flex flex-col gap-1.5">
                {favs.count > 0 && (
                  <ToggleChip
                    on={favesOnly}
                    onClick={() => setFavesOnly((v) => !v)}
                    className="w-full justify-start"
                  >
                    <Star className={cn("size-3.5", favesOnly && "fill-current")} />
                    {t("serverBrowser.favesOnly")}
                  </ToggleChip>
                )}
                <ToggleChip
                  on={hideEmpty}
                  onClick={() => setHideEmpty((v) => !v)}
                  title={t("serverBrowser.hideEmptyHelp")}
                  className="w-full justify-start"
                >
                  <UserCheck className="size-3.5" />
                  {t("serverBrowser.hideEmpty")}
                </ToggleChip>
                {!favesOnly && regions.length > 1 && (
                  <Select value={region} onValueChange={(v) => setRegion(v as RegionKey | "all")}>
                    <SelectTrigger className="h-7 w-full bg-card text-[12px]">
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
                    className="w-full justify-start"
                  >
                    <EyeOff className="size-3.5" />
                    {showHidden
                      ? t("serverBrowser.hideFiltered")
                      : t("serverBrowser.hiddenCount", { count: hiddenCount })}
                  </ToggleChip>
                )}
              </div>
            </PopoverContent>
          </Popover>
          {/* The number has to describe what is on screen. It used to count the whole list
              while the filters — Has riders is on by default — were hiding most of it, so the
              bar said 64 next to thirteen rows. When a filter is narrowing things it says
              both, and the total is the one that needs explaining, not the rows you can see. */}
          {servers && servers.length > 0 && (
            <span className="shrink-0 tabular-figures text-[12.5px] text-faint">
              {shown.length === reachable
                ? t("serverBrowser.count", { count: shown.length })
                : t("serverBrowser.countOf", { count: shown.length, total: reachable })}
            </span>
          )}
          {/* What is on screen is a remembered list until the sweep lands, and it says so. The
              rider counts are the first thing anybody reads off this tab, and a stale one shown
              without its age is worse than no list at all. */}
          {cached && (
            <span className="flex shrink-0 items-center gap-1.5 whitespace-nowrap text-[12.5px] text-faint">
              <Clock className="size-3.5" />
              {cached.source === "shared"
                ? t("serverBrowser.cachedShared", { age: ago(t, cached.asOf) })
                : t("serverBrowser.cachedLocal", { age: ago(t, cached.asOf) })}
            </span>
          )}
        </div>
      </ContextBarLeft>

      <ContextBarRight>
        <Segmented<ViewMode>
          size="sm"
          value={view}
          onChange={setView}
          options={[
            {
              value: "list",
              label: <List className="size-3.5" aria-label={t("serverBrowser.viewList")} />,
            },
            {
              value: "tiles",
              label: (
                <LayoutGrid className="size-3.5" aria-label={t("serverBrowser.viewTiles")} />
              ),
            },
          ]}
        />
        {/* The one control here that may shrink. Everything else keeps its width, so a
            narrow window trims the search box rather than wrapping four button labels. */}
        <SearchBox
          value={query}
          onChange={setQuery}
          placeholder={t("serverBrowser.searchPlaceholder")}
          className="w-[200px] shrink"
        />
        <Button
          variant="outline"
          size="sm"
          className="shrink-0 px-2.5"
          onClick={load}
          disabled={loading}
          title={t("serverBrowser.refresh")}
          aria-label={t("serverBrowser.refresh")}
        >
          <RefreshCw className={cn("size-3.5", loading && "animate-spin")} />
        </Button>
        {/* Join by address, for a server the master list doesn't carry. It lived in the
            sidebar next to Play; with the sidebar gone this is where someone looks for
            it — the page that is already about joining servers. */}
        <Button
          variant="outline"
          size="sm"
          className="shrink-0"
          onClick={() => setJoinOpen(true)}
        >
          <Plug className="size-3.5" />
          {t("join.title")}
        </Button>
        {/* The two nobody reaches for twice in a day. Registering a server is for the one
            the master can't show, and the browser reset is for a game that has wedged its
            own list — both worth having, neither worth a permanent slot in the row. */}
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="sm"
              className="shrink-0 px-2"
              title={t("serverBrowser.moreActions")}
              aria-label={t("serverBrowser.moreActions")}
            >
              <MoreHorizontal className="size-4" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onSelect={() => setRegisterOpen(true)}>
              <ServerCog className="size-4" />
              {t("registerServer.action")}
            </DropdownMenuItem>
            {/* For the game's own Browse screen saying "connection timeout" until you
                restart it. FrostMod clears that by itself when it recognises the state, so
                this is only here for the times it holds off — and only while there is a
                game running to fix. */}
            {gameRunning && (
              <DropdownMenuItem onSelect={unwedgeBrowser} disabled={unwedging}>
                {unwedging ? (
                  <Loader2 className="size-4 animate-spin" />
                ) : (
                  <Unplug className="size-4" />
                )}
                {t("serverBrowser.unwedge")}
              </DropdownMenuItem>
            )}
          </DropdownMenuContent>
        </DropdownMenu>
        <HelpHint title={t("servers.title")} description={t("serverBrowser.help")} />
      </ContextBarRight>

      <JoinServerDialog
        open={joinOpen}
        onOpenChange={(open) => {
          setJoinOpen(open);
          // Cleared on the way out, so opening the dialog by hand later starts on the
          // address the player last typed rather than on someone else's link.
          if (!open) setLinkAddress(undefined);
        }}
        initialAddress={linkAddress}
        onJoined={load}
      />
      <RegisterServerDialog open={registerOpen} onOpenChange={setRegisterOpen} />
      {/* A tile has no list beside it to put the pane next to, so from the grid it still
          opens over the top — the same pane, in a dialog. */}
      {view === "tiles" && (
        <ServerDetailDialog
          {...detailProps}
          onOpenChange={(open) => !open && setSelected(null)}
        />
      )}

      <div
        className={cn(
          "min-h-0 flex-1 px-7 pb-6",
          // The list view scrolls its two columns separately; everything else scrolls whole.
          view === "list" && shown.length > 0 ? "flex gap-4" : "overflow-y-auto",
        )}
      >
        {servers === null ? (
          <Centered>
            <Loader2 className="size-5 animate-spin text-faint" />
            <p className="text-[13px] text-faint">{t("serverBrowser.loading")}</p>
          </Centered>
        ) : error && servers.length === 0 ? (
          // Not the bare error this used to be. A failed list is nearly always the master
          // server having gone quiet, which looks identical from one machine to a firewall
          // problem — so the app asks how many other people are failing the same fetch and
          // leads with that, rather than leaving somebody to debug a machine that is fine.
          <Centered>
            <ConnectionCheck error={error} onRetry={load} />
          </Centered>
        ) : shown.length === 0 ? (
          <Centered>
            <ServerOff className="size-6 text-faint" />
            <p className="text-[13px] text-faint">
              {favesOnly ? t("serverBrowser.favesEmpty") : t("serverBrowser.empty")}
            </p>
          </Centered>
        ) : view === "tiles" ? (
          <div className="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-3">
            {shown.map((s, i) => (
              <ServerCard
                key={`${s.address}-${i}`}
                server={s}
                art={pictureFor(s.track)}
                missing={!!s.track && ASKED.has(s.track) && !(s.track in art)}
                product={catalog[s.track]}
                installing={installingAt.has(s.address)}
                onInstallJoin={installAndJoin}
                favourite={favs.has(s.address)}
                paintSync={paintSync[s.address] ?? 0}
                joining={joining === s.address}
                busy={joining !== null}
                queuePosition={queue?.address === s.address ? queue.position : null}
                onOpen={pick}
                onJoin={join}
                onWait={wait}
                onCopy={copy}
                onToggleFavourite={favs.toggle}
              />
            ))}
          </div>
        ) : (
          // The list is the master, the pane is the detail. Both are on screen at once, so
          // reading the second server no longer means closing the first.
          <>
            <div className="flex w-[34%] min-w-[340px] max-w-[560px] shrink-0 flex-col overflow-hidden rounded-xl border border-input">
              {/* The table's sortable headers, kept as a strip. Seven columns don't fit
                  400px; the sorting they carried is still what orders the list. */}
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-input bg-card px-3 py-2">
                {chip("name", t("serverBrowser.name"))}
                {chip("players", t("serverBrowser.players"))}
                {chip("track", t("servers.track"))}
                {chip("region", t("serverBrowser.location"))}
                {chip("ping", t("serverBrowser.ping"))}
              </div>
              <div className="min-h-0 flex-1 overflow-y-auto">
                {shown.map((s, i) => (
                  <ServerRow
                    key={`${s.address}-${i}`}
                    server={s}
                    art={pictureFor(s.track)}
                    missing={!!s.track && ASKED.has(s.track) && !(s.track in art)}
                    product={catalog[s.track]}
                    selected={s.address === selected}
                    favourite={favs.has(s.address)}
                    paintSync={paintSync[s.address] ?? 0}
                    queuePosition={queue?.address === s.address ? queue.position : null}
                    onSelect={pick}
                    onToggleFavourite={favs.toggle}
                  />
                ))}
              </div>
            </div>
            <div className="min-h-0 min-w-0 flex-1 overflow-hidden rounded-xl border border-input">
              {detail ? (
                // Keyed on the address so picking another row starts the pane clean rather
                // than showing the last server's riders until the new probe lands.
                <ServerDetail key={detail.address} {...detailProps} className="h-full" />
              ) : (
                <ServerDetailEmpty />
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
};

/** One way of ordering the list, set on click and flipped on a second click. The column
 *  headers it replaces sorted the same five things. */
const SortChip = ({
  col,
  label,
  sort,
  dir,
  onSort,
}: {
  col: SortMode;
  label: string;
  sort: SortMode;
  dir: SortDir;
  onSort: (col: SortMode) => void;
}) => (
  <button
    type="button"
    onClick={() => onSort(col)}
    aria-pressed={sort === col}
    className={cn(
      "inline-flex cursor-default items-center gap-0.5 font-cond text-[10.5px] font-bold uppercase tracking-[0.14em] transition-colors",
      sort === col ? "text-primary" : "text-faint hover:text-muted-foreground",
    )}
  >
    {label}
    {sort === col &&
      (dir === "asc" ? <ChevronUp className="size-3" /> : <ChevronDown className="size-3" />)}
  </button>
);

/** `1723459200000` -> `2 minutes ago`. The paint-sync wording, which already exists in every
 *  language the app speaks. */
function ago(t: TFunc<TKey>, at: number): string {
  const secs = Math.max(0, Math.round((Date.now() - at) / 1000));
  if (secs < 60) return t("sync.agoJustNow");
  const mins = Math.round(secs / 60);
  if (mins < 60) return t("sync.agoMinutes", { count: mins });
  const hours = Math.round(mins / 60);
  if (hours < 24) return t("sync.agoHours", { count: hours });
  return t("sync.agoDays", { count: Math.round(hours / 24) });
}

/** The on/off chips: in the filter popover now, still shaped like the bar they came from. */
const ToggleChip = ({
  on,
  onClick,
  title,
  className,
  children,
}: {
  on: boolean;
  onClick: () => void;
  title?: string;
  className?: string;
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
      className,
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
