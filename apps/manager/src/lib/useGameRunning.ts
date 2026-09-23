import { useSyncExternalStore } from "react";
import { isGameRunning } from "@frost/shared/api/mods";
import { AsyncPollingStore } from "./asyncPollingStore";

/** Same cadence as the FrostMod probe in `Context/Frostmod.tsx`. */
const POLL_MS = 5000;
const runningStore = new AsyncPollingStore(false, false, POLL_MS, isGameRunning);

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
  const running = useSyncExternalStore(
    runningStore.subscribe,
    runningStore.getSnapshot,
    runningStore.getSnapshot,
  );
  return { running, refresh: () => void runningStore.refresh() };
}
