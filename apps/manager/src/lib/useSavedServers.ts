import { useCallback, useEffect, useMemo, useState } from "react";
import {
  savedServers,
  addSavedServer,
  editSavedServer,
  removeSavedServer,
  reorderSavedServers,
  type SavedServer,
} from "@frost/shared/api/mods";

export interface SavedServers {
  list: SavedServer[];
  add: (address: string, name: string) => Promise<SavedServer[]>;
  edit: (address: string, newAddress: string, name: string) => Promise<SavedServer[]>;
  remove: (address: string) => Promise<SavedServer[]>;
  /** Move one saved server `by` places, earlier when negative. */
  move: (address: string, by: number) => Promise<SavedServer[]>;
  /** Put the whole list in this order, by address. */
  reorder: (order: string[]) => Promise<SavedServer[]>;
}

/**
 * The servers the player saved by address, from `config.json`.
 *
 * Unlike the stars ({@link useFavorites}) these live in the app config: they carry a name and
 * an order, and validating an address is the Rust parser's job, so every change goes through a
 * command and the list on screen is always the list it stored.
 */
export function useSavedServers(): SavedServers {
  const [list, setList] = useState<SavedServer[]>([]);

  useEffect(() => {
    let live = true;
    savedServers()
      .then((l) => live && setList(l))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, []);

  const keep = useCallback((l: SavedServer[]) => {
    setList(l);
    return l;
  }, []);

  const add = useCallback(
    (address: string, name: string) => addSavedServer(address, name).then(keep),
    [keep],
  );
  const edit = useCallback(
    (address: string, newAddress: string, name: string) =>
      editSavedServer(address, newAddress, name).then(keep),
    [keep],
  );
  const remove = useCallback(
    (address: string) => removeSavedServer(address).then(keep),
    [keep],
  );
  const reorder = useCallback(
    (order: string[]) => reorderSavedServers(order).then(keep),
    [keep],
  );
  const move = useCallback(
    (address: string, by: number) => {
      const order = list.map((s) => s.address);
      const from = order.indexOf(address);
      const to = Math.max(0, Math.min(order.length - 1, from + by));
      if (from < 0 || from === to) return Promise.resolve(list);
      order.splice(to, 0, ...order.splice(from, 1));
      return reorder(order);
    },
    [list, reorder],
  );

  return useMemo(
    () => ({ list, add, edit, remove, move, reorder }),
    [list, add, edit, remove, move, reorder],
  );
}
