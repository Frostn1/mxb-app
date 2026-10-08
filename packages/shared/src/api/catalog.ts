/**
 * The mxbsecure.com mod catalog (`control-plane/src/modapi.ts`): our mirror of mxb-mods.com
 * plus riders' own uploads, its files on cdn.mxbsecure.com.
 *
 * Two uses here. An `mxb://install?id=<uuid>` link from a mod's page on mxbsecure.com installs
 * that mod; and a download the app was about to take from an mxb-mods mirror (Drive, MediaFire…)
 * comes from our copy instead when the catalog holds one. Every call is public and cookie-free.
 */

import { MOD_TYPES, type ModType } from "./mods";

/** The control plane (`crates/core/src/names.rs` `CONTROL_PLANE`). */
export const CATALOG_API = "https://api.mxbsecure.com";

/** How a download from our mirror is named in the queue and the history. */
export const MIRROR_HOST = "mxbsecure.com";

/** The catalog's types (`control-plane/src/mirror.ts` `ASSET_TYPES`). */
export type CatalogType = "paints" | "bikes" | "liveries" | "kits" | "tracks" | "other";

export interface CatalogFile {
  idx: number;
  part: number;
  path: string | null;
  host: string;
  server: boolean;
  recommended: boolean;
  /** stored: on our CDN. original: not mirrored. mirroring: a copy is on its way.
   *  folder: a share whose files follow as parts. */
  state: "stored" | "original" | "mirroring" | "folder";
  filename: string | null;
  size: number | null;
  locked: boolean;
  /** Our download (a redirect to the CDN once stored, to the original before). */
  download: string | null;
  /** The original link. */
  source: string | null;
}

export interface CatalogMod {
  id: string;
  slug: string;
  source: "mirror" | "upload";
  title: string;
  type: CatalogType;
  /** The mxb-mods post, for a mirrored mod. */
  source_url: string | null;
  categories: string[];
  version_seq: number;
  files: CatalogFile[];
}

/** Where one file stands (`POST …/prepare/<idx>[/<part>]`). */
export type PrepareState =
  | { state: "stored"; download: string }
  | { state: "mirroring" }
  | { state: "folder" }
  | { state: "original"; source: string | null };

/** A download the installer can take: the same shape as a mirror on an mxb-mods page. */
export interface DownloadLink {
  url: string;
  host: string;
}

async function getJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(url, { credentials: "omit", ...init });
  if (!res.ok) {
    const message = await res
      .json()
      .then((b) => (b as { error?: string }).error)
      .catch(() => undefined);
    throw new Error(message ?? `Request failed (${res.status})`);
  }
  return (await res.json()) as T;
}

export function getCatalogMod(id: string): Promise<CatalogMod> {
  return getJson<CatalogMod>(`${CATALOG_API}/v1/assets/${encodeURIComponent(id)}`);
}

/** Our copy of an mxb-mods post, by its slug. `null` when the catalog doesn't have it. */
export async function getMirroredMod(slug: string): Promise<CatalogMod | null> {
  const res = await fetch(`${CATALOG_API}/v1/assets/mirror/${encodeURIComponent(slug)}`, { credentials: "omit" });
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`Request failed (${res.status})`);
  return (await res.json()) as CatalogMod;
}

/** Ask for a file: queues the copy when it isn't stored, and says where it stands. */
export function prepareCatalogFile(mod: Pick<CatalogMod, "id" | "version_seq">, file: Pick<CatalogFile, "idx" | "part">): Promise<PrepareState> {
  const path = file.part > 0 ? `${file.idx}/${file.part}` : `${file.idx}`;
  return getJson<PrepareState>(
    `${CATALOG_API}/v1/assets/${encodeURIComponent(mod.id)}/prepare/${path}?version=${mod.version_seq}`,
    { method: "POST" },
  );
}

/**
 * Which app mod type, and which browse category, a catalog type installs as.
 *
 * Liveries go to a bike's paints folder (the Liveries category is what turns that routing on).
 * Paints and "other" mix bike and rider pieces, so they are sorted by what the download holds,
 * like Bikelife. `null` for anything the active game's catalog has no type for.
 */
