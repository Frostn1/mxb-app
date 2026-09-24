import { invoke } from "@tauri-apps/api/core";

/**
 * MXBMRP3, thomas4f's open-source HUD plugin (standings, timing, gap to your best, track map,
 * radar), and whether to suggest it. The check is in `src-tauri/src/mxbmrp3.rs`. It only ever
 * looks for the plugin file; the app never downloads or installs it.
 */
export interface Mxbmrp3Status {
  /** `null` when the game folder isn't known: "don't know", not "not installed". */
  installed: boolean | null;
  /** "Don't ask again" was chosen. */
  dismissed: boolean;
  /** The author's releases page, where the installer is. */
  downloadUrl: string;
}

export const mxbmrp3Status = () => invoke<Mxbmrp3Status>("mxbmrp3_status");

export const setMxbmrp3Dismissed = (dismissed: boolean) =>
  invoke<void>("set_mxbmrp3_dismissed", { dismissed });

/**
 * "Not now", for the rest of this session. Module-level so that saying it on the last setup
 * step also holds for the bar the dashboard would show a moment later.
 */
let snoozed = false;
export const snoozeMxbmrp3 = () => {
  snoozed = true;
};
export const mxbmrp3Snoozed = () => snoozed;

/**
 * Whether to put the suggestion in front of the rider: only when the plugin is known to be
 * missing, they haven't said never, and they haven't said not now this session. A game folder
 * that can't be found suggests nothing.
 */
export function shouldSuggest(status: Mxbmrp3Status | null, snoozedNow: boolean): boolean {
  return status !== null && status.installed === false && !status.dismissed && !snoozedNow;
}
