import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * "Keep Coach on top": the window stays above other windows, MX Bikes' among them. Off unless
 * the rider turns it on. Coach sits beside the game while it's ridden, and clicking into the
 * game put the Coach window behind it, where the rider had to alt-tab to find it again.
 *
 * Remembered per PC in localStorage and applied again at every launch; the window itself
 * doesn't keep the flag.
 */
const KEEP_ON_TOP_KEY = "coach-keep-on-top";

export function keepOnTop(): boolean {
  try {
    return localStorage.getItem(KEEP_ON_TOP_KEY) === "1";
  } catch {
    return false;
  }
}

/** Remember the choice and apply it to the window now. */
export async function setKeepOnTop(on: boolean): Promise<void> {
  try {
    localStorage.setItem(KEEP_ON_TOP_KEY, on ? "1" : "0");
  } catch {
    // Storage blocked: it still applies for this run.
  }
  await applyKeepOnTop(on);
}

/** Put the window on top, or not, outside the Tauri runtime a no-op. */
export async function applyKeepOnTop(on = keepOnTop()): Promise<void> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  await getCurrentWindow().setAlwaysOnTop(on);
}
