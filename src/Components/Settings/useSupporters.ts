import { useCallback, useEffect, useRef, useState } from "react";
import {
  BUNDLED_SUPPORTERS,
  fetchSupporters,
  readCachedManifest,
  type SupportersManifest,
} from "./supporters";

/**
 * The live supporters list.
 *
 * Starts from whatever is already on disk so the section paints filled in, then goes to
 * the network. A failed fetch keeps what's showing and flips `stale` — the alternative,
 * emptying the page because GitHub was unreachable for a second, would tell the player
 * the app has no supporters.
 *
 * Lives here rather than inside `SupportersCard` because the release showcase credits
 * the same people, and two copies would drift.
 */
export function useSupporters() {
  const [manifest, setManifest] = useState<SupportersManifest>(
    () => readCachedManifest() ?? BUNDLED_SUPPORTERS,
  );
  const [loading, setLoading] = useState(true);
  const [stale, setStale] = useState(false);
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const fresh = await fetchSupporters();
      if (!alive.current) return;
      setManifest(fresh);
      setStale(false);
    } catch {
      if (!alive.current) return;
      setStale(true);
    } finally {
      if (alive.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { manifest, loading, stale, refresh };
}