export function catalogInstallType(type: string): { modType: ModType; categoryId: number } | null {
  const by = (id: string, categoryId?: number) => {
    const modType = MOD_TYPES.find((m) => m.id === id);
    return modType ? { modType, categoryId: categoryId ?? modType.categoryId } : null;
  };
  switch (type) {
    case "tracks":
      return by("tracks");
    case "bikes":
      return by("bikes");
    case "liveries":
      return by("bikes", 37);
    case "kits":
      return by("rider", 35);
    case "paints":
    case "other":
      return by("bikelife");
    default:
      return null;
  }
}

/**
 * The file a one-click install takes: the recommended playable one, else the first playable,
 * else a server build when that is all there is. Folder parts are left to their part 0, the
 * share link, which the installer opens as a folder itself.
 */
export function pickCatalogFile(files: CatalogFile[]): CatalogFile | null {
  const options = files.filter((f) => f.part === 0 && (f.download || f.source));
  const playable = options.filter((f) => !f.server);
  return playable.find((f) => f.recommended) ?? playable[0] ?? options[0] ?? null;
}

/** The same link, ignoring a trailing slash and the scheme's case: how a mirror is matched to our row of it. */
export function sameLink(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  const norm = (s: string) => s.trim().replace(/\/+$/, "").replace(/^https?:\/\//i, "").toLowerCase();
  return norm(a) === norm(b);
}

/** How long an install link waits for a copy before taking the original instead. */
const WAIT_MS = 3 * 60_000;
const POLL_MS = 3_000;

/**
 * Where to download a catalog file from: our CDN when it is stored, otherwise the copy is
 * queued and, with `wait`, polled for until it lands. A file the mirror can't take (a host's
 * quota, too big, policy) falls back to the original link, as does running out of time.
 */
export async function catalogDownload(
  mod: Pick<CatalogMod, "id" | "version_seq">,
  file: CatalogFile,
  opts: { wait: boolean; sleep?: (ms: number) => Promise<void>; now?: () => number } = { wait: false },
): Promise<DownloadLink | null> {
  const original = file.source ? { url: file.source, host: file.host } : null;
  if (file.state === "stored" && file.download) return { url: file.download, host: MIRROR_HOST };
  // A folder share installs through its own link: the installer lists it.
  if (file.state === "folder") return original;
  const sleep = opts.sleep ?? ((ms: number) => new Promise<void>((r) => setTimeout(r, ms)));
  const now = opts.now ?? Date.now;
  const until = now() + WAIT_MS;
  for (;;) {
    let s: PrepareState;
    try {
      s = await prepareCatalogFile(mod, file);
    } catch {
      return original;
    }
    if (s.state === "stored") return { url: s.download, host: MIRROR_HOST };
    if (s.state !== "mirroring" || !opts.wait || now() >= until) {
      return s.state === "original" && s.source ? { url: s.source, host: file.host } : original;
    }
    await sleep(POLL_MS);
  }
}

/**
 * The download to take for a mirror picked off an mxb-mods page: our stored copy of the same
 * file when the catalog has one, otherwise the mirror as it was (and a copy is asked for, so
 * the next install gets ours). Only MX Bikes is mirrored. Never throws: the catalog being
 * unreachable just means the original host, as before.
 */
export async function preferMirror<T extends DownloadLink>(game: string, slug: string, mirror: T): Promise<T | DownloadLink> {
  if (game !== "mxb") return mirror;
  try {
    const mod = await getMirroredMod(slug);
    const file = mod?.files.find((f) => sameLink(f.source, mirror.url));
    if (!mod || !file) return mirror;
    if (file.state === "stored" && file.download) return { url: file.download, host: MIRROR_HOST };
    if (file.state === "original") void prepareCatalogFile(mod, file).catch(() => {});
    return mirror;
  } catch {
    return mirror;
  }
}
