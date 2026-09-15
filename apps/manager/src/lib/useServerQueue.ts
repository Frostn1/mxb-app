import { useEffect, useState } from "react";
import { onServerQueue, queueStatus, type QueueState } from "@frost/shared/api/mods";

/** The server line this app is in, or null. Kept live by the backend's `server-queue` event. */
export function useServerQueue(): QueueState | null {
  const [queue, setQueue] = useState<QueueState | null>(null);

  useEffect(() => {
    let alive = true;
    queueStatus()
      .then((q) => alive && setQueue(q))
      .catch(() => {});
    const unlisten = onServerQueue((q) => {
      if (!alive) return;
      setQueue(q.phase === "joined" || q.phase === "ended" ? null : q);
    });
    return () => {
      alive = false;
      unlisten.then((f) => f()).catch(() => {});
    };
  }, []);

  return queue;
}

/** A server the game will turn away. */
export function isFull(s: { players: number; maxPlayers: number }): boolean {
  return s.maxPlayers > 0 && s.players >= s.maxPlayers;
}
