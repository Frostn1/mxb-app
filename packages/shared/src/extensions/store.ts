/**
 * The installed extensions and the looks picked from them, as one small external store.
 *
 * Installed state lives in the backend (`ext_list`); the *picks* — which extension theme,
 * button style and background are on — live in localStorage next to the colorway pick they
 * sit on top of, so the overlay window and a second app window follow along through the
 * storage event, exactly like `Context/Theme.tsx` does for the colorway.
 */

import { useSyncExternalStore } from "react";
import { extList } from "./api";
import type { InstalledExtension } from "./manifest";

export interface ExtSnapshot {
  loaded: boolean;
  developerMode: boolean;
  installed: InstalledExtension[];
  /** `<extId>/<itemId>` or null, for each kind of look. */
  picks: Picks;
  error: string | null;
}

export interface Picks {
  theme: string | null;
  buttons: string | null;
  background: string | null;
}

const KEYS: Record<keyof Picks, string> = {
  theme: "frost-ext-theme",
  buttons: "frost-ext-buttons",
  background: "frost-ext-background",
};

function readPick(k: keyof Picks): string | null {
  try {
    const v = localStorage.getItem(KEYS[k]);
    return v && /^[a-z0-9.-]{3,80}\/[A-Za-z0-9_-]{1,40}$/.test(v) ? v : null;
  } catch {
    return null;
  }
}

function readPicks(): Picks {
  return { theme: readPick("theme"), buttons: readPick("buttons"), background: readPick("background") };
}

let snap: ExtSnapshot = {
  loaded: false,
  developerMode: false,
  installed: [],
  picks: readPicks(),
  error: null,
};
const listeners = new Set<() => void>();

function set(next: Partial<ExtSnapshot>) {
  snap = { ...snap, ...next };
  for (const l of listeners) l();
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function getSnapshot(): ExtSnapshot {
  return snap;
}

let inflight: Promise<void> | null = null;

/** Re-read what's installed. Safe to call often: concurrent calls share one request. */
export function refreshExtensions(): Promise<void> {
  inflight ??= extList()
    .then((r) => set({ loaded: true, developerMode: r.developerMode, installed: r.extensions, error: null }))
    .catch((e) => set({ loaded: true, error: String(e) }))
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

export function setPick(kind: keyof Picks, value: string | null): void {
  try {
    if (value) localStorage.setItem(KEYS[kind], value);
    else localStorage.removeItem(KEYS[kind]);
  } catch {
    // Storage blocked: the pick still applies for this session.
  }
  set({ picks: { ...snap.picks, [kind]: value } });
}

if (typeof window !== "undefined") {
  window.addEventListener("storage", (e) => {
    if (e.key && Object.values(KEYS).includes(e.key)) set({ picks: readPicks() });
  });
}

export function useExtensions(): ExtSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Enabled extensions only: what views, looks and the bridge may use. */
export function enabledExtensions(s: ExtSnapshot = snap): InstalledExtension[] {
  return s.installed.filter((e) => e.enabled && !e.suspended);
}

/** Split `<extId>/<itemId>`. */
export function parsePick(p: string | null): { ext: string; item: string } | null {
  if (!p) return null;
  const i = p.lastIndexOf("/");
  return i > 0 ? { ext: p.slice(0, i), item: p.slice(i + 1) } : null;
}
