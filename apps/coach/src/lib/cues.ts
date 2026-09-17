/** What the rider picked for their live cues, and where it is kept.
 *
 *  Shared, because two things need it: the Live cues panel where the rider sets it, and the
 *  keeper at the app root that writes the sheet while they ride. Kept private to the panel,
 *  the two would drift and the keeper would quietly coach at the wrong level.
 */
export const CUE_LEVEL_KEY = "coach-cue-level";
export const CUE_AMOUNT_KEY = "coach-cue-amount";

export const LEVELS = ["new", "intermediate", "subPro", "pro"] as const;
export const AMOUNTS = ["few", "normal", "lots"] as const;

export function remembered<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v && (allowed as readonly string[]).includes(v) ? (v as T) : fallback;
  } catch {
    return fallback;
  }
}

export function remember(key: string, v: string) {
  try {
    localStorage.setItem(key, v);
  } catch {
    /* no storage: the choice lasts the session */
  }
}
