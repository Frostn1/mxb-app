import { useCallback, useEffect, useRef, useState } from "react";
import { friendsList, type FriendsState } from "../api/friends";

/** The control plane forgets a friend two minutes after their last heartbeat; asking twice
 *  inside that is the right grain. */
const POLL_MS = 30_000;

/**
 * The friends list, kept fresh while something is looking at it.
 *
 * `state` stays on the last good answer when a refresh fails, so a dropped connection does not
 * empty the panel; `error` says the refresh did not land. Polling pauses while the window is
 * hidden: presence only matters to someone who can see it.
 */
export function useFriends(): {
  state: FriendsState | null;
  error: string | null;
  refresh: () => Promise<void>;
} {
  const [state, setState] = useState<FriendsState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const alive = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const next = await friendsList();
      if (!alive.current) return;
      setState(next);
      setError(null);
    } catch (e) {
      if (alive.current) setError(typeof e === "string" ? e : String(e));
    }
  }, []);

  useEffect(() => {
    alive.current = true;
    void refresh();
    const id = window.setInterval(() => {
      if (!document.hidden) void refresh();
    }, POLL_MS);
    return () => {
      alive.current = false;
      window.clearInterval(id);
    };
  }, [refresh]);

  return { state, error, refresh };
}
