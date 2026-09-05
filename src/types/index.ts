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
  /** Share anonymous usage counts (default true). */
  analyticsEnabled?: boolean;
  /** Launch on login (default true). */
  launchAtStartup?: boolean;
  /** Auto-run FrostMod when the app opens (default true). */
  autoRunFrostmod?: boolean;
  /** Extra command-line flags for `frostmod.exe`, exactly as typed. Empty by default. */
  frostmodArgs?: string;
  instantRefresh?: boolean;
  /**
   * Watch `<modsPath>/mods` and reload the game when tracks/bikes are added outside
   * MXB App (e.g. a manual download dropped into the folder). Default true.
   */
  watchModsReload?: boolean;
  /**
   * Inject `mxbsecure.dll` into the running game so locked content can be opened.
   *
   * **Off by default.** It reaches into a process the app usually didn't create, and the
   * DLL hasn't been proven on a real Windows run — a build that armed it for everyone with
   * locked content had the game dying on access violations seconds in, with quitting the
   * app from the tray as the only way out. With it on, launch the game from Play: the app
   * won't inject into a session it didn't start.
   */
  secureContentInject?: boolean;
  /** Intro slideshow already dismissed. Saved with the config (not in localStorage)
   *  so clearing the webview's storage doesn't replay the first-run flow. */
  welcomeSeen?: boolean;
  /** First-run guided tour already finished or skipped. */
  tourDone?: boolean;
  /** Register the global hotkey that summons the in-game overlay (default true). */
  overlayEnabled?: boolean;
  /** Overlay toggle combo in Tauri accelerator syntax, e.g. `"CommandOrControl+Shift+X"`. */
  overlayHotkey?: string;
  /** Which tyre pack the 3D previews fit a bike with. **Blank means the pack the bike's own
   *  `gfx.cfg` names**, which is what the game would fit. Substituting the name is all it
   *  takes to see a bike on another pack — nothing on disk is touched. */
  previewTyres?: string;
  /** Voice chat is off until turned on — a feature that opens a microphone shouldn't be
   *  something anyone discovers by accident. */
  voiceEnabled?: boolean;
  paintSyncEnabled?: boolean;
  mxbsecureEnabled?: boolean;
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
  /**
   * Who posted the mod, as the catalog names them. Null where the site didn't say.
   *
   * Optional rather than required because `ShopItem` extends this shape and a purchased
   * download has no byline to carry — the store's "All My Downloads" page lists files, not
   * authors.
   */
  author?: string | null;
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
  /** Who the catalog credits the mod to, from the byline on its page. `null` when the
   *  page carries none — same meaning as `ModSummary.author`. */
  author: string | null;
  /** The author's profile page on the catalog, for the byline link. */
  authorUrl: string | null;
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
  /** Unix milliseconds the files were last written — what "Recently added" sorts on. 0 when
   *  the filesystem wouldn't say. */
  modified: number;
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
  /**
   * Liveries assigned to this model, by base name (no `.pnt`). Empty means the model has
   * no opinion; a livery no model claims stays on offer under every model.
   */
  paints: string[];
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
  positions: Float32Array;
  /** `2 * vertexCount` — uv0 per vertex (empty if none). */
  uvs: Float32Array;
  /** `3 * vertexCount` — normals per vertex (empty if none). */
  normals: Float32Array;
  /** `3 * triangleCount` — u32 indices, a plain triangle list. */
  indices: Uint32Array;
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

/** A point in the bike's frame — the same one `EdfNode.positions` are in. */
export type Vec3 = [number, number, number];

/**
 * The joints an assembled bike can be posed about.
 *
 * A bike's `.geom` places its parts in the frame it was *authored* in and says nothing about
 * where the suspension rides at rest — there is no travel in the file, and ride height falls
 * out of physics the viewer doesn't run. So the viewer poses instead: the swingarm turns about
 * `pivot`, the fork slides along the axis `rake` tilts through `steerHead`, and the axles say
 * where the wheels ride so a stance can be solved for level.
 */
