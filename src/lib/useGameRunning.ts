import { useCallback, useEffect, useRef, useState } from "react";
import { isGameRunning } from "../api/mods";

/** Same cadence as the FrostMod probe in `Context/Frostmod.tsx`. */
const POLL_MS = 5000;

/**
 * Whether MX Bikes is running, polled in the background.
 *
 * A separate thing from FrostMod's own process, which is why this stayed out of
 * `FrostmodProvider`'s probe — that context consumes this hook rather than folding the two
 * answers into one call. `refresh` is exposed so a launch can check straight away instead
 * of waiting out the interval.
 *
 * All three platforms really probe: Win32 on Windows, `ps` on macOS and `/proc` on Linux,
 * where the game is an ordinary process under Wine/Proton whose argv still names the exe.
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
