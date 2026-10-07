import { invoke } from "@tauri-apps/api/core";

/** Where the mods folder's bytes go. `other` includes files no Library tab lists. */
export interface StorageOverview {
  total: number;
  tracks: number;
  bikes: number;
  paints: number;
  gear: number;
  other: number;
}

/** One installed mod, measured. */
export interface StorageMod {
  name: string;
  path: string;
  /** The Library tab's `installSubpath`; removal is scoped to it. */
  subpath: string;
  category: string;
  kind: string;
  size: number;
  modified: number;
  /** Protected content: listed, never removable here. */
  secured: boolean;
  /** Unix ms of the last MXB Coach recording on it, when there is one. */
  lastUsed: number | null;
}

export interface StorageScan {
  overview: StorageOverview;
  mods: StorageMod[];
  /** MXB Coach sessions "last used" was read from. 0 = no source, size only. */
  coachSessions: number;
}

export interface DuplicateGroup {
  id: string;
  size: number;
  /** Suggested copy to keep first. */
  items: StorageMod[];
}

export type LeftoverMatch = "hash" | "files" | "name";

export interface Leftover {
  path: string;
  name: string;
  size: number;
  modified: number;
  location: "downloads" | "cache";
  matchedBy: LeftoverMatch;
  matches: string[];
}

export interface RemoveReport {
  removed: string[];
  failed: [string, string][];
  bytes: number;
}

export function storageScan(): Promise<StorageScan> {
  return invoke<StorageScan>("storage_scan");
}

export function storageDuplicates(): Promise<DuplicateGroup[]> {
  return invoke<DuplicateGroup[]>("storage_duplicates");
}

export function storageLeftovers(): Promise<Leftover[]> {
  return invoke<Leftover[]>("storage_leftovers");
}

/** Recycle Bin, and only archives the last leftover scan reported. */
export function storageTrashArchives(paths: string[]): Promise<RemoveReport> {
  return invoke<RemoveReport>("storage_trash_archives", { paths });
}

/** The Library's uninstall path: Recycle Bin, scoped to the tab, noted in the ledger. */
export function storageRemoveMods(
  items: { path: string; subpath: string }[],
): Promise<RemoveReport> {
  return invoke<RemoveReport>("storage_remove_mods", { items });
}
