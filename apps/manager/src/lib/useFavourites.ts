import { useCallback, useEffect, useMemo, useState } from "react";
import type { MasterServer } from "@frost/shared/api/mods";

/**
 * Starred servers, remembered across launches.
 *
 * Kept in `localStorage` beside the browser's other sticky preferences rather than in the app
 * config: this is a per-machine convenience, and the backend has no use for it.
 *
 * A favourite is stored as more than an address. The master only describes servers that are
 * *up*, so a starred server that has gone offline would otherwise be a bare `ip:port` with
 * nothing to show. The name and track are cached here precisely so the favourites view can
 * still name it. Those cached fields are refreshed whenever the server appears in a live
 * list, so what's shown offline is the last thing that was actually true.
 */
const FAVS_KEY = "mxb:serversFavourites:v1";

/** A starred server, and the last thing the master said about it. */
export interface Favourite {
  /** `ip:port`, the identity. A server keeps its address across restarts; its name doesn't. */
  address: string;
  /** Last-known display name, for showing a favourite that's currently offline. */
  name: string;
  /** Last-known track, same reason. */
  track: string;
  /** When it was starred, so the offline list has a stable order of its own. */
  savedAt: number;
}

function read(): Favourite[] {
  try {
    const raw = localStorage.getItem(FAVS_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Written by an older build, or hand-edited: keep only entries that still have an
    // address, and fill the rest in rather than dropping a star the player set.
    return parsed
      .filter((f): f is Partial<Favourite> => !!f && typeof f === "object")
      .filter((f) => typeof f.address === "string" && f.address.length > 0)
      .map((f) => ({
        address: f.address as string,
        name: typeof f.name === "string" ? f.name : "",
        track: typeof f.track === "string" ? f.track : "",
        savedAt: typeof f.savedAt === "number" ? f.savedAt : 0,
      }));
  } catch {
    // Corrupt JSON, or a browser that refuses storage. An empty list is the right fallback:
    // the tab still works, it just has nothing starred.
    return [];
  }
}

function write(list: Favourite[]) {
  try {
    localStorage.setItem(FAVS_KEY, JSON.stringify(list));
  } catch {
    // Out of quota or storage disabled; the star still applies for this session.
  }
}

export interface Favourites {
  /** Every starred server, newest star first. */
  list: Favourite[];
  has: (address: string) => boolean;
  /** Star an unstarred server, or unstar a starred one. */
  toggle: (server: Pick<MasterServer, "address" | "name" | "track">) => void;
  count: number;
}

/**
 * `servers` is the live list, used only to keep each favourite's cached name and track
 * current. Passing `null` (nothing loaded yet) leaves what's stored alone: an empty list
 * must never be read as "these servers are all gone".
 */
export function useFavourites(servers: MasterServer[] | null): Favourites {
  const [list, setList] = useState<Favourite[]>(read);

  // Another window of the app can star something too. `storage` fires only in the *other*
  // documents, so this syncs them without echoing our own writes back.
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key === FAVS_KEY) setList(read());
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, []);

  // Refresh the cached name/track of any favourite the live list covers.
  useEffect(() => {
    if (!servers || servers.length === 0) return;
    const live = new Map(servers.map((s) => [s.address, s]));
    setList((prev) => {
      let changed = false;
      const next = prev.map((f) => {
        const s = live.get(f.address);
        if (!s || (s.name === f.name && s.track === f.track)) return f;
        changed = true;
        return { ...f, name: s.name, track: s.track };
      });
      if (!changed) return prev;
      write(next);
      return next;
    });
  }, [servers]);

  const addresses = useMemo(() => new Set(list.map((f) => f.address)), [list]);

  const has = useCallback((address: string) => addresses.has(address), [addresses]);

  const toggle = useCallback(
    (server: Pick<MasterServer, "address" | "name" | "track">) => {
      setList((prev) => {
        const next = prev.some((f) => f.address === server.address)
          ? prev.filter((f) => f.address !== server.address)
          : [
              { address: server.address, name: server.name, track: server.track, savedAt: Date.now() },
              ...prev,
            ];
        write(next);
        return next;
      });
    },
    [],
  );

  // Memoised: this object is a dependency of the browser's filter/sort memo, and a fresh
  // identity every render would make that memo recompute on every render.
  return useMemo(() => ({ list, has, toggle, count: list.length }), [list, has, toggle]);
}
