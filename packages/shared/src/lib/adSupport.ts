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
}

/** On by default: the whole point is that the creator keeps the impression unless a player
 *  deliberately opts out. */
export const DEFAULT_AD_SUPPORT: AdSupportPrefs = { enabled: true };

const KEY = "frost-ad-support";

export function readAdSupport(): AdSupportPrefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return DEFAULT_AD_SUPPORT;
    const v = JSON.parse(raw) as Partial<AdSupportPrefs>;
    // Anything but an explicit `false` reads as on, so a corrupt or half-written value
    // fails toward supporting the creator rather than silently opting the player out.
    return { enabled: v.enabled !== false };
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
