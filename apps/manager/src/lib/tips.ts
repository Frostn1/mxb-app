/**
 * Tips: short notes about features people tend to miss, shown one at a time on the home
 * screen and listed in full under Settings → Tips.
 *
 * Kept gentle on purpose. A tip shows at most once per launch, at most once ever (it counts
 * as done the moment it is shown), never within a couple of days of the last one, never
 * while something is downloading or installing, and never at all once the player turns tips
 * off. Each tip also carries its own condition — the paint sync tip is only for someone who
 * hasn't turned it on — so a tip about something already in use never shows.
 *
 * This module is the pure part: what's remembered and which tip, if any, is next. The tips
 * themselves (their text, condition and action) live with the components that need hooks.
 */

/** One tip as the picker sees it. */
export interface TipCandidate {
  id: string;
  /** `true` when it applies, `false` when it doesn't, `null` while that's still being asked
   *  (a backend call in flight). A tip still being asked holds back the ones after it, so a
   *  slow answer can't let a lower-priority tip jump the queue. */
  eligible: boolean | null;
}

/** What is remembered between launches. */
export interface TipsState {
  /** Tips already shown, dismissed or acted on. None of these shows again by itself. */
  done: string[];
  /** When the last tip was shown, in ms since the epoch. `0` if never. */
  lastShownAt: number;
  /** "Don't show tips" in Settings. */
  off: boolean;
}

/** The shortest gap between two tips, so they stay occasional. */
export const TIP_GAP_MS = 2 * 24 * 60 * 60 * 1000;

const STORAGE_KEY = "mxb.tips";

const EMPTY: TipsState = { done: [], lastShownAt: 0, off: false };

export function readTipsState(): TipsState {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return { ...EMPTY };
    const parsed = JSON.parse(raw) as Partial<TipsState>;
    return {
      done: Array.isArray(parsed.done) ? parsed.done.filter((d) => typeof d === "string") : [],
      lastShownAt: typeof parsed.lastShownAt === "number" ? parsed.lastShownAt : 0,
      off: parsed.off === true,
    };
  } catch {
    return { ...EMPTY };
  }
}

export function writeTipsState(state: TipsState): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    /* A locked-down webview just forgets; the worst case is a tip shown again. */
  }
  listeners.forEach((fn) => fn());
}

const listeners = new Set<() => void>();

/** For `useSyncExternalStore`: Settings and the home card share one record. */
export function subscribeTips(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** Record a tip as done (shown, dismissed or acted on). */
export function markTipDone(state: TipsState, id: string, now?: number): TipsState {
  return {
    ...state,
    done: state.done.includes(id) ? state.done : [...state.done, id],
    lastShownAt: now ?? state.lastShownAt,
  };
}

/**
 * The next tip to show, `null` for none, or `"wait"` while a tip ahead of the rest is still
 * working out whether it applies. Candidates are in priority order.
 */
export function pickTip(
  candidates: TipCandidate[],
  state: TipsState,
  now: number,
): string | "wait" | null {
  if (state.off) return null;
  if (state.lastShownAt > 0 && now - state.lastShownAt < TIP_GAP_MS) return null;
  for (const c of candidates) {
    if (state.done.includes(c.id)) continue;
    if (c.eligible === null) return "wait";
    if (c.eligible) return c.id;
  }
  return null;
}

/** `v0.49.9` / `0.49.9` is at least `min`. Blank or unreadable is never enough. */
export function versionAtLeast(version: string | null | undefined, min: string): boolean {
  if (!version) return false;
  const parts = (v: string) =>
    v
      .trim()
      .replace(/^v/i, "")
      .replace(/[-+].*$/, "")
      .split(".")
      .map((n) => Number.parseInt(n, 10));
  const a = parts(version);
  if (a.some((n) => Number.isNaN(n))) return false;
  const b = parts(min);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const diff = (a[i] ?? 0) - (b[i] ?? 0);
    if (diff !== 0) return diff > 0;
  }
  return true;
}
