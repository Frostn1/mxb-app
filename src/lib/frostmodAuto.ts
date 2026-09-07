import type { FrostmodStatus } from "../types";

/** Everything outside the status snapshot that can hold an unattended install back. */
export interface AutoGates {
  /** The last GitHub check failed — the snapshot is a guess, so act on nothing. */
  statusError: boolean;
  /** An install is already in flight. */
  installing: boolean;
  /** MX Bikes is up. Only blocks an update, never a repair. */
  gameRunning: boolean;
  /** A repair has already been attempted this session. */
  triedRepair: boolean;
  /** The release tag an update has already been attempted for, if any. */
  updatedTo: string | null;
}

/**
 * What, if anything, the app should install without anyone pressing a button.
 *
 * Pure, and in a module of its own, because it is the only thing in the app that starts a
 * download unasked: get it wrong in one direction and a player sits on a FrostMod that
 * can't run, in the other and it reinstalls on a loop.
 *
 * - `"repair"` — missing, half-applied, or too old for the active title. FrostMod does
 *   nothing at all in these states, so this runs even mid-game, and once per session.
 * - `"update"` — works, but there's a newer release. Waits for the game to quit, and is
 *   tried once per tag so a bad release doesn't retry every poll while a newer one still
 *   gets its own attempt.
 */
export function autoInstallAction(
  status: FrostmodStatus | null,
  gates: AutoGates,
): "repair" | "update" | null {
  if (!status || gates.statusError || gates.installing) return null;

  if (
    !gates.triedRepair &&
    (!status.installed || status.needsRepair || !status.supportedForGame)
  ) {
    return "repair";
  }

  // Ordered after the repair arm and never alongside it: a FrostMod that is both
  // unsupported and out of date must start one install, not two.
  const { latest } = status;
  if (
    status.installed &&
    latest &&
    status.version !== latest &&
    gates.updatedTo !== latest &&
    !gates.gameRunning
  ) {
    return "update";
  }

  return null;
}
