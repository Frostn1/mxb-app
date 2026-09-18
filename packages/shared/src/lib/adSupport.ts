/**
 * Whether the app shows a mod's own mxb-mods.com page while you browse and install it.
 *
 * The app browses the catalog through mxb-mods.com's REST API and installs straight from the
 * mirror, so a player never loads the mod's page — and the ads on that page are how the site
 * and the people who run it earn. On (the default) the app opens the real page behind its own
 * window while a mod is open or installing, restoring that view. Off, browsing goes back to
 * the app-only path and the site earns nothing from your visit — which is why turning it off
 * is guarded (Settings → General).
 *
 * One stored flag, read wherever a mod page is opened or an install starts.
 */
export interface AdSupportPrefs {
  /** Open the creator's page in the background while viewing/installing a mod. */
  enabled: boolean;
  /**
   * Tuck the page *behind* the app instead of beside it.
   *
   * Off by default: beside the app the page is fully on screen, which is what an ad network
   * actually counts as a view — a page hidden behind the app may earn the creator nothing. On
   * for a player who'd rather it stayed out of the way, at the cost of that revenue.
   */
  behindApp: boolean;
}

/** On, and beside the app, by default: the creator keeps the impression — and it's a real,
 *  visible one — unless a player deliberately changes it. */
export const DEFAULT_AD_SUPPORT: AdSupportPrefs = { enabled: true, behindApp: false };

const KEY = "frost-ad-support";

export function readAdSupport(): AdSupportPrefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_AD_SUPPORT;
    const v = JSON.parse(raw) as Partial<AdSupportPrefs>;
    // Anything but an explicit `false` reads as on, so a corrupt or half-written value
    // fails toward supporting the creator rather than silently opting the player out.
    // `behindApp` is the opposite: only an explicit `true` tucks it away, so the default
    // (beside, and visible) is what a missing or garbled value gives.
    return { enabled: v.enabled !== false, behindApp: v.behindApp === true };
  } catch {
    return DEFAULT_AD_SUPPORT;
  }
}

export function writeAdSupport(prefs: AdSupportPrefs): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // A browser that refuses storage just means the default (on) next launch — no worse than
    // not having saved, and never a reason to fail the toggle.
  }
}