export interface BikeRig {
  /** Swingarm pivot. */
  pivot: Vec3;
  /** A point on the steering axis (the head itself). */
  steerHead: Vec3;
  /** Rake in degrees, tilting the steering axis back from vertical. */
  rake: number;
  /** Where the wheels ride. Null when the `.geom` names no axles. */
  frontAxle: Vec3 | null;
  rearAxle: Vec3 | null;
  /**
   * Where a rider sits, from the `.geom`'s `seat_height_ref`. Null when it names none.
   *
   * The bike's own statement of where its seat is — nothing in the mesh marks one — in the
   * same frame as the axles above, which is what lets the viewer stand a rider on it.
   */
  seat: Vec3 | null;
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
  /**
   * The tyres mod the wheels were drawn from, or `null` when the bike drew none.
   *
   * What was actually fitted, not what was asked for: a pick naming a pack that isn't
   * installed falls back to the bike's own, and the picker shows what's on screen.
   */
  tyres: string | null;
  /**
   * Whether the bike's `.geom` placed the parts into one frame.
   *
   * False means each node is still in its own local frame, so a vertex's position and normal
   * say nothing about where it sits on the bike. The Designer names a sheet region's flank and
   * facing from exactly those, and stays quiet rather than guessing when this is false.
   */
  assembled: boolean;
  /** The joints to pose about. Null for a bike that wasn't assembled. */
  rig: BikeRig | null;
}

/**
 * One bone of a rider's rig, as `rider.edf` stores it, already turned into the frame the
 * viewer draws the body in.
 */
export interface Bone {
  /** The rig's own name, e.g. `riderRIG_LeftElbow`. The game references these in `gfx.cfg`. */
  name: string;
  /** Index into the same array. Null for the root, and only for the root. */
  parent: number | null;
  /** Bone space → model space at rest. Row-major, translation in the fourth column. */
  bind: number[];
  /** Model space → bone space at rest. */
  invBind: number[];
  /** The slice of the mesh this bone covers, in bone space. */
  aabbLo: Vec3;
  aabbHi: Vec3;
}

/** Which bones move which vertices: four of each per vertex, in `nodes` order. */
export interface Skin {
  indices: number[];
  weights: number[];
}

