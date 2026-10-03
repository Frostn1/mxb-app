import type { RotationResult } from "./api";

/** What to tell the user after a rotation save, and whether a restart is still needed. */
export function describeRotationSave(result: Pick<RotationResult, "live" | "restartRequired">): { message: string; needsRestart: boolean } {
  if (result.restartRequired) {
    return {
      message: "Saved, but this server is too old to change tracks while it runs. Restart it to use the new rotation.",
      needsRestart: true,
    };
  }
  return { message: "Rotation saved. It applies at the next event; nobody is disconnected.", needsRestart: false };
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
