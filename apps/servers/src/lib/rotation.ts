import type { RotationResult } from "./api";

/** What to tell the user after a rotation save, and whether a restart is still needed. */
export function describeRotationSave(result: Pick<RotationResult, "live" | "restartRequired">): { message: string; needsRestart: boolean } {
  if (result.restartRequired) {
    return {
      message: "Saved, but this server is too old to change tracks while it runs. Restart it to use the new rotation.",
      needsRestart: true,
    };
  }
  return { message: "Rotation saved. It applies at the next track load; nobody is disconnected.", needsRestart: false };
}

/** The order that makes `selected` the track after the running one: it first, the others as
 *  they were, the running track last. */
export function playNextQueue(current: string, rotation: string[], selected: string): string[] {
  return [selected, ...rotation.filter((track) => track !== selected), current];
}

/** One of `rotation` at random, or null when it is empty. */
export function randomTrack(rotation: string[], random: () => number = Math.random): string | null {
  if (rotation.length === 0) return null;
  return rotation[Math.min(rotation.length - 1, Math.floor(random() * rotation.length))];
}

/** What the server reports once queue (from playNextQueue) is saved and rotated to: its
 *  first track is running, the rest is the rotation. Shown at once while the server works. */
export function stateAfterSwitch<T extends { current: string | null; rotation: string[] }>(state: T, queue: string[]): T {
  return { ...state, current: queue[0] ?? null, rotation: queue.slice(1) };
}
/** Why a protected package can't be used (shown in the library and the rotation). */
export const PROTECTED_REASON = "Can't run on this server (protected package)";

export const isProtected = (track: string, protectedTracks: readonly string[] | undefined): boolean => !!protectedTracks?.includes(track);

/** `null` when `track` may join the rotation, otherwise the reason it can't. */
export function addBlockedReason(track: string, protectedTracks: readonly string[] | undefined): string | null {
  return isProtected(track, protectedTracks) ? `${track}: ${PROTECTED_REASON}.` : null;
}

/** The queue's tracks that can't run, so a save that would fail is never sent. */
export function blockedInQueue(queue: readonly string[], protectedTracks: readonly string[] | undefined): string[] {
  return Array.from(new Set(queue.filter((track) => isProtected(track, protectedTracks))));
}