export interface RiderPart {
  part: "body" | "helmet" | "boots" | "protection" | "suit" | "gloves";
  nodes: EdfNode[];
  textures: PaintTexture[];
  /** Only the body carries a rig; gear is rigid and hangs off a bone. */
  skeleton?: Bone[];
  skin?: Skin | null;
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

/**
 * A track's scenery — what stands on the ground the terrain grid describes.
 *
 * Positions are world metres in the game's own left-handed frame, the same one the terrain
 * grid is placed in, so the viewer mirrors X over both at once.
 */
export interface TrackScenery {
  /** `3 * vertexCount`, world metres. */
  positions: Float32Array;
  /** `3 * vertexCount`, unit length. */
  normals: Float32Array;
  /** `2 * vertexCount`. Tiling, so these run well outside 0–1. */
  uvs: Float32Array;
  /** `3 * triangleCount`. Sorted so each material's triangles sit together. */
  indices: Uint32Array;
  /** One run of triangles per material. */
  groups: TrackSceneryGroup[];
  /** The surfaces the map paints those runs with. */
  textures: TrackSceneryTexture[];
  /**
   * How many connected pieces the scenery comes apart into — one per tent, trailer or
   * foliage card. The unit a designer picks, hides or moves.
   */
  pieceCount: number;
  /** Which piece each triangle belongs to — turns a ray hit into a thing you can point at. */
  pieceOfTriangle: Uint32Array;
  /** World bounds, metres: `[minX, minY, minZ, maxX, maxY, maxZ]`. */
  bounds: [number, number, number, number, number, number];
}

export interface TrackSceneryGroup {
  material: number;
  triStart: number;
  triCount: number;
}

export interface TrackSceneryTexture {
  /** Which material this paints. */
  material: number;
  width: number;
  height: number;
  /**
   * An alpha cut-out — foliage, crowd, fencing. It has to be drawn with an alpha test:
   * without one every leaf card is an opaque rectangle, and a treeline becomes a wall.
   */
  alpha: boolean;
  /** `width * height * 4`, RGBA, first row first. */
  pixels: Uint8Array<ArrayBuffer>;
}

/** A track's sky, its backdrop, and the light it sits under. */
export interface TrackBackdrop {
  /** Direction the sun comes from, as the track states it. */
  sun: [number, number, number] | null;
  skyColour: [number, number, number] | null;
  sunColour: [number, number, number] | null;
  ambientColour: [number, number, number] | null;
  fogColour: [number, number, number] | null;
  fogDensity: number | null;
  /** The dome overhead, and the ring of land beyond the track. Either may be empty. */
  sky: TrackMeshArrays;
  backdrop: TrackMeshArrays;
}

/** Bare mesh arrays, in world metres. */
export interface TrackMeshArrays {
  positions: Float32Array;
  normals: Float32Array;
  uvs: Float32Array;
  indices: Uint32Array;
  /** The picture it carries. A sky dome is a few hundred triangles and one large image. */
  picture: { width: number; height: number; pixels: Uint8Array<ArrayBuffer> } | null;
}

/**
 * A tiling sheet of the track's own ground, and its relief.
 *
 * What the ground is made of, not what is where — tiled far finer than the third of a metre
 * a track states its surface at.
 */
export interface TrackGround {
  colour: TrackSceneryTexture;
  /** Its normal map, where the track ships one under a ground name. */
  normal: TrackSceneryTexture | null;
}

/** What a track pins to a point but ships no mesh for. Mirrors `scenery::Placement`. */
export interface TrackPlacement {
  /** A key, not prose — the UI translates it. */
  kind: "prop" | "marshal" | "camera" | "sound";
  /** The `.edf` for a prop, the `.wav` for a sound, otherwise the track's own name for it. */
  name: string;
  /** World metres, game frame. */
  pos: [number, number, number];
  /** Degrees, where the track states one. */
  heading: number | null;
  /** Full rotation in degrees, for props that carry one — what a `.scr` writes back. */
  rot?: [number, number, number] | null;
}

/** What the dropzone decided a dropped item is. Mirrors `dropzone::ContentKind`. */
export type DropKind =
  | "modsTree"
  | "track"
  | "bike"
  | "bikePaint"
  | "soundSet"
  | "riderGear"
  /** A tyre set (`mods/tyres`). Only ever comes out of a split pack — on its own a tyre
   *  package says too little to be recognised. */
  | "tyres"
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
  /** The pack it arrived in filed it here. */
  | "packLayout"
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

/** Where a download's bytes came from: the mod site, a shop purchase, or a local file the
 *  user imported or dragged in. */
export type DownloadSource = "site" | "shop" | "hub" | "file";

export type DownloadStatus = "installed" | "failed";

/** One finished download, as kept in `download-history.json`. */
export interface DownloadRecord {
  id: string;
  /** Unix milliseconds, stamped by the backend. */
  at: number;
  title: string;
  /** Empty for a dragged-in file — it has no mod page to go back to. */
  slug: string;
  subpath: string;
  destFolder: string;
  categoryId: number | null;
  source: DownloadSource;
  /** Which mirror served it — MediaFire, Google Drive, MEGA… */
  host: string | null;
  /** The link it came from, so a failed row can be retried in place. Null for shop and file. */
  url: string | null;
  bytes: number | null;
  status: DownloadStatus;
  error: string | null;
}

/** What a caller supplies; `id` and `at` are the backend's to assign. */
export type NewDownload = Omit<DownloadRecord, "id" | "at">;

/** Whether the mods tree still holds a mod the ledger knows about.
 *  `parked` is Manage having moved it aside — recoverable, and not a deletion. */
export type LedgerState = "present" | "parked" | "gone";

/** One mod the library has held, from the first time it was seen. Outlives the files:
 *  the snapshot fields are captured while the mod is installed and are all that remains
 *  once it is deleted. See `src-tauri/src/ledger.rs`. */
export interface LedgerEntry {
  /** Lowercased path relative to the MX Bikes root — the row's stable identity. */
  key: string;
  /** Last known path relative to that root, e.g. `mods/tracks/EU/RedBud.pkz`. */
  rel: string;
  name: string;
  category: LibraryCategory;
  folder: string;
  size: number;
  isDir: boolean;
  /** Unix milliseconds. */
  firstSeen: number;
  lastSeen: number;
  state: LedgerState;
  /** When it went, stamped once — null while it is still installed or parked. */
  goneAt: number | null;
  /** The mod's own declared name, often nothing like its filename. */
  title: string | null;
  author: string | null;
  location: string | null;
  /** Track length in metres. */
  length: number | null;
  /** Thumbnail filename on disk; use `thumbData` to render. */
  thumb: string | null;
  /** When the snapshot was taken, whether or not it found anything — plenty of archives
   *  carry no metadata, and those must not be re-read on every pass. */
  snapshotAt: number | null;
  /** Where the Trash put the files, when the app deleted them and could tell. Non-null is
   *  what makes Restore offerable; null means it went some other way. */
  trashedAt: string | null;
}

/** A ledger row on its way to the UI, with its thumbnail inflated for rendering. */
export interface LedgerRow extends LedgerEntry {
  /** `data:image/jpeg;base64,…`, or null when no snapshot was ever taken. */
  thumbData: string | null;
}

export type InstallStage =
  | "resolving"
  | "downloading"
  | "extracting"
  | "placing"
  /** The download held several mods and the user is picking which of them to install.
   *  Nothing is written yet, and the queue lane has already moved on. */
  | "review"
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

/** Whether FrostMod's DLL is actually inside the running game. Mirrors
 *  `frostmod::AttachState`.
 *
 *  `running` (the pill's usual source) only says the launcher process is up. These two
 *  answers come apart when the game runs at a higher integrity level than the app: the
 *  injector can't open a process above it, so FrostMod is running and simply never gets
 *  in — which used to look like the app lying about it. */
export type AttachState =
  | "game_not_running"
  | "attached"
  /** Up, not in yet, and still inside the grace period. Not a problem. */
  | "attaching"
  | "not_attached"
  /** Windows won't let us look inside the game — and won't let FrostMod in either. */
  | "blocked"
  | "unknown";

/** Mirrors `frostmod::Attachment`. */
export interface Attachment {
  state: AttachState;
  /** What is wrong and how to fix it. Empty unless the state calls for it. */
  reason: string;
}

/** The attach states worth putting in front of the user. */
export const ATTACH_PROBLEM: readonly AttachState[] = ["blocked", "not_attached"];

/** Result of pressing Play. `already_running` means we deliberately did nothing. */
export type LaunchOutcome = "launched" | "already_running";

export type LiveRefresh =
  | "refreshed"
  | "failed"
  | "game_not_running"
  | "disabled"
  | "unsupported"
  /** The running `mxbikes.exe` isn't a build the loader offset is known good for, so
   *  nothing was run in it. Reads to the player exactly like `failed` — the look didn't
   *  change — and both fall to the same "reselect your profile" note. */
  | "unknown_build";

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
  /**
   * Liveries that couldn't be moved into or out of `paints/`. MX Bikes holds bike files
   * open while it runs, so a swap or an assignment made mid-session can leave some
   * liveries where they were — and the filter is only as good as the move.
   */
  paints_stuck: number;
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
  /**
   * A loose `msvcr90.dll` beside the game exe that the app didn't remove on its own.
   *
   * `clear`/`removed` mean there's nothing to say. `foreign` and `locked` mean a file that
   * aborts MX Bikes with R6034 is still sitting there — see {@link StrayMsvcr90}.
   */
  strayMsvcr90: StrayMsvcr90;
  /**
   * What became of a `frostmod.dlo` hand-installed into the game's own `plugins` folder.
   *
   * `absent` is the normal case. Anything else means plugin mode is in play: the game loads
   * FrostMod itself at startup, with no `frostmod.exe` involved. Nothing used to update that
   * copy, so it outlived every release — one player was running a v0.12 plugin against a
   * v0.16.2 install, which hung the game before the loading screen.
   */
  gamePlugin: PluginCopy;
  /**
   * What became of the app's own `frostmod_session.dlo` in that same folder.
   *
   * This one the app installs, so `current` is the state everyone should be in. It is a
   * copy of the injected `frostmod.dll` under a name FrostMod reads as "publish the server
   * name and do nothing else" — no hooks, no overlay. It exists because the game only hands
   * the server name to a plugin it loaded itself, so without it the app cannot tell which
   * server anyone is on, and paint sync and voice have nothing to key on.
   */
  sessionPlugin: PluginCopy;
}

