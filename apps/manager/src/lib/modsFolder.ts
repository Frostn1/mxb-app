import type { GameFolderCheck } from "@frost/shared/api/mods";

/**
 * The folder the game reads by default, when the saved mods folder is somewhere else *and*
 * that default folder is a real game folder too.
 *
 * Both existing is the case worth a warning: a stale copy next to the one MX Bikes uses is
 * what "everything I install is missing in-game" looks like. A saved folder that is simply
 * elsewhere (the default folder isn't there, so nothing else could be the one in use) is a
 * legitimate relocation and says nothing.
 */
export function defaultFolderInUse(
  saved: GameFolderCheck,
  defaultCheck: GameFolderCheck | null,
): string | null {
  if (!saved.usable || saved.matchesExpected || !saved.expected) return null;
  if (!defaultCheck || !defaultCheck.exists || !defaultCheck.usable) return null;
  return saved.expected;
}
