/** A PiBoSo title the app can drive. Stable ids — they key `Config.games`. */
export type GameId = "mxb" | "gpb";

/** Features that only exist for some titles. Drives nav and settings gating. */
export interface GameCaps {
  /** FrostMod, the in-process mod loader. An MX Bikes plugin — no GP Bikes build. */
  frostmod: boolean;
  /** Re-run the profile loader in the live game after applying a preset. */
  instantRefresh: boolean;
  /** The 3D preview (Locker / Rider / bike preview). */
  viewer: boolean;
  /** The authenticated paid-content shop (mxbikes-shop.com). */
  shop: boolean;
  /** The Manage view (parking mods to trim what the game loads). */
  manage: boolean;
  /** Join a server by address (launches the game with `-directconnect`). */
  joinByAddress: boolean;
  /** The Servers tab and paint sync (MX Bikes dedicated servers, MX Bikes GUIDs). */
  servers: boolean;
}

/** One gear folder under `mods/rider` that new content installs into. */
export interface RiderArea {
  /** Folder name, e.g. `helmets`. */
  folder: string;
  /** Models here keep their liveries in `<model>/paints`. */
  paints: boolean;
  /** …and a `<model>/goggles` folder besides. */
  goggles: boolean;
}

/** One title the app can drive, as reported by `listGames()`. */
export interface GameInfo {
  id: GameId;
  /** Product name, shown verbatim — never translated. */
  display: string;
  /** The executable's file name, for when the UI has to name the file another tool
   *  should be pointed at (ReShade's installer asks for exactly this one). */
  exe: string;
  /** Top-level folders under `<modsPath>/mods` for this title. */
  modsDirs: string[];
  /** Host of this title's catalog, e.g. `mxb-mods.com`. Shown wherever the UI names
   *  the site it links out to. */
  catalogDomain: string;
  /** Gear areas under `mods/rider`, in the order to offer them. The two titles share
   *  almost nothing here — MX Bikes has boots and protection, GP Bikes bakes those into
   *  the rider model and has riding-style animations instead. */
  riderAreas: RiderArea[];
  /** Rider models the title ships, so a paint has somewhere to go on an empty mods
   *  folder. Empty for GP Bikes: every suit there is made for a downloaded model. */
  riderStockProfiles: string[];
  /** Folders inside `riders/<model>/` beyond `paints` — MX Bikes' `gloves`, `goggles`. */
  riderProfileExtras: string[];
  caps: GameCaps;
}

/** Saved folders for one title. */
export interface GamePaths {
  modsPath?: string;
  gamePath?: string;
  profilesPath?: string;
  reshadePath?: string;
}

