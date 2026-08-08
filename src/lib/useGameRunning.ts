import { useCallback, useEffect, useRef, useState } from "react";
import { isGameRunning } from "../api/mods";

/** Same cadence as the FrostMod probe in `Context/Frostmod.tsx`. */
const POLL_MS = 5000;

/**
 * Whether MX Bikes is running, polled in the background.
 *
 * Kept out of `FrostmodProvider` on purpose: that context tracks FrostMod's own process,
 * and the game is a separate thing to watch. `refresh` is exposed so a launch can check
 * straight away instead of waiting out the interval.
 *
 * Note the probe is Win32-only, so off Windows this is permanently `false` — a Linux
 * player sees Play rather than "MX Bikes running", which is harmless: the backend still
 * routes through Steam, which focuses the running game instead of starting a second one.
 */
export function useGameRunning(): { running: boolean; refresh: () => void } {
  const [running, setRunning] = useState(false);
  const mounted = useRef(true);

  const refresh = useCallback(() => {
    isGameRunning()
      .then((r) => mounted.current && setRunning(r))
      .catch(() => mounted.current && setRunning(false));
  }, []);

  useEffect(() => {
    mounted.current = true;
    refresh();
    const id = setInterval(refresh, POLL_MS);
    return () => {
      mounted.current = false;
      clearInterval(id);
    };
  }, [refresh]);

  return { running, refresh };
}
