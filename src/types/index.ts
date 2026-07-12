// Shared types mirroring the Rust command structs (serde `rename_all = "camelCase"`).

export interface Config {
  modsPath: string;
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
  /** Host label shown on the page, e.g. "drive.google.com". */
  host: string;
  /** The "Default" file the author marks as the one to grab. */
  isDefault: boolean;
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

/** A single mod file on disk inside an installed folder. */
export interface InstalledMod {
  path: string;
  name: string;
}

/** A folder under `<MX Bikes>/<subpath>` containing mod files. */
export interface InstalledModFolder {
  path: string;
  name: string;
  mods: InstalledMod[];
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