export interface Config {
  /** Which title the app is driving. Absent on configs written before multi-game
   *  support, which the backend reads as `"mxb"`. */
  activeGame?: GameId;
  /** Saved folders per title. The active game's entry mirrors the flat fields below,
   *  which stay the source of truth for it. */
  games?: Partial<Record<GameId, GamePaths>>;
  modsPath: string;
  /** Active game's **install** dir (its executable + core archives). */
  gamePath?: string;
  /**
   * Override for the PiBoSo `profiles` folder. Empty (normal) means it lives at
   * `<modsPath>/profiles`; set only when profiles sit outside the game folder.
   */
  profilesPath?: string;
  /**
   * Override for the folder ReShade is installed in. Empty (normal) means the game's
   * install dir. Kept apart from `gamePath` on purpose: that one also drives `rider.pkz`
   * lookup, the game log folder and launching.
   */
  reshadePath?: string;
  /**
   * macOS: the Wine binary that starts the game. Empty (normal) means auto-detected.
   * Ignored on Windows and Linux.
   */
  wineRunner?: string;
  /** Hide to the tray on close and keep running (default true). */
  runInBackground?: boolean;
  /** Launch on login (default true). */
  launchAtStartup?: boolean;
  /** Auto-run FrostMod when the app opens (default true). */
  autoRunFrostmod?: boolean;
  instantRefresh?: boolean;
  /**
   * Watch `<modsPath>/mods` and reload the game when tracks/bikes are added outside
   * MXB App (e.g. a manual download dropped into the folder). Default true.
   */
  watchModsReload?: boolean;
  /** Intro slideshow already dismissed. Saved with the config (not in localStorage)
   *  so clearing the webview's storage doesn't replay the first-run flow. */
  welcomeSeen?: boolean;
  /** First-run guided tour already finished or skipped. */
  tourDone?: boolean;
  /** Register the global hotkey that summons the in-game overlay (default true). */
  overlayEnabled?: boolean;
  /** Overlay toggle combo in Tauri accelerator syntax, e.g. `"CommandOrControl+Shift+X"`. */
  overlayHotkey?: string;
  /** Voice chat is off until turned on — a feature that opens a microphone shouldn't be
   *  something anyone discovers by accident. */
  voiceEnabled?: boolean;
  /** Microphone to listen to. **Blank means "follow the system default"**, so a player
   *  who never picks one keeps tracking the device they change in Windows later. */
  voiceInputDevice?: string;
  /** Where other riders come out. Blank means the system default, as above. */
  voiceOutputDevice?: string;
  /** Mic-key combo in Tauri accelerator syntax. */
  voicePttHotkey?: string;
  /** Latch the mic (press to open, press to close) instead of holding the key.
   *  Off by default: push-to-talk can't leave a microphone open by accident. */
  voiceToggleToTalk?: boolean;
  /** Microphone gain, 1 = untouched. */
  voiceInputGain?: number;
  /** Playback volume for other riders, 0..1. */
  voiceOutputVolume?: number;
  /** App version whose release showcase has been seen. Blank on an install that
   *  predates the showcase, which is what marks it as an upgrade worth telling. */
  seenVersion?: string;
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

/** A mod's community score on mxb-mods.com, as shown under the site's own thumbnails. */
export interface ModRating {
  /** Mean score out of 5 — only meaningful when `count` is above zero. */
  average: number;
  count: number;
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
  /**
   * The post's category names ("2023 KTM 450 SX-F OEM", "Liveries", "KTM"). A livery is
   * filed under one category per bike it fits, which names its target far more precisely
   * than the title does — see `bikesFromCategories` in `api/mods`.
   */
  categories: string[];
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
  /** A riding-style animation from `mods/rider/animations`. Both titles. */
  | "animation"
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
  /** Whether the set has a mesh (any `.edf` — a bike may ship one per part). A set with
   * files but no mesh is incomplete and can't be applied; an empty set can (see `empty`). */
  valid: boolean;
  /** No files at all — an intentional "no model" swap that removes the current model. */
  empty: boolean;
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

/** A sound set the bike can be swapped between (active first). Mirrors `ModelVariant`. */
export interface SoundVariant {
  /** Variant name (folder name, or "Stock" for the built-in / no-sound default). */
  name: string;
  /** Whether this is the currently-active sound. */
  active: boolean;
  /** Whether the set has both must-files (`engine.scl` + `sfx.cfg`). A set with files
   * but missing one is incomplete and can't be applied; an empty set can (see `empty`). */
  valid: boolean;
  /** No sound files at all — the "Stock" set that reverts to the built-in engine sound. */
  empty: boolean;
  /** Number of sound files in the set. */
  fileCount: number;
}

/** A bike and every sound it can be swapped between, plus its model->sound bindings. */
export interface BikeSounds {
  /** Bike folder name under `mods/bikes`. */
  bike: string;
  /** The active sound's name ("Stock" if never swapped). */
  active: string;
  /** The bike's currently-active model swap, so bindings render relative to it. */
  activeModel: string;
  variants: SoundVariant[];
  /** model-swap variant name -> bound sound variant name. */
  bindings: Record<string, string>;
}

/** A model- or sound-set folder found loose inside a bike dir, not yet in its library. */
export interface LooseSwapCandidate {
  /** Variant name (the folder's own name) it would be registered under. */
  name: string;
  /** Path relative to the bike dir (`"Factory OEM"` or `"models/Factory OEM"`). */
  source: string;
  /** `"model"` → `FrostMod Models/`, `"sound"` → `FrostMod Sounds/`. */
  kind: "model" | "sound";
  /** Number of top-level files in the set. */
  fileCount: number;
}

/** A bike with one or more loose (unregistered) model / sound sets. */
export interface LooseSwapBike {
  bike: string;
  candidates: LooseSwapCandidate[];
}

/** Outcome of registering loose swaps (moving them into `FrostMod Models/`). */
export interface RegisterReport {
  /** Bikes that had at least one candidate. */
  bikes: number;
  /** Candidate folders successfully moved into the library. */
  registered: number;
  /** Candidates skipped (name already taken, or the move failed). */
  skipped: number;
  /** `FrostMod Models/` folders newly created on disk. */
  foldersCreated: number;
}

/**
 * A bike whose setup files (`.hrc`/`.cfg`/`.geom`) were carried into a swap folder by a
 * version that treated the whole bike folder as the model set. The game can't see such a
 * bike at all until they're restored.
 */
export interface OrphanedSetup {
  bike: string;
  /** Filenames missing from the bike root that a parked variant still holds. */
  files: string[];
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
  /**
   * Names the pixels held on the Rust side. Fetch them with {@link textureBytes} and build
   * a `THREE.DataTexture` — the RGBA never crosses as text, so paints cost no encode and
   * carrying the model's base textures on every paint costs no memory.
   */
  token: string;
}

/** One selectable paint (livery) for a bike: a name + its textures. */
export interface BikePaint {
  name: string;
  /**
   * The `.pnt` on disk, for a paint installed loose in the bike's `paints` folder — the
   * file the viewer watches so re-saving it re-dresses the model. `null` for a paint packed
   * inside the archive, which nothing rewrites in place.
   */
  path: string | null;
  textures: PaintTexture[];
  changesPreview: boolean;
}

export interface BikeModel {
  nodes: EdfNode[];
  paints: BikePaint[];
  /**
   * The model's own textures — the look it ships with, before any paint replaces one.
   *
   * The same pixels the paints already carry where they don't supply their own, listed
   * once on their own so "the model's look" can be asked for by name. A paint's `livery`
   * and the mesh's `livery` are the same field once they're folded together, and the
   * Designer's reference underlay is the one place that difference matters.
   */
  base: PaintTexture[];
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
  /** The mesh carries its own texture, so the preview can offer a "Stock" entry
   *  alongside the packed paints. Preview-only — never a loadout value. */
  hasStock: boolean;
  hasStockGoggles: boolean;
}

/**
 * One source image picked in the paint studio, as the backend read it.
 *
 * `width`/`height` are what will be packed; `sourceWidth`/`sourceHeight` are what the file
 * measures. They differ only when the image had to be resized onto power-of-two edges the
 * game accepts — `resized` says so, and the studio warns rather than silently reshaping
 * somebody's artwork.
 */
export interface StudioImage {
  path: string;
  /** The texture name it will be packed under — the file's stem, editable before saving. */
  name: string;
  width: number;
  height: number;
  sourceWidth: number;
  sourceHeight: number;
  resized: boolean;
  /**
   * Pixels held on the Rust side, same as any decoded paint's — but shrunk for display, so
   * they measure `previewWidth`×`previewHeight` rather than the sheet's own size.
   */
  token: string;
  previewWidth: number;
  previewHeight: number;
}

/** One texture of a paint being built: a file on disk, packed under `name`. */
export interface BuildTexture {
  path: string;
  name: string;
}

/**
 * Where a built paint is written: into the game's `mods` folder at `rel`
 * (`bikes/<Bike>/paints`, `rider/helmets/<Helmet>/paints`…), or into a folder the player
 * picked, for a paint they mean to share rather than install.
 */
export type PaintDest = { kind: "mods"; rel: string } | { kind: "folder"; path: string };

export interface SavedPaint {
  path: string;
  /** The texture names the file ended up carrying — what the mesh will bind. */
  textures: string[];
  bytes: number;
}

/** A paint taken apart into editable `.tga` sheets. */
export interface PaintTemplate {
  dir: string;
  files: string[];
  textures: string[];
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

/** One file inside a track. `role` is a key the UI translates, not prose. */
export interface TrackFile {
  name: string;
  role:
    | "heightfield"
    | "terrain"
    | "scenery"
    | "road"
    | "surfaces"
    | "config"
    | "model"
    | "image"
    | "sound"
    | "other";
}

/**
 * A track's metadata and contents. Deliberately cheap — the backend answers it from the
 * archive's index without inflating anything, so the track view can paint before the
 * terrain has been read.
 */
export interface TrackInfo {
  meta: PkzMeta;
  files: TrackFile[];
  /** Whether the track carries a heightfield at all, so the view can say so up front. */
  hasTerrain: boolean;
}

/** A track's terrain grid, unpacked from the binary IPC blob. */
/**
 * A picture of a track's surfaces, laid over the terrain.
 *
 * Built from the coverage masks in the track's own height file, so it describes exactly the
 * ground the grid does.
 */
export interface TrackOverview {
  width: number;
  height: number;
  /** `width * height * 4` bytes, RGBA, first row first. */
  pixels: Uint8Array<ArrayBuffer>;
}

export interface TrackTerrain {
  width: number;
  height: number;
  /** Metres of ground covered by one sample at this level of detail. */
  metresPerSample: number;
  /** Over the whole master grid, so the colour ramp doesn't shift between detail levels. */
  minHeight: number;
  maxHeight: number;
  /**
   * Whether the track stated its sample spacing. When it didn't, the relief is real but the
   * ground it is drawn across was assumed, so how steep the terrain looks is a guess.
   */
  scaleKnown: boolean;
  /** 0–1. How sure the backend's probe was that it read the height file correctly. */
  confidence: number;
  /**
   * Whether the heights are metres. A height file that doesn't say what its samples mean
   * leaves them as raw units, and the elevation range is then a number about nothing.
   */
  heightsInMetres: boolean;
  /** `width * height` heights in metres, row-major. */
  heights: Float32Array;
}

/** What the dropzone decided a dropped item is. Mirrors `dropzone::ContentKind`. */
export type DropKind =
  | "modsTree"
  | "track"
  | "bike"
  | "bikePaint"
  | "soundSet"
  | "riderGear"
  | "reshadePreset"
  | "unknown";

/** Why it decided that. A key, not prose — the UI translates it. */
export type DropReason =
  | "modsTree"
  | "categoryDirs"
  | "paintsBundle"
  | "soundMarkers"
  | "trackMarkers"
  | "trackPackage"
  | "bikeConfig"
  | "loosePaint"
  | "gearFolders"
  | "riderTexture"
  | "gearTexture"
  | "reshadePreset"
  | "unrecognised";

export interface DropChoice {
  /** Written straight into `destFolder`, e.g. `MX1OEM_2023_KTM_450/paints`. */
  value: string;
  /** A real bike or folder name — shown verbatim, never translated. */
  label: string;
  /** The category this destination lives under, so the UI never infers it from the path. */
  subpath: string;
}

export interface DropItem {
  id: string;
  name: string;
  kind: DropKind;
  reason: DropReason;
  /** `mods/<x>`. Empty while `needsChoice` and the user hasn't picked. */
  subpath: string;
  destFolder: string;
  /** The structural part of `destFolder`, which survives re-filing: a bike stays in its own
   *  folder, so choosing "MX2" means `MX2/<Bike>` rather than replacing it. */
  keepFolder: string;
  /** The content doesn't say where it belongs — the user must choose. */
  needsChoice: boolean;
  choices: DropChoice[];
  /** Existing files this would replace, relative to the mods folder. */
  collisions: string[];
  fileCount: number;
  bytes: number;
  /** Extra detail worth showing — a bike's real name and class, a track's name. */
  detail?: string;
}

export interface DropSkipped {
  name: string;
  reason: string;
}

export interface DropPlan {
  id: string;
  items: DropItem[];
  skipped: DropSkipped[];
  totalBytes: number;
}

export interface DropPreview {
  fileCount: number;
  bytes: number;
  collisions: string[];
}

export interface DropCommitItem {
  id: string;
  subpath: string;
  destFolder: string;
}

export interface DropInstalled {
  id: string;
  name: string;
  files: number;
  dest: string;
}

export interface DropFailed {
  id: string;
  name: string;
  error: string;
}

export interface DropOutcome {
  installed: DropInstalled[];
  failed: DropFailed[];
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
  /** Mods the folder watcher saw change, as `<type>/<name>`. Only the watcher sets
   *  this — an in-app install already knows what it placed. */
  mods?: string[];
}

/** Result of pressing Play. `already_running` means we deliberately did nothing. */
export type LaunchOutcome = "launched" | "already_running";

export type LiveRefresh =
  | "refreshed"
  | "failed"
  | "game_not_running"
  | "disabled"
  | "unsupported";

/** Result of a payload-carrying command sent to FrostMod (see `frostmod.rs`). */
export type CommandOutcome =
  | "signaled"
  | "not_running"
  | "write_failed"
  | "unsupported"
  /** Deliberately not sent — the installed FrostMod isn't one we'll give this verb to. */
  | "withheld";

export interface PresetApplyOutcome {
  content_reload: ReloadOutcome;
  game_running: boolean;
  live_refresh: LiveRefresh;
  /** Set only when the preset performed a model swap. See `SwapApplyOutcome`. */
  model_refresh: CommandOutcome | null;
}

/** One ReShade preset the app can switch to. Mirrors `reshade::Preset`. */
export interface ReshadePreset {
  name: string;
  path: string;
  active: boolean;
  /** In the app's `FrostMod ReShade` folder, so it can be deleted. A preset found loose in
   *  the game folder is someone else's file and is only ever read. */
  managed: boolean;
  /** Effects the preset asks for that aren't installed — it will render without them.
   *  Always empty when `hasShaders` is false; that case is reported once, on the status. */
  missingEffects: string[];
}

/** State of the ReShade install and its presets. Mirrors `reshade::Status`. */
export interface ReshadeStatus {
  /** The folder we looked in. Empty means none is configured — "don't know", not "absent". */
  gameDir: string;
  /** That folder is the player's `reshadePath` override rather than the game's install dir. */
  custom: boolean;
  /** A folder is configured and isn't there — distinct from never having picked one. */
  folderMissing: boolean;
  /** A ReShade `opengl32.dll` is in place. MX Bikes and GP Bikes are OpenGL. */
  installed: boolean;
  /** ReShade is here under a DirectX name these games never load — a fixable mistake. */
  wrongApi: string | null;
  version: string | null;
  hasShaders: boolean;
  /** Name of the active preset, empty when none resolves. */
  active: string;
  presets: ReshadePreset[];
}

/** Outcome of switching preset. Thin by nature — nothing of ours reloads. */
export interface ReshadeApplyOutcome {
  gameRunning: boolean;
}

/** Outcome of a Locker model/sound swap — same shape/feedback as a preset apply. */
export interface SwapApplyOutcome {
  content_reload: ReloadOutcome;
  game_running: boolean;
  live_refresh: LiveRefresh;
  /**
   * Model swaps only (`null` for sound). `live_refresh` re-runs the game's
   * *customization* loader — that reloads paints/gear but never the mesh, so the
   * model needs FrostMod to re-apply the bike. `signaled` means FrostMod was asked;
   * it still no-ops unless the swapped bike is the one currently selected in-game.
   */
  model_refresh: CommandOutcome | null;
}

/** Install/version/running snapshot for the FrostMod settings panel. */
export interface FrostmodStatus {
  /** `frostmod.exe` present in the app-managed folder. */
  installed: boolean;
  /** Installed release tag, if known. */
  version: string | null;
  /** Latest release tag on GitHub (null if the check failed / offline). */
  latest: string | null;
  /**
   * The binaries on disk aren't the ones the recorded version ships — an install
   * that didn't fully apply. Reinstalling is the fix.
   */
  needsRepair: boolean;
  /** FrostMod currently running (its reload event exists). */
  running: boolean;
  /**
   * The installed build is safe to run against the *active* game. False means
   * "installed, but too old for this title" — FrostMod v0.10.0 attaches to GP Bikes and
   * then reloads using MX Bikes' offsets, which crashes it. Updating is the fix, so offer
   * that rather than a start; the backend refuses to launch it either way.
   */
  supportedForGame: boolean;
  /**
   * Visual C++ runtimes this PC is short of. Empty is the normal case, and always the
   * case off Windows.
   *
   * `vc90` is what the *game* imports (`MSVCR90`) and `vc140` what `frostmod.dll` does.
   * Either being absent means FrostMod will most likely fail to attach with a bare
   * "…dll was not found" box over the game. Unlike the flags above this doesn't stop
   * FrostMod starting — it's a warning with a one-click fix attached.
   */
  missingRuntimes: VcRuntime[];
}

/** A Visual C++ runtime the FrostMod chain needs. Matches `vcruntime::Runtime`. */
export type VcRuntime = "vc90" | "vc140";

/** What a runtime install did. `cancelled` is the user dismissing the UAC prompt. */
export type RuntimeInstallOutcome = "installed" | "cancelled";

/** What an install landed, beyond succeeding. */
export interface FrostmodInstallReport {
  /** The release tag now on disk. */
  version: string;
  /**
   * The running game still has the previous FrostMod mapped in, so the new one
   * only takes over once MX Bikes is restarted.
   */
  needsGameRestart: boolean;
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
  /** Host label (e.g. `catbox`), shown in the import dialog. */
  host: string;
  /** Bundle size in bytes. */
  size: number;
  /**
   * Every slice of the bundle, in order, when it was too big for one upload. Absent means
   * the whole thing is at `url` — which is also the first slice.
   */
  parts?: string[];
}

/**
 * The content a preset needs installed, beyond the cosmetics in its loadout: the track
 * it's ridden on, plus packs the player pins as always-needed (the OEM pack).
 *
 * Paths are relative to the MX Bikes root, forward-slashed
 * (`mods/tracks/EU/RedBud.pkz`), so they survive the mods folder moving.
 */
export interface PresetContent {
  tracks: string[];
  keep: string[];
}

/** A saved, named, bike-agnostic preset (a loadout you can apply to any bike). */
export interface Preset {
  name: string;
  loadout: Loadout;
  /** Uploaded asset bundle, set only on a full-share code. */
  bundle?: BundleRef | null;
  /** Race content. Absent on presets that only ever dress a bike. */
  content?: PresetContent | null;
}

/** One mod as the Manage tab sees it — its identity is `rel`, enabled or not. */
export interface ModEntry {
  /** Path relative to the MX Bikes root, *as if enabled* (`mods/tracks/RedBud.pkz`). */
  rel: string;
  name: string;
  /** Library category (`track`, `bike`, `bikePaint`, `helmet`, …). */
  category: LibraryCategory;
  /** Folder inside its content type. Empty at the top level. */
  folder: string;
  size: number;
  enabled: boolean;
  /** An extracted track folder rather than a single archive. */
  isDir: boolean;
}

/** What racing a preset would move, worked out before anything does. */
export interface StatePlan {
  keep: ModEntry[];
  disable: ModEntry[];
  enable: ModEntry[];
  /** Slots the preset asks for that aren't installed. */
  unresolved: UnresolvedSlot[];
  gameRunning: boolean;
}

/** What a Manage operation actually did. `failed` is `[rel, reason]` per stuck file. */
export interface ModsStateOutcome {
  disabled: number;
  enabled: number;
  deleted: number;
  failed: [string, string][];
  content_reload: ReloadOutcome;
  game_running: boolean;
  /** Present only on a race apply, when cosmetics went in alongside the content. */
  look: PresetApplyOutcome | null;
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

/** Where a shared file goes back on the importer's machine. */
export interface ShareItem {
  name: string;
  /** Path under the mods root, forward-slashed (`tracks/EU/RedBud.pkz`). */
  rel: string;
  size: number;
  isDir: boolean;
}

/** A picked path that can't be shared, and why. */
export interface ShareSkipped {
  path: string;
  reason: string;
}

/** Preview of what sharing the current picks would carry — nothing is uploaded yet. */
export interface SharePlan {
  items: ShareItem[];
  skipped: ShareSkipped[];
  totalSize: number;
}

/** What a `MXBS1-` file-share code decodes to. */
export interface FileShare {
  items: ShareItem[];
  /** Size of the hosted zip — what an import downloads. */
  totalSize: number;
  bundle: BundleRef;
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
  | "ridingStyle" // stock mx / sm, plus installed `rider/animations` styles
  | "tyres" // tyre models
  | "font"; // number-plate / suit fonts (free text)

// ───────────────────────────── mxbikes-shop catalog ─────────────────────────────
//
// Mirrors the serde output of `src-tauri/src/mods/shop_catalog.rs`. Browse-only: there is
// deliberately no download URL here, because buying happens on the store's own site.
//
// Note these do NOT extend `ModSummary`. A shop item has no slug, no single category id and
// no post date, and `ShopItem` (above) already shows what forcing that shape costs.

/** What an item costs now, and what it costs normally. */
export interface ShopPrice {
  /** The normal price; the low end of the range when `hasRange`. */
  base: number | null;
  /** The high end of the normal range. Null when the item has a single option. */
  baseMax: number | null;
  /** The discounted price — only ever set when `onSale`. */
  sale: number | null;
  saleMax: number | null;
  /** A sale price exists *and* the clock is inside its window. */
  onSale: boolean;
  /** Several options (e.g. a paint, or a paint plus the PSD), so show a range. */
  hasRange: boolean;
  /**
   * The store gives this away. Distinct from a price of 0 — a pay-what-you-want item starts
   * at 0 without being free, and "$0.00" reads as broken where "Free" doesn't.
   */
  free: boolean;
  /** Whole percent off, rounded down. */
  discountPct: number | null;
  /** Unix seconds. Only set when the dump really carries a window — never invent one. */
  saleEnds: number | null;
}

export interface ShopMod {
  id: number;
  title: string;
  /** The product page. Null means the URL failed origin checks — hide the Buy button. */
  url: string | null;
  image: string | null;
  author: string | null;
  authorUrl: string | null;
  categoryIds: number[];
  categoryNames: string[];
  /** Unix seconds. */
  updated: number | null;
  price: ShopPrice;
}

export interface ShopModDetail extends ShopMod {
  /** Already sanitised in Rust. */
  descriptionHtml: string | null;
  images: string[];
}

export interface ShopCategory {
  id: number;
  name: string;
  slug: string;
  parent: number | null;
  /** 0 for top level, so the UI can indent without walking the tree. */
  depth: number;
  /** Items in this category and its descendants — what picking it actually selects. */
  count: number;
}

export interface ShopPage {
  items: ShopMod[];
  total: number;
  hasMore: boolean;
  currency: string;
  generatedTs: number | null;
  stale: boolean;
}

export interface ShopStatus {
  /** This build has a shop credential. False hides the Shop tab entirely. */
  available: boolean;
  count: number;
  currency: string;
  generatedTs: number | null;
  fetchedAt: number | null;
  stale: boolean;
  /** Old enough that the prices shouldn't be presented quietly. */
  veryStale: boolean;
  error: string | null;
}

export type ShopSort =
  | "newest"
  | "recentlyUpdated"
  | "priceAsc"
  | "priceDesc"
  | "onSale"
  | "nameAsc";