/**
 * The mods tree's standing with a cloud sync tool, as reported by the `mods-dehydrated`
 * event at the start of every game session. Matches `cloudfiles::Dehydrated`.
 *
 * Two different problems arrive on this one event. `count > 0` means bytes have actually
 * been evicted — the game can crash reading them off the load screen. `count === 0` with a
 * `provider` set means nothing is missing but the tree still sits behind a sync driver, and
 * the game's whole-tree read during loading is slow enough to look like a hang.
 */
export interface ModsDehydrated {
  /** Content files that are placeholders rather than real bytes. */
  count: number;
  /** How many were looked at, so `count` has a denominator. */
  scanned: number;
  /** A few names, to make the warning concrete. */
  examples: string[];
  /** The sync tool the tree sits under, e.g. `"OneDrive"`. Null if it doesn't sit under one. */
  provider: string | null;
}

/** State of a FrostMod plugin copy in the game's `plugins` folder. Matches `PluginCopy`. */
export type PluginCopy = "absent" | "current" | "refreshed" | "locked";

/**
 * A Visual C++ runtime the FrostMod chain needs. Matches `vcruntime::Runtime`.
 *
 * `vc140_x86` never appears in {@link FrostmodStatus.missingRuntimes} — nothing we ship is
 * 32-bit, so its absence proves nothing is wrong and it would only ever be a false alarm.
 * It exists for the repair, which installs the pair Microsoft's own downloads page hands
 * out, and for the manual download links.
 */
