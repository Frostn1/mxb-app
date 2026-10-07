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
  Bookmark,
} from "lucide-react";
import { toast } from "sonner";
import { invoke } from "@tauri-apps/api/core";
import { open as pickFile } from "@tauri-apps/plugin-dialog";
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
  joinListedServer,
  closeAndJoinListedServer,
  queueJoin,
  serversWithPaintSync,
  serverTrackPreviews,
  serverTrackInactive,
  serverTrackCatalog,
  guessServerTrack,
  modsStateSet,
  resolveQuickInstall,
  destStorageKey,
  resetServerBrowser,
  modTypesFor,
  probeSavedServers,
  type CatalogTrack,
  type MasterServer,
  type SavedServer,
} from "@frost/shared/api/mods";
import { useConfig } from "@frost/shared/Context/Config";
import { useInstall } from "../../Context/Install";
import { useT, type TFunc, type TKey } from "@/i18n";
import { useFavorites } from "@/lib/useFavorites";
import { useSavedServers } from "@/lib/useSavedServers";
import { useGameRunning } from "@/lib/useGameRunning";
import { isFull, useServerQueue } from "@/lib/useServerQueue";
import { BoundedCache } from "@/lib/boundedCache";
import { REGION_LABEL_KEY, REGION_ORDER, canonicalRegion, type RegionKey } from "@/lib/serverRegion";
import { serverMatchesQuery } from "@/lib/serverClasses";
import JoinServerDialog from "../Shell/JoinServerDialog";
import { LoadingMark } from "../Shell/LoadingMark";
import { guessPicture, rememberGuess, useTrackGuesses, warmTracks } from "./trackGuesses";
import ServerDetail, { ServerDetailDialog, ServerDetailEmpty } from "./ServerDetail";
import ServerCard from "./ServerCard";
import ServerRow from "./ServerRow";
import ConnectionCheck from "./ConnectionCheck";
import RegisterServerDialog from "./RegisterServerDialog";
import SavedServers, { type SavedRow } from "./SavedServers";
import FriendsPanel from "./FriendsPanel";
import { joinAction } from "./joinAction";
import {
  RESOLVE_TIMEOUT_MS,
  startWatch,
  stepWatch,
  withTimeout,
  type InstallWatch,
} from "./installWatch";
import { useFriends } from "@/lib/useFriends";
import { friendsByAddress } from "@/lib/friends";
import SavedServerDialog, { savedServerError } from "./SavedServerDialog";

/** A saved server that didn't answer: its address, the player's name for it, and Join left on.
 *  Everything a reply would have filled in is empty rather than guessed. */
function offlineRow(address: string, name: string): MasterServer {
  return {
    name,
    address,
    joinable: true,
    lanAddress: "",
    players: 0,
    maxPlayers: 0,
    pingMs: null,
    passworded: false,
    location: "",
    bots: 0,
    rating: "",
    track: "",
    trackLayout: "",
    categories: [],
    bikes: [],
    session: "",
    raceLength: "",
    conditions: "",
    realisticWeather: false,
    forceCockpit: false,
    noAids: false,
    limitedTyreSets: false,
    hidden: "",
  };
}

/** How often saved servers off the master list are asked again when no sweep has landed.
 *  The app's own sweep beat, so they tick at the same pace as everything else on the tab. */
const SAVED_PROBE_MS = 2 * 60 * 1000;

/** The key saved servers are matched on: hostnames ignore case, as the Rust side does. */
const addrKey = (address: string) => address.toLowerCase();

type ViewMode = "tiles" | "list";
const VIEW_KEY = "mxb:serversView:v1";

/** Installed artwork (`""` means no picture) or `null` for a confirmed missing track. */
const LIBRARY = new BoundedCache<string | null>(256, 32 * 1024 * 1024);
const LIBRARY_PENDING = new Set<string>();
/** What our server knows about tracks the player lacks, bounded with its retained images. */
const CATALOG = new BoundedCache<CatalogTrack>(256, 32 * 1024 * 1024);
/** When each track was last asked of our server. One it didn't know yet is asked again after
 *  `REASK_MS`, since asking is what gets it looked up. */
