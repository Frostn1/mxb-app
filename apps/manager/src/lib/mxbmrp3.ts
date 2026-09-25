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
 * Whether to put the suggestion in front of the rider: only when the plugin is known to be
 * missing, they haven't said never, and they haven't said not now this session. A game folder
 * that can't be found suggests nothing.
 */
export function shouldSuggest(status: Mxbmrp3Status | null, snoozedNow: boolean): boolean {
  return status !== null && status.installed === false && !status.dismissed && !snoozedNow;
}

export interface Mxbmrp3State {
  status: Mxbmrp3Status | null;
  /** "Not now", for the rest of this session. */
  snoozed: boolean;
}

/**
 * One copy of the status and the session's "not now", shared by every place the suggestion
 * shows. With a copy each, "Suggest it again" in Settings left the Dashboard bar hidden, and
 * "Not now" on the setup card didn't reach the bar mounted a moment later.
 */
export function createMxbmrp3Store(load: () => Promise<Mxbmrp3Status> = mxbmrp3Status) {
  let state: Mxbmrp3State = { status: null, snoozed: false };
  const listeners = new Set<() => void>();
  const set = (next: Partial<Mxbmrp3State>) => {
    state = { ...state, ...next };
    listeners.forEach((l) => l());
  };
  return {
    get: () => state,
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    async refresh() {
      try {
        set({ status: await load() });
      } catch {
        set({ status: null });
      }
    },
    snooze: () => set({ snoozed: true }),
    /** "Suggest it again" also undoes this session's "not now": the rider just asked for it. */
    unsnooze: () => set({ snoozed: false }),
  };
}

export const mxbmrp3Store = createMxbmrp3Store();
