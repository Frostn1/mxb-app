/**
 * Whether a mod the Storage page is about to remove can be fetched again later.
 *
 * The answer comes from the app's own download history: a mod that arrived from mxb-mods,
 * the Shop or MXB Hub has a place to go back to. It is matched to the installed file the
 * same way catalog cards are badged "Installed" (`installedMatch`), so the hint and the
 * badge can never disagree about which file a listing is.
 *
 * Other sources plug in through {@link registerRedownloadResolver}. The cdn.mxbsecure.com
 * mirror is the one planned; nothing here depends on it existing.
 */
import type { DownloadRecord } from "@frost/shared/types";
import { buildInstalledIndex } from "./installedMatch";

export type RedownloadKind = "site" | "shop" | "hub" | "mirror";

export interface RedownloadSource {
  kind: RedownloadKind;
  /** The catalog title it was downloaded as. */
  title: string;
  /** mxb-mods slug, for a site download. */
  slug?: string;
}

/** A further source, asked only about mods the history has no answer for. */
export type RedownloadResolver = (mod: { name: string; path: string }) => RedownloadSource | null;

const resolvers: RedownloadResolver[] = [];

/** Add a source (e.g. the mirror). Returns the function that removes it again. */
export function registerRedownloadResolver(resolver: RedownloadResolver): () => void {
  resolvers.push(resolver);
  return () => {
    const i = resolvers.indexOf(resolver);
    if (i >= 0) resolvers.splice(i, 1);
  };
}

function sourceOf(record: DownloadRecord): RedownloadSource | null {
  if (record.status !== "installed") return null;
  if (record.source === "site" && record.slug)
    return { kind: "site", title: record.title, slug: record.slug };
  if (record.source === "shop" || record.source === "hub")
    return { kind: record.source, title: record.title };
  return null; // a dragged-in file has nowhere to go back to
}

/** Re-download sources for the given mods, keyed by path. Mods with none are absent. */
export function redownloadSources<T extends { name: string; path: string }>(
  mods: readonly T[],
  history: readonly DownloadRecord[],
): Map<string, RedownloadSource> {
  const out = new Map<string, RedownloadSource>();
  const index = buildInstalledIndex(mods);
  // History is newest first, so the newest record for a mod wins.
  for (const record of history) {
    const src = sourceOf(record);
    if (!src) continue;
    const hit = index.match(record.title);
    if (hit && !out.has(hit.path)) out.set(hit.path, src);
  }
  for (const mod of mods) {
    if (out.has(mod.path)) continue;
    for (const resolve of resolvers) {
      const src = resolve(mod);
      if (src) {
        out.set(mod.path, src);
        break;
      }
    }
  }
  return out;
}
