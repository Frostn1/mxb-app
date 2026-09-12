import { useCallback, useEffect, useMemo, useState } from "react";

/** Starred servers, by `ip:port`. Per machine, so `localStorage` rather than the app config. */
const KEY = "mxb:serversFavorites:v1";

function read(): Set<string> {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(KEY) ?? "[]");
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

export function useFavorites(): Favorites {
  const [starred, setStarred] = useState(read);

  useEffect(() => {
    try {
      localStorage.setItem(KEY, JSON.stringify([...starred]));
    } catch {
      // Storage disabled; the star still holds for this session.
    }
  }, [starred]);

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
