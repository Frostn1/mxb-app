// Shared types mirroring the Rust command structs (serde `rename_all = "camelCase"`).

export interface Config {
  modsPath: string;
  /** Hide to the tray on close and keep running (default true). */
  runInBackground?: boolean;
  /** Launch on login (default true). */
  launchAtStartup?: boolean;
  /** Auto-run FrostMod when the app opens (default true). */
  autoRunFrostmod?: boolean;
}

/** A track-mod as it appears in search results / browse grid. */
export interface ModSummary {
  id: number;
  slug: string;
  title: string;
  /** Canonical mxb-mods.com page URL. */
  link: string;
  /** ISO date string. */
  date: string;
  /** Featured image URL, if any. */
  image: string | null;
  categoryId: number;
}

/** One download choice on a mod page (hosts vary: Google Drive, MediaFire, …). */
export interface DownloadOption {
  url: string;
  /** Host label shown on the page, e.g. "drive.google.com" or "Media Fire". */
  host: string;
  /** The "Default" file the author marks as the one to grab. */
  isDefault: boolean;
  /** A dedicated-server build — not needed for normal play. */
  isServer: boolean;
  label: string;
}

/** Full detail for a single mod page. */
export interface ModDetail {
  id: number;
  slug: string;
  title: string;
  link: string;
  date: string;
  /** Rendered HTML description from the WP REST API. */
  descriptionHtml: string;
  images: string[];
  /** e.g. "Beta 19", when the page states it. */
  version: string | null;
  downloads: DownloadOption[];
}

/** An installed `.pkz` mod file found under the type's folder (at any depth). */
export interface InstalledMod {
  /** File name, e.g. `Mosctesting.pkz`. */
  name: string;
  /** Absolute path on disk. */
  path: string;
  /** Relative parent folder under the subpath (`""` if top-level). */
  folder: string;
  /** File size on disk, in bytes. */
  size: number;
}

/** How an installed item exists on disk. */
export type LibraryKind = "pkz" | "folder" | "loose";

/**
 * Type-specific classification the Library uses to group and label items.
 * (Kept as a loose union of known values; unknown strings fall back to "misc".)
 */
export type LibraryCategory =
  | "track"
  | "bike"
  | "bikePaint"
  | "bikeModelSwap"
  | "helmet"
  | "helmetPaint"
  | "goggles"
  | "boots"
  | "bootPaint"
  | "protection"
  | "protectionPaint"
  | "gloves"
  | "outfit"
  | "misc";

/**
 * A richer installed item than {@link InstalledMod}: also covers extracted mod
 * folders and loose paint files, tagged for grouping + detail in the Library.
 */
export interface LibraryEntry {
  name: string;
  path: string;
  folder: string;
  size: number;
  kind: LibraryKind;
  category: LibraryCategory;
  /** For paints / model-swaps: the owning bike / gear model / rider profile. */
  parent: string | null;
}

/**
 * One selectable model for a bike (Locker / model swap). The `active` one is the
 * bike's live loose file set; the rest are folders under `FrostMod Models/`.
 */
export interface ModelVariant {
  /** Variant name (folder name, or "Original" for the un-captured default). */
  name: string;
  /** Whether this is the currently-active model. */
  active: boolean;
  /** Whether the set has a `model.edf` (an invalid variant can't be applied). */
  valid: boolean;
  /** Number of top-level files in the set. */
  fileCount: number;
}

/** A bike and every model it can be swapped between (active first). */
export interface BikeModels {
  /** Bike folder name under `mods/bikes`. */
  bike: string;
  /** The active variant's name ("Original" if never swapped). */
  active: string;
  variants: ModelVariant[];
}

/**
 * Parsed structure of an installed `.pkz`, loaded lazily per library card.
 * `locked` marks a GUID-locked/encrypted archive that can't be inspected
 * (only its name + size are known).
 */
export interface PkzMeta {
  locked: boolean;
  /** Display name from the archive's `.ini`, if readable. */
  name: string | null;
  author: string | null;
  location: string | null;
  /** Track length in metres. */
  length: number | null;
  /** Reference altitude in metres. */
  altitude: number | null;
  /** Preview image as a `data:image/png;base64,…` URI, if one was found. */
  thumbnail: string | null;
}

export type InstallStage =
  | "resolving"
  | "downloading"
  | "extracting"
  | "placing"
  | "done"
  | "error";

/** Streamed over the `install-progress` Tauri event during Add to Library. */
export interface InstallProgress {
  slug: string;
  stage: InstallStage;
  /** Bytes received so far (downloading stage). */
  received?: number;
  /** Total bytes, when the server reports Content-Length. */
  total?: number;
  message?: string;
}

/**
 * Result of asking FrostMod (the in-game live-reload tool) to refresh:
 * - `signaled`    — FrostMod was running and reloaded the mods folder live.
 * - `not_running` — FrostMod isn't running; the mod loads on the game's next launch.
 * - `unsupported` — not a Windows build (dev only).
 */
export type ReloadOutcome = "signaled" | "not_running" | "unsupported";

/** Emitted on `frostmod-reload` after a mod is placed. */
export interface FrostmodReload {
  slug: string;
  outcome: ReloadOutcome;
}

/** Install/version/running snapshot for the FrostMod settings panel. */
export interface FrostmodStatus {
  /** `frostmod.exe` present in the app-managed folder. */
  installed: boolean;
  /** Installed release tag, if known. */
  version: string | null;
  /** Latest release tag on GitHub (null if the check failed / offline). */
  latest: string | null;
  /** FrostMod currently running (its reload event exists). */
  running: boolean;
}