const CATALOG_ASKED = new BoundedCache<number>(512, 512 * 8);
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
  const friendsFeed = useFriends();
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

  // The player's saved servers. One the sweep carries takes its row from there; one it doesn't
  // is asked directly, and `asked` is every address the last of those probes covered — so an
  // address in `asked` but not in `probed` is a server that didn't answer.
  const saved = useSavedServers();
  const [probed, setProbed] = useState<Record<string, MasterServer>>({});
  const [asked, setAsked] = useState<Set<string>>(() => new Set());
  const [savedDialog, setSavedDialog] = useState<{ editing: SavedServer | null } | null>(null);
  const listed = useMemo(
    () => new Map((servers ?? []).map((s) => [addrKey(s.address), s])),
    [servers],
  );
  const unlistedKey = useMemo(
    () =>
      saved.list
        .map((s) => s.address)
        .filter((a) => !listed.has(addrKey(a)))
        .join("\n"),
    [saved.list, listed],
  );
  // Asked again whenever a sweep lands, so a saved server's numbers move at the list's pace
  // rather than freezing at whatever it said when the tab opened — and on a beat of its own
  // too, because a master that has stopped answering stops the sweeps, and a private server
  // is exactly the one worth watching while it does.
  const [probeBeat, setProbeBeat] = useState(0);
  useEffect(() => {
    if (!unlistedKey) return;
    const id = window.setInterval(() => setProbeBeat((n) => n + 1), SAVED_PROBE_MS);
    return () => window.clearInterval(id);
  }, [unlistedKey]);
  useEffect(() => {
    if (servers === null || !unlistedKey) return;
    const addresses = unlistedKey.split("\n");
    let live = true;
    probeSavedServers(addresses)
      .then((rows) => {
        if (!live) return;
        setProbed(Object.fromEntries(rows.map((r) => [addrKey(r.address), r])));
        setAsked(new Set(addresses.map(addrKey)));
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [servers, unlistedKey, probeBeat]);

  /** The sweep plus the saved servers only a direct probe found — what the track art and
   *  identification run over, so a saved card gets the same picture a listed one would. */
  const known = useMemo(() => {
    if (servers === null) return null;
    const extra = Object.values(probed).filter((s) => !listed.has(addrKey(s.address)));
    return extra.length ? [...servers, ...extra] : servers;
  }, [servers, probed, listed]);

  const savedRows = useMemo<SavedRow[]>(
    () =>
      saved.list.map((entry) => {
        const key = addrKey(entry.address);
        const live = listed.get(key) ?? probed[key];
        // An unnamed server that didn't answer is known only by its address, so that is its name.
        const row = live ?? offlineRow(entry.address, entry.name || entry.address);
        return {
          saved: entry,
          row,
          server: live && entry.name ? { ...live, name: entry.name } : row,
          status: live ? "online" : asked.has(key) ? "offline" : "checking",
        };
      }),
    [saved.list, listed, probed, asked],
  );

  const removeSaved = useCallback(
    (entry: SavedServer) => {
      const before = saved.list.map((s) => s.address);
      saved
        .remove(entry.address)
        .then(() =>
          toast.success(t("savedServers.removed", { name: entry.name || entry.address }), {
            action: {
              label: t("savedServers.undo"),
              // Back where it was, not at the end.
              onClick: () =>
                void saved
                  .add(entry.address, entry.name)
                  .then(() => saved.reorder(before))
                  .catch((e: unknown) => toast.error(savedServerError(t, e))),
            },
          }),
        )
        .catch((e: unknown) => toast.error(savedServerError(t, e)));
    },
    [saved, t],
  );

  const moveSaved = useCallback(
    (entry: SavedServer, by: number) => {
      saved.move(entry.address, by).catch((e: unknown) => toast.error(savedServerError(t, e)));
    },
    [saved, t],
  );

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
  const [library, setLibrary] = useState<Record<string, string | null>>(() =>
    Object.fromEntries(LIBRARY.entries()),
  );
  // Missing tracks the player does have, parked by Manage: id to the `rel` that turns it on.
  const [inactive, setInactive] = useState<Record<string, string>>({});
  // Bumped when a track is installed from a tile, so its own art replaces the catalogue's.
  const [installed, setInstalled] = useState(0);
  // Asked for either view now: the list's rows carry the art small, and the pane beside them
  // shows it as the hero. It was tiles-only while the list was a table of text.
  useEffect(() => {
    if (!known?.length) return;
    // A "not installed" is asked again on every sweep, as the comment above always promised:
    // it used to stick for the life of the app, so a track copied in while the tab was open
    // was offered as a download until restart. The backend snapshot is keyed on every folder
    // in the track tree, so re-asking is a directory walk, not a rescan of the archives.
    const tracks = [
      ...new Set(
        known
          .map((s) => s.track)
          .filter(
            (tr) =>
              tr && !LIBRARY_PENDING.has(tr) && (!LIBRARY.has(tr) || LIBRARY.get(tr) === null),
          ),
      ),
    ];
    if (tracks.length === 0) return;
    // Claim before invoking: a second sweep can land while the first decode is still running.
    for (const tr of tracks) LIBRARY_PENDING.add(tr);
    serverTrackPreviews(tracks)
      .then(async (found) => {
        const absent: string[] = [];
        for (const tr of tracks) {
          const value = Object.prototype.hasOwnProperty.call(found, tr) ? found[tr] : null;
          LIBRARY.set(tr, tr, value, value === null ? 1 : 2 * value.length);
          if (value === null) absent.push(tr);
        }
        setLibrary(Object.fromEntries(LIBRARY.entries()));
        // Of the ones the game can't see, which the player has parked in Manage: those are
        // switched on, never downloaded again.
        const parked = absent.length ? await serverTrackInactive(absent).catch(() => null) : {};
        if (!parked) return;
        setInactive((cur) => {
          const next = { ...cur };
          for (const tr of tracks) delete next[tr];
          return { ...next, ...parked };
        });
      })
      .catch(() => {
        // A failed batch learned nothing and is safe to retry on the next sweep.
      })
      .finally(() => {
        for (const tr of tracks) LIBRARY_PENDING.delete(tr);
      });
  }, [known, installed]);

  // Identify every track in the list without waiting to be asked. Opening a server to find
  // out what it is running, and to see a picture of it, is work the list can do itself — and
  // with the answers kept on disk between runs, a settled install asks for nothing at all.
  useEffect(() => {
    if (!known?.length) return;
    let live = true;
    void warmTracks(
      known.map((s) => ({ id: s.track, hint: s.name })),
      guessServerTrack,
      () => live,
    );
    return () => {
      live = false;
    };
  }, [known]);

  // The tracks the player lacks, from our server: what they are, their picture, the price.
  const [catalog, setCatalog] = useState<Record<string, CatalogTrack>>(() =>
    Object.fromEntries(CATALOG.entries()),
  );
  useEffect(() => {
    if (!known?.length) return;
    const now = Date.now();
    const tracks = [...new Set(known.map((s) => s.track))].filter(
      (tr) =>
        tr &&
        library[tr] === null &&
        !CATALOG.has(tr) &&
        now - (CATALOG_ASKED.get(tr) ?? 0) > REASK_MS,
    );
    if (tracks.length === 0) return;
    for (const tr of tracks) CATALOG_ASKED.set(tr, tr, now, 8);
    serverTrackCatalog(tracks)
      .then((found) => {
        for (const [tr, product] of Object.entries(found)) {
          CATALOG.set(tr, tr, product, 2 * JSON.stringify(product).length);
        }
        setCatalog(Object.fromEntries(CATALOG.entries()));
      })
      .catch(() => {});
  }, [known, library]);

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
      // Any one of a server's classes counts, so "mx2" finds a server that lists MX2 last.
      list = list.filter((s) => serverMatchesQuery(s, q));
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
    () =>
      (servers ?? []).find((s) => s.address === selected) ??
      // A saved server the master doesn't list opens on its direct answer, or its stand-in.
      savedRows.find((r) => r.row.address === selected)?.row ??
      null,
    [servers, selected, savedRows],
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
  const joinError = useCallback(
    (e: unknown) => {
      if (e === "server_bike_no_profile") return t("serverBrowser.bikeNoProfile");
      if (e === "server_bike_no_match") return t("serverBrowser.bikeNoMatch");
      return typeof e === "string" ? e : t("serverBrowser.joinFailed");
    },
    [t],
  );

  const closeThenJoin = useCallback(
    async (server: MasterServer) => {
      setJoining(server.address);
      try {
        await closeAndJoinListedServer(server.address, server.categories, server.bikes);
        toast.success(t("join.launching", { address: server.address }));
      } catch (e) {
        toast.error(joinError(e));
      } finally {
        setJoining(null);
      }
    },
    [t, joinError],
  );

  const join = useCallback(
    async (server: MasterServer) => {
      if (joining) return;
      setJoining(server.address);
      try {
        const outcome = await joinListedServer(server.address, server.categories, server.bikes);
        if (outcome === "already_running") {
          // The game reads the connect flag only at startup, so an open copy can't be sent
          // anywhere — which used to be the end of it. The way through is to replace the
          // process, and that is worth offering rather than leaving as a fact to act on.
          toast.info(t("join.alreadyRunning", { game: game.display }), {
            duration: 12_000,
            action: {
              label: t("join.closeAndJoin"),
              onClick: () => void closeThenJoin(server),
            },
          });
        } else {
          toast.success(t("join.launching", { address: server.address }));
        }
      } catch (e) {
        toast.error(joinError(e));
      } finally {
        setJoining(null);
      }
    },
    [joining, t, game.display, closeThenJoin, joinError],
  );

  // A full server turns you away, so the button gets you in line instead of failing.
  const wait = useCallback(
    async (s: MasterServer) => {
      try {
        await queueJoin(s.address, s.name, s.categories, s.bikes);
      } catch (e) {
        toast.error(typeof e === "string" ? e : t("queue.joinFailed"));
      }
    },
    [t],
  );

  // A free track goes through the install queue. It only joins the server afterward when
  // the player chose the explicit Install & join action.
  const { startPendingInstall, startImport, active, queued, cancel } = useInstall();
  const [installing, setInstalling] = useState<
    Record<string, { server: MasterServer; joinAfter: boolean }>
  >({});
  // Each install's watch: whether it has been seen running (a finished card left over from an
  // earlier install of the same track must not join the server before this one has even
  // started), and when it last moved, so a stalled or vanished job can't hold the tile.
  const watches = useRef(new Map<string, InstallWatch>());
  const doneInstalling = useCallback((slug: string) => {
    watches.current.delete(slug);
    setInstalling((cur) => {
      const rest = { ...cur };
      delete rest[slug];
      return rest;
    });
  }, []);
  // Tracks whose install failed, stalled or found nothing to download, with whether the
  // player had asked to join afterwards. Their servers offer Join anyway / Retry / Pick
  // instead of the same Install that just failed.
  const [failedTracks, setFailedTracks] = useState<Record<string, boolean>>({});
  const markFailed = useCallback((track: string, joinAfter: boolean) => {
    if (!track) return;
    setFailedTracks((cur) => ({ ...cur, [track]: joinAfter }));
  }, []);
  const clearFailed = useCallback((track: string) => {
    setFailedTracks((cur) => {
      if (!(track in cur)) return cur;
      const rest = { ...cur };
      delete rest[track];
      return rest;
    });
  }, []);
  // A clock for the watches while anything is installing: a stall shows as nothing
  // happening, and nothing happening re-renders nothing.
  const [tick, setTick] = useState(0);
  const anyInstalling = Object.keys(installing).length > 0;
  useEffect(() => {
    if (!anyInstalling) return;
    const id = window.setInterval(() => setTick((n) => n + 1), 5_000);
    return () => window.clearInterval(id);
  }, [anyInstalling]);

  const watchFrom = useCallback(
    (slug: string, s: MasterServer, joinAfter: boolean) => {
      clearFailed(s.track);
      watches.current.set(slug, startWatch(Date.now(), active.find((a) => a.slug === slug)));
      setInstalling((cur) => ({ ...cur, [slug]: { server: s, joinAfter } }));
    },
    [active, clearFailed],
  );

  const installTrack = useCallback(
    (s: MasterServer, product: CatalogTrack, joinAfter = false) => {
      const slug = product.slug;
      const tracks = modTypesFor(game.id).find((m) => m.id === "tracks");
      if (!slug || !tracks) return;
      watchFrom(slug, s, joinAfter);
      startPendingInstall({
        slug,
        title: product.name,
        subpath: tracks.installSubpath,
        resolve: async () => {
          try {
            // One page fetch. A site that never answers must not keep the tile on Installing.
            const res = await withTimeout(
              resolveQuickInstall(slug, tracks, game, tracks.categoryId),
              RESOLVE_TIMEOUT_MS,
            );
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
              description: e === "timeout" ? t("serverBrowser.installTimedOut") : String(e),
            });
          }
          doneInstalling(slug);
          markFailed(s.track, joinAfter);
          return null;
        },
      });
    },
    [game, startPendingInstall, doneInstalling, markFailed, watchFrom, t],
  );

  // "Pick the track": the player already has the file — downloaded from a page the app
  // couldn't, or handed over by a friend. Same install queue as any import, then the join.
  const pickTrack = useCallback(
    async (s: MasterServer) => {
      const tracks = modTypesFor(game.id).find((m) => m.id === "tracks");
      if (!tracks) return;
      let picked: string | string[] | null = null;
      try {
        picked = await pickFile({
          multiple: false,
          filters: [{ name: t("modDetail.modFiles"), extensions: ["pkz", "zip", "rar", "7z"] }],
        });
      } catch (e) {
        toast.error(t("serverBrowser.installFailed", { title: s.track }), {
          description: String(e),
        });
        return;
      }
      if (typeof picked !== "string") return;
      // Keyed like a catalogue install when there is one, so progress lands on this tile; a
      // track nobody hosts is a plain file import, slug-less like any other.
      const slug = catalog[s.track]?.slug ?? "";
      let destFolder = "";
      try {
        destFolder = localStorage.getItem(destStorageKey(game, tracks)) ?? "";
      } catch {
        // Storage disabled: the tracks root, which the game reads too.
      }
      watchFrom(slug, s, s.joinable);
      startImport({
        slug,
        title: catalog[s.track]?.name || s.track,
        subpath: tracks.installSubpath,
        destFolder,
        categoryId: tracks.categoryId ?? undefined,
        path: picked,
      });
    },
    [game, catalog, startImport, watchFrom, t],
  );

  const installOnly = useCallback(
    (s: MasterServer, product: CatalogTrack) => installTrack(s, product),
    [installTrack],
  );

  const installAndJoin = useCallback(
    (s: MasterServer, product: CatalogTrack) => installTrack(s, product, true),
    [installTrack],
  );

  // A track the player already has, parked by Manage: put it back where the game looks, then
  // join. The same move Manage's own switch makes — nothing is downloaded or copied.
  const activateAndJoin = useCallback(
    async (s: MasterServer, rel: string) => {
      try {
        const out = await modsStateSet([rel], true);
        if (out.failed.length > 0) {
          toast.error(t("serverBrowser.activateFailed"), { description: out.failed[0][1] });
          return;
        }
      } catch (e) {
        toast.error(t("serverBrowser.activateFailed"), { description: String(e) });
        return;
      }
      LIBRARY.delete(s.track);
      LIBRARY_PENDING.delete(s.track);
      CATALOG.delete(s.track);
      setInactive((cur) => {
        const next = { ...cur };
        delete next[s.track];
        return next;
      });
      // Same ordering as a finished install: Rust drops its snapshot before anything re-asks.
      await invoke("invalidate_server_track_cache").catch(() => {});
      setInstalled((n) => n + 1);
      void guessServerTrack(s.track, s.name)
        .then((g) => rememberGuess(s.track, g))
        .catch(() => {});
      if (isFull(s)) void wait(s);
      else void join(s);
    },
    [t, join, wait],
  );

  useEffect(() => {
    const now = Date.now();
    for (const [slug, intent] of Object.entries(installing)) {
      const job = active.find((a) => a.slug === slug);
      const waiting = queued.find((q) => q.slug === slug);
      // A picked file is copied, not downloaded: no byte counts to watch and nothing a timeout
      // should stop, so it reads as local work until it finishes.
      const copying =
        job?.source.kind === "import" && !["done", "error", "review"].includes(job.stage);
      const { watch, verdict } = stepWatch(
        watches.current.get(slug) ?? startWatch(now),
        copying ? { stage: "placing" } : job,
        !!waiting,
        now,
      );
      watches.current.set(slug, watch);
      if (verdict === "running") continue;
      doneInstalling(slug);
      if (verdict === "timeout") {
        // Stop a transfer that stopped moving, so Retry starts clean rather than collapsing
        // onto it. One still waiting behind other installs is left in line: it isn't stuck,
        // it just can't be what the join waits on.
        if (job && (job.stage === "resolving" || job.stage === "downloading")) cancel(job.key);
        else if (waiting?.preparing) cancel(waiting.key);
        toast.error(t("serverBrowser.installFailed", { title: job?.title ?? intent.server.track }), {
          description: t("serverBrowser.installTimedOut"),
        });
      }
      // A pack goes to review and an error has its own card; neither is ready to ride. Nor is
      // a job that stopped or vanished — but none of them may take the join away.
      if (verdict !== "done") {
        if (verdict !== "gone" || job?.stage !== "review") {
          markFailed(intent.server.track, intent.joinAfter);
        }
        continue;
      }
      clearFailed(intent.server.track);
      const s = servers?.find((x) => x.address === intent.server.address) ?? intent.server;
      if (s?.track) {
        // Preview and identification share a backend library snapshot. The completed install
        // is the boundary at which both positive and negative answers become stale.
        LIBRARY.delete(s.track);
        LIBRARY_PENDING.delete(s.track);
        CATALOG.delete(s.track);
        // Refresh only after Rust has dropped its snapshot; otherwise this render can race
        // the invalidation IPC and faithfully re-cache the pre-install directory listing.
        void invoke("invalidate_server_track_cache").finally(() => setInstalled((n) => n + 1));
      }
      if (!intent.joinAfter) continue;
      if (s && isFull(s)) void wait(s);
      else void join(s);
    }
    // `tick` only re-runs the watches while nothing else changes.
  }, [active, queued, installing, servers, join, wait, doneInstalling, markFailed, clearFailed, cancel, t, tick]);

  const installingAt = useMemo(
    () => new Set(Object.values(installing).map(({ server }) => server.address)),
    [installing],
  );
  const failedSet = useMemo(() => new Set(Object.keys(failedTracks)), [failedTracks]);

  /** The same decision a server tile makes, so a friend on a track the player lacks is offered
   *  the install rather than a join the game would fail. Anything that needs the player to
   *  choose (buy, nothing to install, already queued) opens the server's pane instead. */
  const friendJoinKind = useCallback(
    (s: MasterServer) =>
      joinAction({
        missing: !!s.track && library[s.track] === null,
        inactive: inactive[s.track],
        product: catalog[s.track],
        installing: installingAt.has(s.address),
        failed: failedSet.has(s.track),
        queued: queue?.address === s.address,
        joinable: s.joinable,
        full: isFull(s),
      }),
    [library, inactive, catalog, installingAt, failedSet, queue],
  );
  const joinFriend = useCallback(
    (s: MasterServer) => {
      const action = friendJoinKind(s);
      switch (action.kind) {
        // A failed install still joins: that is what the friend's button promises.
        case "join":
        case "failed":
          void join(s);
          break;
        case "wait":
          void wait(s);
          break;
        case "install":
          installAndJoin(s, action.product);
          break;
        case "activate":
          void activateAndJoin(s, action.rel);
          break;
        default:
          pick(s);
      }
    },
    [friendJoinKind, join, wait, installAndJoin, activateAndJoin, pick],
  );
  const friendsHere = useMemo(() => {
    const by = friendsByAddress(friendsFeed.state?.friends ?? [], known ?? []);
    return Object.fromEntries(Object.entries(by).map(([address, list]) => [address, list.length]));
  }, [friendsFeed.state, known]);

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
  const pictureFor = (track: string) => library[track] || guessPicture(track) || undefined;

  const detailProps = {
    server: detail,
    art: detail ? pictureFor(detail.track) : undefined,
    missing: !!detail?.track && library[detail.track] === null,
    product: detail ? catalog[detail.track] : undefined,
    installing: !!detail && installingAt.has(detail.address),
    failed: !!detail && failedSet.has(detail.track),
    onPickTrack: pickTrack,
    favourite: !!detail && favs.has(detail.address),
    joining,
    busy: joining !== null,
    queue,
    onJoin: join,
    onWait: wait,
    onInstall: installOnly,
    onInstallJoin: installAndJoin,
    inactive: detail ? inactive[detail.track] : undefined,
    onActivateJoin: activateAndJoin,
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
            <DropdownMenuItem onSelect={() => setSavedDialog({ editing: null })}>
              <Bookmark className="size-4" />
              {t("savedServers.add")}
            </DropdownMenuItem>
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
      <SavedServerDialog
        open={savedDialog !== null}
        onOpenChange={(open) => !open && setSavedDialog(null)}
        editing={savedDialog?.editing ?? null}
        onSubmit={(address, name) => {
          const editing = savedDialog?.editing;
          return editing ? saved.edit(editing.address, address, name) : saved.add(address, name);
        }}
      />
      {/* A tile has no list beside it to put the pane next to, so from the grid it still
          opens over the top — the same pane, in a dialog. So does a saved card while the list
          below it is empty, since there is no pane on screen for it to fill. */}
      {(view === "tiles" || shown.length === 0) && (
        <ServerDetailDialog
          {...detailProps}
          onOpenChange={(open) => !open && setSelected(null)}
        />
      )}

      {friendsFeed.state && (
        <FriendsPanel
          state={friendsFeed.state}
          servers={known ?? []}
          actionFor={(ref) => {
            const s = (known ?? []).find((x) => x.address === ref.address);
            return s ? friendJoinKind(s).kind : "missing";
          }}
          onJoin={(ref) => {
            const s = (known ?? []).find((x) => x.address === ref.address);
            if (s) joinFriend(s);
          }}
          onChanged={() => void friendsFeed.refresh()}
        />
      )}

      <SavedServers
        rows={savedRows}
        cards={{
          pictureFor,
          library,
          catalog,
          installingAt,
          failedTracks: failedSet,
          onPickTrack: pickTrack,
          favourite: favs.has,
          paintSync,
          friends: friendsHere,
          joining,
          queue,
          onOpen: pick,
          onJoin: join,
          onWait: wait,
          onInstall: installOnly,
          onInstallJoin: installAndJoin,
          inactive,
          onActivateJoin: activateAndJoin,
          onCopy: copy,
          onToggleFavourite: favs.toggle,
        }}
        onAdd={() => setSavedDialog({ editing: null })}
        onEdit={(entry) => setSavedDialog({ editing: entry })}
        onRemove={removeSaved}
        onMove={moveSaved}
      />

      <div
        className={cn(
          "min-h-0 flex-1 px-7 pb-6",
          // The list view scrolls its two columns separately; everything else scrolls whole.
          view === "list" && shown.length > 0 ? "flex gap-4" : "overflow-y-auto",
        )}
      >
        {servers === null ? (
          <Centered>
            <LoadingMark label={t("serverBrowser.loading")} />
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
                missing={!!s.track && library[s.track] === null}
                product={catalog[s.track]}
                installing={installingAt.has(s.address)}
                failed={failedSet.has(s.track)}
                onPickTrack={pickTrack}
                onInstall={installOnly}
                onInstallJoin={installAndJoin}
                inactive={inactive[s.track]}
                onActivateJoin={activateAndJoin}
                favourite={favs.has(s.address)}
                paintSync={paintSync[s.address] ?? 0}
                friends={friendsHere[s.address] ?? 0}
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
                    missing={!!s.track && library[s.track] === null}
                    product={catalog[s.track]}
                    selected={s.address === selected}
                    favourite={favs.has(s.address)}
                    paintSync={paintSync[s.address] ?? 0}
                    friends={friendsHere[s.address] ?? 0}
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
