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
 * Windows and macOS both really probe (Win32 on one, `ps` on the other — under Wine the
 * game is a normal process). Linux is permanently `false`, which is harmless: the backend
 * routes through Steam, and Steam focuses the running game rather than starting a second.
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
