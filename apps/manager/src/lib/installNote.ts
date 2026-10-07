import type { ReloadOutcome } from "@frost/shared/types";
import type { TKey } from "@/i18n";

/**
 * What an install's success toast says about the running game.
 *
 * FrostMod reloads content live, but only when it is running and the reload signal got
 * through. Without it the game keeps the list it built at startup, so a mod installed
 * while the game is open does not appear until it restarts. Saying nothing about that
 * reads as "installed but broken", which is how it is reported.
 */
export function installNoteKey(
  reload: ReloadOutcome | null,
  gameRunning: boolean,
): TKey {
  if (reload === "signaled") return "install.reloadedDesc";
  return gameRunning ? "install.restartDesc" : "install.addedDesc";
}
