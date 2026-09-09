import { getPkzMeta, getPkzMetaCached } from "../api/mods";
import type { LibraryEntry, PkzMeta } from "../types";

/**
 * Shared store for mod metadata (name, author, thumbnail) read out of `.pkz` archives.
 *
 * Reading one means opening the archive and decoding its preview image, so a library
 * of a few hundred mods can't simply ask for all of them at once — that fans out into
 * hundreds of concurrent archive reads and image decodes, which is enough to bring a
 * machine to its knees rather than just making the app slow.
 *
 * Three things keep it bounded:
 *
 * 1. {@link primeMetaCache} pulls everything already cached on disk in a single round
 *    trip, so a library that has been opened before needs no archive reads at all.
 * 2. Callers request the remainder only when the card is actually on screen.
 * 3. {@link requestMeta} lets a few of those run at a time and shares in-flight work.
 */

const cache = new Map<string, PkzMeta>();
const inflight = new Map<string, Promise<PkzMeta | null>>();

/** Size is part of the key so replacing a mod in place invalidates its entry. */
export function metaKey(entry: Pick<LibraryEntry, "path" | "size">): string {
  return `${entry.path}:${entry.size}`;
}

/** Metadata already in hand, if any — never triggers a read. */
export function peekMeta(key: string): PkzMeta | undefined {
  return cache.get(key);
}

/**
 * Fill the store from the on-disk cache for a freshly scanned library, in one call.
 * Entries the backend hasn't inspected yet are simply left out. Never rejects — a
 * failure here just means the cards fall back to requesting metadata individually.
 */
export async function primeMetaCache(entries: LibraryEntry[]): Promise<void> {
  const wanted = entries.filter((e) => !cache.has(metaKey(e)));
  if (wanted.length === 0) return;
  try {
    const metas = await getPkzMetaCached(wanted.map((e) => e.path));
    metas.forEach((meta, i) => {
      if (meta) cache.set(metaKey(wanted[i]), meta);
    });
  } catch {
    /* fall back to per-card requests */
  }
}

/** How many archives we ask the backend to open at once. */
const MAX_PARALLEL = 3;

let active = 0;
const waiting: Array<() => void> = [];

function acquire(): Promise<void> {
  if (active < MAX_PARALLEL) {
    active += 1;
    return Promise.resolve();
  }
  return new Promise<void>((resolve) => {
    waiting.push(() => {
      active += 1;
      resolve();
    });
  });
}

function release(): void {
  active -= 1;
  waiting.shift()?.();
}

/**
 * Read a mod's metadata, queued behind whatever else is in flight. Resolves `null`
 * when the archive can't be read — the caller keeps its icon-and-size fallback.
 */
export function requestMeta(path: string, key: string): Promise<PkzMeta | null> {
  const cached = cache.get(key);
  if (cached) return Promise.resolve(cached);

  const pending = inflight.get(key);
  if (pending) return pending;

  const run = acquire()
    .then(() => getPkzMeta(path))
    .then((meta) => {
      cache.set(key, meta);
      return meta;
    })
    .catch(() => null)
    .finally(() => {
      release();
      inflight.delete(key);
    });

  inflight.set(key, run);
  return run;
}
