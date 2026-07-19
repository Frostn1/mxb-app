export interface Config {
  modsPath: string;
  /** MX Bikes **install** dir (holds core `rider.pkz`) for the 3D rider body. */
  gamePath?: string;
  /** Hide to the tray on close and keep running (default true). */
  runInBackground?: boolean;
  /** Launch on login (default true). */
  launchAtStartup?: boolean;
  /** Auto-run FrostMod when the app opens (default true). */
  autoRunFrostmod?: boolean;
  instantRefresh?: boolean;
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

export type LibraryCategory =
  | "track"
  | "bike"
  | "bikePaint"
  | "bikeModelSwap"
  | "sound"
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

/** A material group over a node's kept triangles (for per-part texturing). */
export interface Submesh {
  /** Mesh-group name from the `.edf` (e.g. `frame.005`, `chain`). */
  name: string;
  /** Start triangle in the KEPT triangle list. */
  triStart: number;
  triCount: number;
  texture: string | null;
  uvTile: number | null;
}

/** One decoded mesh node from a bike's `.edf`, ready for a three.js geometry. */
export interface EdfNode {
  name: string;
  /** `3 * vertexCount` — positions (local space). */
  positions: number[];
  /** `2 * vertexCount` — uv0 per vertex (empty if none). */
  uvs: number[];
  /** `3 * vertexCount` — normals per vertex (empty if none). */
  normals: number[];
  /** `3 * triangleCount` — u32 indices, a plain triangle list. */
  indices: number[];
  /** Material groups over the kept triangle list (empty if not resolved). */
  submeshes: Submesh[];
  texture: string | null;
}

/** One texture decoded from a `.pnt` paint, ready for the 3D viewer. */
export interface PaintTexture {
  /** Internal texture name without extension (`livery`, `helmet`, `rider`…). */
  name: string;
  width: number;
  height: number;
  /** `data:image/png;base64,…` — bind straight into a three.js texture loader. */
  png: string;
}

/** One selectable paint (livery) for a bike: a name + its textures. */
export interface BikePaint {
  name: string;
  textures: PaintTexture[];
  changesPreview: boolean;
}

export interface BikeModel {
  nodes: EdfNode[];
  paints: BikePaint[];
}

export interface RiderPart {
  part: "body" | "helmet" | "boots" | "protection" | "suit" | "gloves";
  nodes: EdfNode[];
  textures: PaintTexture[];
}

/** The rider's real 3D preview, assembled from a loadout's installed gear + paints. */
export interface RiderModel {
  parts: RiderPart[];
}

export interface GearPaints {
  paints: string[];
  goggles: string[];
}

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

export type ReloadOutcome = "signaled" | "not_running" | "unsupported";

/** Emitted on `frostmod-reload` after a mod is placed. */
export interface FrostmodReload {
  slug: string;
  outcome: ReloadOutcome;
}

export type LiveRefresh =
  | "refreshed"
  | "failed"
  | "game_not_running"
  | "disabled"
  | "unsupported";

export interface PresetApplyOutcome {
  content_reload: ReloadOutcome;
  game_running: boolean;
  live_refresh: LiveRefresh;
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

export interface Loadout {
  paint: string;
  bikeFont: string;
  rider: string;
  helmet: string;
  helmetPaint: string;
  gogglesPaint: string;
  suitPaint: string;
  suitFont: string;
  boots: string;
  bootsPaint: string;
  glovesPaint: string;
  protection: string;
  protectionPaint: string;
  ridingStyle: string;
  tyres: string;
  raceNumber: string;
  modelSwap: string;
}

export interface BundleRef {
  /** Direct-download URL of the uploaded `.zip`. */
  url: string;
  /** Host label (e.g. `pixeldrain`), shown in the import dialog. */
  host: string;
  /** Bundle size in bytes. */
  size: number;
}

/** A saved, named, bike-agnostic preset (a loadout you can apply to any bike). */
export interface Preset {
  name: string;
  loadout: Loadout;
  /** Uploaded asset bundle, set only on a full-share code. */
  bundle?: BundleRef | null;
}

/** One asset a preset references, resolved to its source + `mods/` destination. */
export interface BundleAsset {
  slot: string;
  value: string;
  name: string;
  /** Destination path relative to `<MX Bikes>/mods`. */
  relDest: string;
  absPath: string;
  size: number;
  isDir: boolean;
}

/** A slot whose value can't be bundled (free-text font, stock, or not installed). */
export interface UnresolvedSlot {
  slot: string;
  value: string;
  reason: string;
}

/** Preview of what a preset's full bundle would carry. */
export interface BundlePlan {
  assets: BundleAsset[];
  unresolved: UnresolvedSlot[];
  totalSize: number;
}

/** Phases emitted on `preset-bundle-progress` while a bundle is created/imported. */
export type BundlePhase =
  | "bundling"
  | "uploading"
  | "downloading"
  | "installing"
  | "done";

/** Emitted on `preset-bundle-progress`. */
export interface BundleProgress {
  phase: BundlePhase;
  message?: string;
}

export type SlotSource =
  | "bikePaint" // liveries for the selected bike
  | "helmet" // helmet models
  | "helmetPaint" // paints for the selected helmet
  | "goggles" // goggles for the selected helmet (+ per-profile)
  | "boots" // boot models
  | "bootPaint" // paints for the selected boots
  | "outfit" // rider kit/suit paints (per rider profile)
  | "gloves" // glove paints
  | "protection" // protection models
  | "protectionPaint" // paints for the selected protection
  | "rider" // rider profile (default_mx / default_sm)
  | "ridingStyle" // mx / sm
  | "tyres" // tyre models
  | "font"; // number-plate / suit fonts (free text)
