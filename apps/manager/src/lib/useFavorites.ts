import { useCallback, useEffect, useMemo, useState } from "react";

/** Starred servers, by `ip:port`. Per machine, so `localStorage` rather than the app config. */
const SERVERS_KEY = "mxb:serversFavorites:v1";

function read(key: string): Set<string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(key) ?? "[]");
    return new Set(Array.isArray(parsed) ? parsed.filter((a) => typeof a === "string") : []);
  } catch {
    return new Set();
  }
}

export interface Favorites {
  has: (address: string) => boolean;
  toggle: (address: string) => void;
  count: number;
}

export function useFavorites(key: string = SERVERS_KEY): Favorites {
  const [starred, setStarred] = useState(() => read(key));

  useEffect(() => {
    try {
      localStorage.setItem(key, JSON.stringify([...starred]));
    } catch {
      // Storage disabled; the star still holds for this session.
    }
  }, [key, starred]);

  const toggle = useCallback((address: string) => {
    setStarred((prev) => {
      const next = new Set(prev);
      if (!next.delete(address)) next.add(address);
      return next;
    });
  }, []);

  return useMemo(
    () => ({ has: (a: string) => starred.has(a), toggle, count: starred.size }),
    [starred, toggle],
  );
}