export type VcRuntime = "vc90" | "vc140" | "vc140_x86";

/**
 * What's left of a loose `msvcr90.dll` beside the game exe. Mirrors `vcruntime::Stray`.
 *
 * A loose VC9 CRT there kills the game with *"R6034 — An application has made an attempt to
 * load the C runtime library incorrectly"*, because the CRT refuses to initialise outside a
 * `Microsoft.VC90.CRT` activation context and a loose copy is never in one.
 *
 * - `clear` — nothing there. The normal case, and always the case off Windows.
 * - `removed` — there was one, it was this app's (0.9.2–0.10.0 planted them), it's gone.
 * - `foreign` — one matching no VC90 assembly on this PC. Someone else put it there, so the
 *   app won't delete it unasked; the player is shown it and offered the move.
 * - `locked` — ours, but something holds it open. That something is the game: closing it
 *   and pressing again is the fix.
 */
export type StrayMsvcr90 = "clear" | "removed" | "foreign" | "locked";

/** What a runtime install did. `cancelled` is the user dismissing the UAC prompt. */
export type RuntimeInstallOutcome = "installed" | "cancelled";

/**
 * What a repair run did. Mirrors `vcruntime::RepairReport`.
 *
 * Nothing here is an error: a repair does what it can and reports the rest, so
 * `stillMissing` is a list to hand download links for rather than a failure.
 */
export interface RuntimeRepairReport {
  /** Runtimes that went on during this run. */
  installed: VcRuntime[];
  /** Runtimes that were already there — "nothing to do" is a real answer worth saying. */
  alreadyPresent: VcRuntime[];
  /** Still absent afterwards: a declined UAC prompt, a failed download, a pending reboot. */
  stillMissing: VcRuntime[];
  /** What the sweep of the game folder found. `removed` is a repair that did something;
   *  `foreign`/`locked` is one that found the likeliest reason the game won't start and
   *  needs the player to say the word. */
  strayMsvcr90: StrayMsvcr90;
  /** False when no game folder is configured, so there was nowhere to look. */
  gameDirKnown: boolean;
}

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

/**
 * MXB Hub — `shop.mxb-hub.com`, the marketplace `mxbhub.com` redirects to.
 *
 * Deliberately expressed as extensions of the shop's types rather than as a parallel set. The
 * two stores sell the same kinds of thing to the same person, and the grid, the price tag, the
 * purchase card and the detail page are shared between them — types that merely resembled each
 * other would make every one of those a translation layer. What the Hub adds is a `slug` (its
 * API is addressable by one, where the shop's dump is not) and a creator link.
 */
export interface HubMod extends ShopMod {
  slug: string;
}

export interface HubModDetail extends ShopModDetail {
  slug: string;
  /** The store's own one-line summary — where a Hub listing says "in-game ready PKZ". */
  summary: string | null;
}

export interface HubCategory extends ShopCategory {
  /** The category's page on the store; also the creator's page, under `creators`. */
  link: string | null;
}

export interface HubPage {
  items: HubMod[];
  total: number;
  hasMore: boolean;
  currency: string;
}

/**
 * `relevance` is absent because the store rejects it — measured, not assumed. `onSale` is a
 * filter here rather than an order, which is what the Store API actually offers.
 */
export type HubSort = "newest" | "popular" | "priceAsc" | "priceDesc" | "nameAsc";

export type ShopSort =
  | "newest"
  | "recentlyUpdated"
  | "priceAsc"
  | "priceDesc"
  | "onSale"
  | "nameAsc";

/* ── Content lock ──────────────────────────────────────────────────────────────────── */

/** One file a locking run would produce, or the reason it will be left alone. */
export interface LockItem {
  /** Where the file lands under each GUID folder — relative to the parent of the
   *  selection it came from, so picking a folder keeps the folder. */
  rel: string;
  abs: string;
  bytes: number;
  /** A `.pkz` is locked as an archive; everything else as a single file. */
  kind: "archive" | "file";
  /** `null` when the file will be locked. */
  skip: "junk" | "empty" | "protected" | null;
}

export interface LockOutcome {
  guids: number;
  files: number;
  written: number;
  skipped: number;
  bytes: number;
  outDir: string;
}

export interface LockProgress {
  done: number;
  total: number;
  guid: string;
  file: string;
}
