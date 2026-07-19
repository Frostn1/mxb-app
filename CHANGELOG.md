# Changelog

## 2026-07-18 — v0.2.1

### Fixed
- **Helmet sits better on the rider model** — slightly smaller helmet scale so it
  proportions correctly against the body.

## 2026-07-18 — v0.2.0 — per-part bike textures, rider gear preview, library 3D quick-view

Highlights of this release (full detail in the dated entries below):
- **Per-part bike textures in the 3D viewer** — each mesh part binds its own map
  (metals, plastics, number plate, exhaust) via the model's material index, instead
  of one texture smeared over the whole bike.
- **Rider gear preview** — helmets, boots and goggles (and their paints) now render
  on the rider model, including paints from extracted (loose-folder) gear.
- **Library 3D quick-view** — a one-click 3D preview button on library items.
- **Rider / Presets reorg** — presets no longer embed a 3D preview; preview a build
  from the Rider tab instead.

## 2026-07-18 — internal cleanup

### Changed
- Trimmed verbose source comments across the codebase, keeping only short notes
  that clarify non-obvious parameters, byte offsets and invariants.

## 2026-07-18 — rider gear paints from extracted folders + goggles

### Fixed
- **Extracted (loose-folder) gear now shows its paints instead of rendering grey.**
  The paint-name match only accepted `.pkz`-style internal paths (`/paints/…`), so a
  paint in an *extracted* gear folder (`paints/…`, no leading slash) was skipped.
- **Folder helmets now load their goggle paints.** The gear-folder reader only scanned
  `paints/`; it now also reads `goggles/`, and the loadout's goggle-paint choice is
  threaded through `load_gear` so the selected goggles render.

## 2026-07-18 — helmet/boot browse shows models only

### Fixed
- **Rider browse "Helmets" and "Boots" now list only models, not paints.** The chips
  pointed at the parent categories (Helmets 33 / Boots 31), which also aggregate
  paints, goggles and addons — so models and paints were mixed. They now query the
  dedicated model subcategories (Helmet Models 313 / Boot Models 343).

## 2026-07-18 — per-part bike textures, library 3D quick-view, Rider/Presets

### Added
- **Quick 3D-view button on library items.** A 3D-cube icon on each bike / gear /
  paint card opens the 3D viewer directly (shared eligibility logic with the detail
  view's "View in 3D").
- **Save a rider look by name in the Rider tab**, plus a **"View in Rider"** button in
  the Presets builder (and on saved cards) that opens the look on the player model.

### Changed
- **Bike parts now bind their real texture from the model's per-submesh material
  index** (the `.edf` `block-4` field), so each part gets its correct map — metals,
  plastics, number plate, exhaust — instead of the largest texture smeared over
  everything. Validated across Honda/KTM/Yamaha/Suzuki/TM. Number plates stay on the
  `gfx.cfg` override; a part whose index can't be resolved renders **neutral grey**
  rather than the wrong texture.
- **Presets no longer embeds a 3D preview** — preview a build via the Rider tab.
- **Refined rider boot seating on the player model** — larger boots, seated higher off
  the leg-bottom and nudged forward, with a wider outward stance so they sit naturally
  under the legs.

### Fixed
- **Heavy paints render again.** `.pnt` paint textures are downscaled to 1024² for the
  preview (they were shipped at full 4096²), so multi-map paints no longer blow the
  webview's WebGL memory budget and fail to show.

## 2026-07-18 — viewer: boots preview orientation & framing

### Fixed
- **Boots preview now renders correctly.** Four separate defects in the gear
  preview's boot handling:
  - *Upside down.* Boots share the gear frame but their worn-up axis is the
    **opposite** of the helmet's — after `to_right_handed` negates gear X, a boot's
    leg-opening sits at ≈-0.07 and its sole at ≈-0.50 (measured from the real Fox
    Instinct mesh), so "up" is +X, the reverse of the helmet's -X crown. Boots now
    take a dedicated `BOOT_ROT` (+90° roll) instead of the helmet's `GEAR_ROT` (-90°).
  - *Boots overlapping ("smooshed").* A boots `.edf` ships both feet as separate
    nodes (`boot_l`/`boot_r`) authored coincident at the ankle; they were rendered
    stacked. New `bootSides` splits them onto left/right feet (on-body), and the
    Library solo preview separates the pair by half a boot width.
  - *Toe-in splay.* Each foot now yaws so its heel→toe axis points straight forward
    (`straightenYaw`, from the front/back 20% of the mesh), cancelling the mould's
    built-in angle.
  - *Framing.* The solo boots pair is recentred on the origin explicitly (the
    up-righted bbox sits well below its own origin), so the camera frames it instead
    of leaving it under view.

## 2026-07-18 — viewer: un-mirror gear/rider

### Fixed
- **Rider gear/helmet artwork no longer renders mirrored.** MX Bikes is a DirectX
  (left-handed) engine; bikes were already converted to three.js' right-handed frame
  (`to_right_handed`) but gear/rider meshes deliberately were not — on the assumption
  a mirror is "invisible on a helmet shell." It isn't: decal text ("Red Bull",
  "Oakley", "Troy Lee Designs") read backwards on every gear part. Gear/rider now goes
  through `to_right_handed` too, and `GEAR_ROT` flips to a −90° roll to keep the helmet
  upright under the flipped up-axis (front-back and the left-right→−X mapping are
  unchanged, so the boot mirror and seating anchors still hold). Removed the now-unused
  `orient_windings_nodes`/`orient_windings` lighting-only path.

## 2026-07-17 — viewer: preview gear paints on the stock model

### Added
- **Loose gear paints (boots / helmet / protection) preview on the game's stock
  model** (`load_stock_gear_model`). A boot paint installs as a bare `.pnt` with no
  model — the boot *model* is stock, in `rider.pkz` — so previewing one now loads
  the stock boots mesh and applies the paint, the same way a rider-outfit paint
  renders on the stock body. Wired for the `bootPaint`/`helmetPaint`/`protectionPaint`
  Library categories; outfit/glove paints keep the rider-body preview.
- A caption notes when a paint is shown on the stock model, since a `.pnt` painted
  for a *different* model (its texture name won't match the stock one) is
  force-applied and may not line up — e.g. the installed `Purple White Alpinestar
  Boots.pnt` (texture `aboots`) and `RDS Leopard GBootz W.pnt` are for boot models
  not installed, so they render on the stock boots but with mismatched UVs. Stock
  boots + a stock-named paint line up perfectly.

### Fixed
- **Rider gear LODs no longer render stacked.** Gear packs its LODs as repeated node
  names in one `.edf` (the stock boots ship `boot_l`/`boot_r` three times);
  `keep_lod0` keeps only the highest-detail node of each name wherever rider-side
  meshes are decoded.

## 2026-07-17 — browse: separate boot/protection paints from models

### Added
- **Boot Paints and Protection Paints browse filters.** mxb-mods splits each gear
  type into a model category and a paints child (Boots 31 / Boot Paints 126,
  Protection 36 / Protection Paints 135) — the same split the site's search uses.
  The Rider tab already surfaced Helmet Paints; it now surfaces all three, so a
  paint can be found without wading through models.

### Changed
- **Gear paints install onto the right model kind.** When a paint comes from a
  known paints category (`riderPaintKind`), the install destination is biased to
  that kind's installed models only — a boot paint targets a boot model (and
  falls back to the sole installed boot model, the "just installed a new model"
  case), never a helmet/protection folder. The shared per-type remembered folder
  is also ignored when it belongs to a different gear kind.

### Added
- **Goggles are textured and paint-selectable.** A helmet's `.edf` carries a
  separate `goggles` submesh, and the mod ships a `goggles/` paint folder (its own
  texture, e.g. `TLDSE4goggle`) distinct from the shell's `paints/` (`TLDSE4`). The
  viewer now binds each submesh to its own paint and adds a **Goggles** dropdown
  next to the Paint one, so the lens/strap colour can be picked independently. Six
  goggle paints + fourteen helmet paints listed for the TLD SE4, verified.

### Fixed
- **Goggles wore the helmet skin.** Gear was drawn with one material per `.edf`
  node, so the goggles submesh sampled the helmet atlas at its own UVs and rendered
  dark/garbled. Gear now builds **per-submesh** materials (the path bikes already
  use), binding `goggles`→goggle texture and everything else→the shell texture. The
  binding is by submesh name, so it holds across mods without hard-coded texture
  names.

## 2026-07-17 — viewer: the model is left-handed (mirrored bike + inside-out lighting)

### Fixed
- **`.edf` models are authored left-handed (DirectX); three.js is right-handed**
  (`edf.rs::to_right_handed`, applied in `main.rs` after `assemble_bike`). Feeding
  those coordinates straight in mirrored the model, which caused two bugs that
  looked unrelated:
  - **Mirrored artwork** — "HONDA" on the seat and "CRF"/"450R" on the shrouds
    rendered back-to-front (the reported "seat is flipped").
  - **"Holes" / dark facets** — a mirror inverts triangle orientation, so against
    the model's own normals **100.0%** of the Honda's `chassis`/`fsusp` triangles
    read as back-facing. Backface culling was never involved (the viewer renders
    `DoubleSide`, so nothing ever vanished): `DoubleSide` lighting does
    `normal *= gl_FrontFacing ? 1 : -1`, so every normal was negated and the whole
    bike was lit from the inside. The geometry was complete the entire time.

  Negating X on positions + normals fixes both at once, and the winding then agrees
  with the normals with no re-winding. Applied *after* assembly deliberately — the
  `.geom` mounts and rake rotations are authored in the game's frame, and mirroring
  X inverts a rotation about X. Proven by software-rendering the real Honda: the
  text reads correctly and the black facets resolve into solid red bodywork.
- **Rider/gear lighting** (`edf.rs::orient_windings_nodes`). Gear shares the same
  left-handed convention (boots 100.0%, TLD SE4 helmet 99.9% back-facing) and was
  lit inside-out too, but it's authored X-up and the viewer bbox-fits it with
  anchors tuned to the un-converted frame — so its winding is corrected for
  lighting while its geometry is left alone.
- Confirmed **`flipY = false` is correct** and left unchanged. Both `.pnt` and
  embedded `model.edf` textures are stored bottom-row-first, and `flipY = true`
  drags the atlas's engine-metals region onto the bodywork. The mirrored text came
  from the mesh, not the texture.

### Added
- **Paints that don't fit the model are now labelled** instead of silently doing
  nothing (`BikePaint.appliesToModel`; dropdown shows "— not for this model" plus a
  note on the canvas). A `.pnt` replaces a model texture *by name*, which is the
  whole mechanism: the Honda binds `2021crf`/`w_plate`/`chain`, so its own
  `stock.pnt` (ships `chain`, `wheel`…) applies, while `#96_CR450F'26.2HRC_TRD.pnt`
  ships only `plastics`/`plastics_n` — it's painted for a '26 HRC model-swap body
  kit that isn't installed, binds nothing, and the game wouldn't apply it either.

## 2026-07-17 — viewer: `.edf` indices start at `ic+4` (the root of every mesh artifact)

### Fixed
- **`.edf` index buffers are read from `ic+4`, not `ic+8`** (`edf.rs`). There is no
  flag word after `tri_count`: the zero read there is `idx0`, which is 0 because
  every node's first triangle is `(0,1,2)`. The off-by-one validated itself —
  skipping `idx0` at the front and eating the trailing `submesh_count` at the back
  landed the name anchor exactly — so blocks "checked out" while every triangle was
  built from a shifted index window. This was the single root cause behind the
  shattered/faceted gear, the streaks, the holes and the half-rendered rider.
  Proof: the decode now yields exactly `tri_count` triangles with zero degenerates
  (stock helmet 4120/4120, TLD SE4 6318/6318, boots 1950, armour 2922), and the
  rider body renders as a clean, complete figure.

### Removed
- **The strip decoder, the degenerate-ratio heuristic and the UV-span streak
  filter** — all three existed only to compensate for the bad offset. There is no
  strip encoding in this format; bikes, gear and the rider body are all plain
  triangle lists. `parse_rider` is gone (callers use `parse`), and the UV-span rule
  is gone with it — it was deleting real geometry.

## 2026-07-16 — viewer: read the bike's configs instead of guessing

### Added
- **`gfx.cfg` + `.hrc` are now loaded** from a bike (`cfg.rs`). They ship as plain
  text inside the `.pkz` and state outright what the viewer used to infer: which
  node is a part's full-detail mesh, and which mesh group binds to which texture.
- **Every texture packed in a `model.edf` is extracted under the model's own name**
  (`2021crf`, `exhaust_22`, `w_plate`; `plastics`, `450f_metals` on the KTM) instead
  of keeping only the largest and renaming it `albedo`. Those names are the whole
  binding mechanism, and collapsing them threw them away.

### Changed
- **LOD selection is `.hrc`-driven.** A `.hrc` names `level0` and its LOD variants
  outright, so the old heuristic (strip a `b`/`c` before the first digit, tiebreak on
  triangle count — which once silently flipped the KTM 450 onto its un-placeable
  LOD-B chassis) is now only a fallback for a loose `.edf` with no configs.
- **Texture binding moved to Rust and is driven by the bike's files**, not by regex
  over mesh-group names in the viewer. `gfx.cfg`'s `texture = …` overrides win
  (`plate → w_plate`, `chain → chain`); everything else takes the model's primary
  diffuse; a paint replaces a model texture of the **same name**. The viewer now just
  looks the resolved name up.
- Bike textures use `RepeatWrapping` — the number plates' UV islands run outside
  0–1, and the Honda's exhaust is authored on UV tile 1.

### Fixed
- **Bike paints smeared across the whole bike** (rider number in the wrong place, the
  paint map dragged over the rear fender). The viewer forced *every* mesh group to
  the paint's `plastics`, so a paint authored for a different model was stretched
  over parts it was never drawn for — engine, forks, swingarm and all. A paint only
  applies where the model names a texture the paint carries; a stock Honda's body
  texture is `2021crf`, so its `'26 HRC` paints (drawn for a swapped model) correctly
  leave it alone now, rather than being smeared over it.
- **The exhaust wore body graphics.** The Honda's exhaust is authored wholly on UV
  tile 1 (u ∈ [1.001, 2.000]), which selects a second texture (`exhaust_22`) sampled
  at `u - 1`. Verified by rendering: it now reads as brushed metal.

## 2026-07-16 — v0.1.6

### Fixed
- **Gear mods rendered as shattered polygons**: a mesh's index buffer is either a
  stitched triangle *strip* or a plain triangle *list*, and content exports both —
  PiBoSo's own gear/rider are strips (~50% of their triples are degenerate
  stitches), while e.g. the `TLD SE4` helmet mod is a list (6%). Decoding a list as
  a strip invents ~3.7 triangles per vertex of garbage (a closed mesh is ~2), which
  is exactly the shattered surface and streaks. Each node now picks from its own
  degenerate ratio instead of being told by the caller.
- **Gear rendered lying on its face**: helmets/boots/protection are authored
  **X-up** (a helmet extends up from an origin at the neck), not Z-up like bikes or
  Y-up like the rider body — so they need a roll about Z, not the bike's X flip.
  Verified by rendering the mesh down each axis.
- **Gear previewed at a top-down angle**: the viewer's camera sits high to look over
  a bike; a single gear item is small and centred, so it now gets a level view.
  (The move also has to go through OrbitControls, which owns the camera and was
  silently reverting it.)
- **Packaged `.pkz` mods extracted as empty files**: an archive written *streaming*
  leaves its per-entry sizes zero in the local file header (they live only in the
  central directory), so every entry came out 0 bytes — a packaged gear mod looked
  like it simply had no model. Sizes are now read from the central directory, with
  the local header as fallback. Verified on a real 30 MB helmet mod (`helmet.edf`
  0 → 9.1 MB) with no change to OEM bike/track archives.
- **Installed gear mods never rendered**: the gear loader only looked for an
  *extracted folder*, but gear installs as a packaged `.pkz` — and the Library only
  passed bikes to the 3D viewer, so opening a helmet showed the rider body wearing
  that helmet's paint. Gear now loads from a folder **or** a `.pkz`, and a
  helmet/boots/protection item opens as a standalone 3D preview of that piece.
- **Paint colours were channel-swapped everywhere**: `.pnt` pixels are stored
  **RGBA**, but the decoder swapped them as if they were BGRA — turning every
  paint's navy into brown and red into blue, on bike liveries as well as rider
  kits. Proven against PiBoSo's own stock `white_navy.pnt`, whose navy only reads
  as navy unswapped. Pixels are now returned verbatim; added an `#[ignore]`
  real-file guard (`stock_white_navy_decodes_navy_not_brown`) so it can't regress.
  (The old `libpnt` fixture uses the opposite order to the real game files, so it's
  no longer treated as ground truth for channel order.)
- **Rider body rendered as a shredded half-mesh**: skinned models store their
  indices as stitched triangle **strips**, not lists — reading them as a list
  recovered only ~1/3 of the surface, wrongly grouped. New `edf::parse_rider`
  strip-decodes them (rider body: 2,840 → 11,701 triangles, a solid figure), while
  bikes/gear keep the list decode (`edf::parse`) their non-submesh parts need.
- **Rider paint mapped as noise**: the `.edf` UV block is a single 2-float set
  (**stride 8**), not two sets at stride 16 — the old read sampled every *other*
  vertex's UV. Per-triangle UV span dropped 0.44 → 0.053.
- **Paints were mapped upside-down**: MX Bikes is a DirectX game, so its textures
  use a **top-left UV origin**; three.js's default `flipY` mirrored them, making
  the torso sample the pants region ("the rotation is off"). Now `flipY = false`.
  Proven on stock `white_navy.pnt`: flipped it renders an asymmetric kit (one leg
  yellow, one navy), unflipped a correct symmetric one. Affects bike liveries too.
- **Streaky "lines" across the rider**: strip-transition triangles whose vertices
  straddle different UV islands smear the atlas across the model. They're often
  short in 3D, so the existing long-and-thin sliver test missed them; they're now
  dropped by a UV-span test (a real triangle covers ~0.03 of the sheet, these span
  0.85–1.0). Strip decode only, so bikes are untouched.
- **Rider preview was slow**: the body was re-read from the 105 MB `rider.pkz` and
  re-parsed (28 MB `.edf`) on every open; now cached per profile.

### Added
- **Full-bundle preset sharing ("they have nothing" import)**: a preset can now be
  shared as a complete asset bundle, not just a config code. **Create full bundle**
  in the Share dialog resolves every asset the loadout references (liveries, gear
  models + paints, gloves, outfit, tyres, model-swap variants) via `scan_library`,
  zips them into a `mods/`-mirrored tree plus `preset.json`, uploads it to an
  anonymous host (pixeldrain), and returns a share code with the download link
  embedded. **Full import** on the other end downloads the bundle (reusing the
  Google Drive / MediaFire / Mega / direct download + `place_mod` pipeline) and
  installs every file into the correct `mods/` subfolder — so a recipient who owns
  none of the mods still gets the complete look. New Rust modules `bundle.rs`
  (resolve/build/import) and `upload.rs`; `preset_bundle_stats` /
  `preset_bundle_create` / `preset_bundle_import` commands. Backward-compatible: an
  optional `bundle` field on `Preset`, so legacy `MXBP1-` codes still decode. The
  Share dialog previews bundle size + which slots can't travel and notes the link
  is public/temporary; free-text fonts and stock/uninstalled slots aren't bundled.
- **3D preview in Presets (bike + rider)**: a live 3D panel renders the current
  loadout — the real bike model decoded from its `.edf`/`.pkz`, its livery, and the
  rider's installed gear (helmet/boots/protection meshes + suit/gloves paints) — and
  updates as you change slots. Native decoders (`edf`, `paint`) mean no external
  tools. Optional **game install folder** setting (Settings → MX Bikes folder) points
  at the MX Bikes install so the real rider **body** (`rider.pkz`) can load.

### Changed
- Heavy backend commands (model/paint/library loads, `.pkz` decode) now run **off
  the UI thread** (`async` + `spawn_blocking`), so opening the viewer or library no
  longer freezes the window and a malformed asset returns an error instead of
  crashing.

### Fixed
- **More paints render in the 3D viewer**: some `.pnt` paints are packaged in a
  non-plain container rather than a plain `PNT\0` file. The paint decoder now
  handles both transparently (`paint::decode_any`), used everywhere paints are read.
- **Much faster 3D bike-model load**: textures encode with fast deflate and in
  parallel (`rayon`) instead of serial max-compression PNG — big bikes load quickly.
- **No more freezes / blank white screens**: added a global + canvas `ErrorBoundary`
  with a WebGL context-loss handler, so a render error shows a recoverable panel
  instead of an unrecoverable white screen.
- **Rider body no longer see-through**: rider meshes render double-sided
  (`THREE.DoubleSide`), so the body reads as solid.
- **Reliable `tauri dev` startup**: a `predev` step frees the Vite port, and dev
  builds fully quit on window close (release builds still hide to tray) so the port
  isn't orphaned.

## 2026-07-15 — v0.1.5

### Added
- **Instant preset refresh (Windows)**: applying a preset while MX Bikes is
  running now updates the bike's look **live** — no game restart and no manual
  profile reselect. It re-runs the game's own profile loader in place (found by
  reverse-engineering `mxbikes.exe`). **On by default**; toggle under
  **Settings → General → Instant preset refresh**.
- **Honest apply feedback**: the apply toast now says exactly how it took effect
  — refreshed live, "reselect your profile in-game to load it" (while the game is
  open), or "loads next launch" — instead of implying a FrostMod content reload
  already applied the new look.

### Changed
- Instant refresh lives in **Settings** (default on), not as a toggle inside the
  preset menu, so it doesn't alarm players mid-customization.
- `presets_apply` returns a richer `PresetApplyOutcome` (`content_reload`,
  `game_running`, `live_refresh`); new `gameproc` module handles game detection
  and the in-place loader call; new `instant_refresh` setting persists the choice.

### Fixed
- **FrostMod update no longer fails with "file in use"**: updating FrostMod from
  the app now **stops** the running FrostMod first, overwrites
  `frostmod.exe`/`.dll` (with a short lock-release retry), then **restarts** it —
  so updates are seamless instead of erroring because the files were in use.

## 2026-07-15 — v0.1.4

### Added
- **Sound mods visible in the Library**: installed bike sounds now appear as
  their own **Sound** entries. Because a sound merges into an OEM bike folder
  (indistinguishable from stock on disk), the app records provenance at install
  time (`soundmods` store → `sound-mods.json`) and the Library surfaces exactly
  those bike folders that still carry the sound files — no guessing, no
  mislabeling stock bikes. New `sound` library category (label/icon).
- **Auto-pick the right sound download per bike**: sound-mod pages list a
  *different* download per bike ("Just KTM 250SX-F") plus a "Main pack with all
  bikes" default — these are **not** mirrors. The install dialog now treats them
  as per-bike options, auto-selecting the link that matches the chosen bike (and
  falling back to the all-bikes pack), instead of claiming "all mirrors contain
  the same file". New `pickDownloadForBike` + `isSoundContext`/`SOUND_CATEGORY_ID`.
- **Presets — customization loadouts**: new Presets tab that saves a full look
  (bike livery, number/suit fonts, tyres, rider kit, helmet + paint, goggles,
  gloves, boots + paint, protection + paint, riding style, race number) and applies
  it to a bike on command. MX Bikes keeps the selected look **per bike** in
  `profiles/<profile>/profile.ini` (one section per slot, keyed by bikeid); a preset
  is a bike-agnostic bundle of those values. Capture a bike's current look or build
  one from installed mods (dependent pickers — helmet paints follow the chosen
  helmet, etc.), save it named, and quick-apply — writing only the target bike's
  rows (with a `profile.ini.bak` backup) and nudging a running FrostMod to reload.
  A preset can also carry a **model swap** (applied via the Locker's model-swap
  machinery). **Share** presets as portable `MXBP1-…` codes (copy/paste) that others
  **Import**, with a missing-mod warning for anything they haven't installed. New
  line-oriented profile.ini editor (`presets` Rust module) that rewrites only the
  targeted `<bikeid>=` lines, `presets_*` Tauri commands, and a `presets.json` store.
- **Model Swaps — in-app bike model swaps**: new Model Swaps tab that mirrors FrostMod's
  in-game model swapper (F8 > 3) from the app. Lists each swappable bike (a folder
  with a loose `model.edf` **or** a `FrostMod Models/` library — so a bike whose
  active Original is `.pkz`-packed still shows and stays reachable), its active
  model, and every alternate set under `<Bike>/FrostMod Models/`, and lets you
  switch between them — the same backup-current / move-in-chosen file dance (whole
  loose set, `paints/` left put, with rollback) and `_active.txt` marker FrostMod
  uses, so the two stay interchangeable. Signals a running FrostMod to live-reload
  after a swap. New `scan_model_swaps` / `apply_model_swap` Tauri commands (Rust
  `modelswap` module).
- **Silent FrostMod setup**: FrostMod now installs and starts automatically on
  first run instead of showing a "Set up FrostMod?" prompt. Added a manual
  re-check button next to the FrostMod row in Settings.
- **In-app MEGA downloads**: MEGA public file links now install directly in the
  app (fetch + decrypt via the pure-Rust `mega` crate on the existing reqwest
  client) with the same progress stages as other hosts — no browser round-trip
  and no external megatools/MEGAcmd binary required. Folder links still fall back
  to manual browser download.
- **In-app MediaFire downloads**: MediaFire file links install directly in the app
  again. Verified empirically (full 427 MB `.pkz`) that MediaFire's CDN no longer
  blocks the rustls client, so `resolve_mediafire` + the normal download path
  handle them — the old "CDN blocks non-browser TLS" workaround no longer applies.

### Changed
- **MX Bikes Shop installs route by mod type**: a purchased download no longer
  always lands in `mods/tracks`. A structured archive (a `mods/` tree, top-level
  `bikes/tracks/rider/…`, or a `<Bike>/paints/` livery bundle) now self-routes by
  its own folders — the livery-bundle case works regardless of the caller's
  default type — and content that can't be inspected (a locked `.pkz`) picks
  its bucket from the item's title (`guess_mod_type`) instead of assuming tracks.
- **FrostMod update check**: Settings now re-checks FrostMod against GitHub when
  it opens (and when the About "Check for updates" button is pressed), so a newer
  release surfaces an "Update to vX" button instead of a stale "Up to date".
- MEGA and MediaFire are no longer treated as "blocked" hosts in the install UI,
  so their mirrors get the in-app install button instead of the
  download-and-import fallback (`BLOCKED_HOST_PATTERNS` is now empty).

### Fixed
- **Sound mods no longer routed into a bike's `paints/`**: bike **sound** mods
  (`engine.scl` + `sfx.cfg`, plus audio samples) install to the bike-folder
  **root** (next to `paints/`), never inside it. The install picker now offers a
  per-bike **root** destination and defaults sounds to the name-matched bike; the
  Rust placer gained a sound-bundle guard that strips a stray `paints` segment so
  loose sound files can't be misfiled (with new placement tests). Well-packaged
  mods that carry a full `mods/bikes/…` tree already merged correctly — this also
  removes the misleading "install to paints" the dialog used to suggest.
- **FrostMod "up to date" false positive**: a failed or offline GitHub check no
  longer displays as "Up to date". The panel now distinguishes *Checking…*,
  *Couldn't check* (offering "Reinstall latest"), and a confirmed-current install,
  so users aren't told they're current when the check simply didn't run.
- **MediaFire link resolution**: replaced the stale `id="downloadButton"` fallback
  regex (which matched nothing on today's pages) with the current
  `aria-label="Download file"` link inside `#download_link`.
- **Bare `.pnt` paints install**: mods shipped as a loose `.pnt` file (not zipped)
  now pass through extraction like `.pkz` does, instead of failing with
  "Unsupported archive type". More common now that MEGA links install in-app.

## 2026-07-15 — v0.1.3

### Fixed
- **Kaizo servers no longer hidden from the browser**: the app now manages
  FrostMod's `frostmod_serverfilter.yaml` in the FrostMod folder. FrostMod's stock
  default filter blocked Kaizo (a `kaizo` name rule + a `k[a4][il1]z[o0]` spam
  regex); we now write a curated `# frostmod-filter v4` config that keeps the
  ad/cheat-shop spam rules but drops the Kaizo matches. Written on FrostMod
  install/update and refreshed before each managed launch, so existing installs
  get corrected automatically; a filter the user has hand-edited is left untouched.

### Removed
- **Locker (experimental 3D bike-livery viewer)**: removed the Locker scene and its
  sidebar/dashboard entries; the feature is dropped for this release.

## 2026-07-15 — testing feedback pass

### Added
- **Full library detail view**: clicking any installed track/bike/gear card opens
  a dedicated detail page — large preview (**click to enlarge in a lightbox**),
  all parsed metadata (name, author, length, altitude, location), format, size and
  on-disk path, plus quick actions (Move / Show in Explorer / Uninstall). Backed by
  a new `get_pkz_preview` command that returns a full-resolution preview (the card
  thumbnail stays small); `pkz` internals refactored into a shared `inspect` used
  by both.
- **Extracted-folder tracks now appear in the library**: tracks installed as loose
  files (not a single `.pkz`) are detected by their track markers (`.map`/`.trh`/…)
  and shown as one item with name/author/preview read from their loose `.ini` — the
  old scan only listed `.pkz` and silently dropped these.
- **Every rider category is now visible**: the Rider (player) library groups by
  category — Helmets, Helmet Paints, Goggles, Boots, Boot Paints, Protection,
  Gloves and Outfit/Kit — surfacing loose paints/gloves/goggles/outfit that the old
  `.pkz`-only scan hid (only helmets/boots showed before). Each item carries its
  info/thumbnail where readable.
- **Bike detail shows its liveries + model swaps**: a bike's detail view lists the
  paints in `<Bike>/paints` and any model-swap `.pkz` inside the bike's folder;
  gear models likewise list their paints/goggles. Backed by a richer
  `scan_library` command (kind/category/parent per item) used by the library while
  install pickers keep the leaner `get_installed_mods`.

### Fixed
- **New bikes no longer install into a bike's `paints` folder**: only actual bike
  **liveries** (WP category 37) default/route into `<Bike>/paints`; new bikes,
  sounds and unknown bike content default to `mods/bikes` root, and a remembered
  livery `paints` folder is no longer inherited by a subsequent new-bike install.
- **Install dialog no longer clips its own header/X**: the dialog is capped at
  `85vh` with a scrolling body, so expanding the folder picker can’t push the modal
  past the viewport and hide the close button.
- **Guard against accidental reinstalls**: quick-install, bulk-install and "Add to
  Library" now show an "are you sure — this overwrites the installed files" confirm
  when the mod is already in your library.

### Changed
- **Removed the retired "Wheels" bike browse category** (id 95) — it no longer
  maps to real content.
- **Uninstall works on extracted-folder mods**, not just `.pkz` files (moves the
  whole folder to the Recycle Bin).

## 2026-07-15

### Changed
- **v0.1.2 release** — bumped version across `package.json`, `tauri.conf.json`
  and `Cargo.{toml,lock}`.
- **About credits trimmed** to a single "Frost" credit (links to
  github.com/Frostn1); removed the Blarne / "Long live MXBMM" lines.
- **All app state now lives in one Local AppData folder**: config, shop session,
  and the FrostMod install moved from Roaming to
  `%LOCALAPPDATA%\com.frost.mxbikes\` (joining the existing cache), so everything
  is in one per-machine place. No migration (pre-release) — old Roaming files are
  simply re-created on next launch.

### Added
- **Rider content**: a new **Rider** browse section (Rider Kit, Helmets, Helmet
  Paints, Gloves, Boots, Protection) installing into `mods/rider`. Paints route to
  the right place automatically — helmet/boot/protection paints into their model's
  `paints`/`goggles` (pick the installed model, name-matched like bike liveries),
  and rider outfit + gloves into the rider **profile** you choose
  (`riders/<profile>/{paints,gloves}`, scanned from your install via a new
  `scan_rider_targets` command).
- **File logging**: added `tauri-plugin-log`, writing to
  `%LOCALAPPDATA%\com.frost.mxbikes\logs\`. Startup logs the app version and the
  data/log dir paths, and shop session/login/download failures are now logged.

### Added
- **First-launch welcome tour**: a 3-slide intro overlay (what MXB App is →
  browse & install → FrostMod) shown once on first launch before folder setup,
  tracked via a `mxb:welcomeSeen:v1` localStorage flag. New
  `Components/Welcome/Welcome.tsx`.
- **Windows executable publisher & metadata**: the installer and the `.exe`
  version info now carry a publisher ("Frost"), copyright, homepage and
  description so Windows shows a proper publisher/details instead of blanks.
  Set via `bundle.publisher`/`copyright`/`homepage`/`shortDescription`/
  `longDescription` in `tauri.conf.json`. (Does not replace Authenticode code
  signing — SmartScreen may still warn until the exe is signed.)

- **Rich library cards from inside the `.pkz`**: plain-zip tracks and
  bikes now show their **real name, author, length and a preview thumbnail** read
  straight from the archive's `.ini` and preview image, plus the **file size** on
  every card. Preview images (often TGA, which browsers can't render) are decoded
  and downscaled to a small JPEG in Rust. **Locked `.pkz` are
  detected and skipped gracefully** — they show a lock badge with just name + size.
  Parsing is lazy per card (list paints instantly) and cached to disk. Backed by a
  new `get_pkz_meta` Tauri command + `pkz` module (`image`/`base64` crates), with
  `size` added to the `InstalledMod` model.

- **MX Bikes Shop downloads**: a new **Shop** tab lets you sign in to
  mxbikes-shop.com and install the tracks you've **already purchased**
  ("All My Downloads") with the same one-click download → extract → place flow and
  "Installed" badge as Browse. Sign-in happens in a real WebView window (your
  password never touches the app); the captured session is persisted so you stay
  logged in across restarts, with a Log out button. Backed by new `shop_*` Tauri
  commands, an authenticated shared `reqwest` client, and a reusable
  `download_and_place` install helper shared with the free catalog.

### Fixed
- **Install destination picker for bike liveries**: the folder list is now
  **scrollable** and no longer overflows the popup, long bike names **truncate**
  instead of cutting off, and it's a **command-style search** — probable bikes
  (matched from the mod name) show under "Probably" at the top, with a search box
  to find any bike.

### Added
- **Start FrostMod without restarting the app**: if FrostMod isn't running, a play
  button appears on the sidebar status pill and in Settings → FrostMod to launch it
  on the spot.

### Added
- **FrostMod is managed in-app**: MXB App now **downloads FrostMod** from its GitHub
  releases, **runs `frostmod.exe`** hidden in the background so it's connected as
  soon as the app opens, and **updates** it — no manual setup. Settings → FrostMod
  shows the installed vs latest version with an Install / Update button and a
  "Run FrostMod automatically" toggle, and a first-run prompt offers to set it up.
  The managed process is stopped on a real Quit. (Injector is Windows-only; the
  manager no-ops elsewhere.)
- **Runs in the background like Discord**: closing the window now hides MXB App to
  a **system-tray icon** (Show / Quit menu) instead of quitting, so it keeps running
  and FrostMod stays connected. **Launches at login** by default. Both are
  toggleable in Settings → **General** ("Keep running in the background", "Launch at
  startup"). Backed by a tray icon + `WindowEvent::CloseRequested` intercept and the
  `tauri-plugin-autostart` plugin; prefs persist in the app config (default ON).
- **"Made with ❄ by Frost"** credit in Settings → About, linking to the author.

### Changed
- **Release assets get clean names**: a CI finalize step renames the ugly
  `MXB.App_0.1.0_x64-setup.exe` to `MXB-App-0.1.0-x64.exe` (and the `.dmg`
  likewise) and repoints `latest.json`, so downloads look trustworthy. Signatures
  are over file content, so self-update still verifies.

## 2026-07-14

### Added
- **Windows install wizard**: the Windows build now ships a branded **NSIS**
  installer (welcome → license → install → finish) instead of a bare bundle.
  Installs **per-user with no admin/UAC** prompt, uses the snowflake app icon, and
  shows the MIT license. Configured in `tauri.conf.json` (`bundle.windows.nsis`);
  MSI dropped from the targets.
- **Auto-update**: the app checks GitHub Releases on launch (quietly) and offers
  **"Restart & update"** via a toast when a newer signed build exists; a manual
  **Check for updates** button lives in Settings → About. Backed by the Tauri
  `updater` + `process` plugins, signed release artifacts (`createUpdaterArtifacts`),
  and a `latest.json` published by CI. Requires the `TAURI_SIGNING_PRIVATE_KEY`
  secret and a published release to take effect.
- **App icon**: a snowflake mark on an icy gradient badge, generated into
  `src-tauri/icons/*` (`.ico`, `.icns`, PNGs) — this is what shows on the
  taskbar/dock and the `.exe`. The in-app UI is unchanged.
- **Platform-adaptive title bar**: on macOS the window now uses native
  decorations with `titleBarStyle: "Overlay"` (new `tauri.macos.conf.json`), so
  it gets real traffic-lights, rounded corners and the native shadow, and our
  custom window buttons are hidden. Windows keeps the frameless custom title bar
  and its Windows-style controls, unchanged.
- README: roadmap entries for **bike + rider liveries** and **auto-update**.

### Fixed
- The product name still read "MXB App by Frost" in `productName` (the name shown
  on the taskbar and the installed `.exe`) and in the window title — both are now
  **MXB App**. Remaining in-app copy that called the app "Frost" (title bar,
  Setup, install/blocked-host text) now says **MXB App**. (FrostMod, the separate
  live-reload tool, keeps its name.)

### Changed
- README tech stack updated to Tailwind + shadcn/ui + lucide + Sonner (was MUI).

## 2026-07-13

### Added
- **UI redesign**: a dark, Apple-clean rebuild of the whole UI on Tailwind +
  shadcn/ui, replacing MUI. A permanent **left sidebar** (Browse / Library /
  Settings) with a live install badge, a persistent **global install indicator**
  and **FrostMod status pill** that survive navigation, and the game path.
- **Settings screen** (new): game folder (change / auto-detect + re-scan),
  appearance (Light / Dark / System theme), FrostMod status + reload, and about.
- **Install popup** on "Add to Library": pick the destination folder (with mod
  counts, remembered per category) and choose a download mirror (default
  pre-selected, browser-only hosts flagged) before installing.
- **Toast notifications** (bottom-right) for install success/failure and
  uninstall, replacing inline alerts.
- **Library actions**: per-mod context menu with Move to folder, **Show in
  Explorer**, and **Uninstall** (moves the file to the Recycle Bin via new
  `reveal_in_explorer` / `uninstall_mod` Tauri commands + the `trash` crate).
- **Mod Detail** right-rail install surface with a live stage chain
  (Resolve → Download → Extract → Place → Reload) and a guided 2-step
  blocked-host flow for browser-only mirrors.
- README release badges: latest release, release date, and total download count
  (dynamic via shields.io, GitHub-backed), plus MIT license and Windows x64
  platform badges. Added a root `LICENSE` file (MIT).
- **FrostMod live-reload integration**: when you add a mod, the app now signals a
  running [FrostMod](https://github.com/Frostn1/frostmod) to re-scan the mods
  folder so new tracks/bikes appear in-game without a restart. Works by setting
  FrostMod's own `Local\FrostModReload` Windows event (the same trigger as
  pressing **R** in its console) — no changes to FrostMod required. The mod
  detail view shows whether FrostMod picked it up live or isn't running, and new
  `frostmod_reload` / `frostmod_running` commands back a manual trigger + status.

- **Right-click actions**: right-clicking a mod in **Browse** offers *Quick
  install*, *Open details*, and *Select*; right-clicking a row in **Library**
  opens the same Move / Show in Explorer / Uninstall menu as the 3-dot button.
- **Quick install**: installs a mod straight from Browse with no detail page and
  no dialog — it resolves the best direct mirror and reuses the remembered (or
  auto-guessed) destination folder, then reports where it landed via a toast.
  Browser-only hosts (MediaFire/Mega) can't install silently and are skipped
  with an explanation.
- **Multi-select + bulk install** in Browse: select mods via the card checkbox or
  the right-click menu, then *Quick install N* from the selection bar
  (with *Select all* / *Clear*).
- **Install queue**: installs still run strictly one at a time, but extra
  requests now queue and drain in order, with a "+N queued" line on the sidebar's
  install card.

### Fixed
- Mod Detail screenshots rendered squashed: the gallery and thumbnail strip are
  flex children of a scrolling column, so they were being **shrunk** instead of
  scrolled and lost their 16:9 height. Pinned them with `flex-none`.
- The **GitHub / Changelog links in Settings** pointed at a non-existent
  `Frostn1/frost` repo — corrected to `Frostn1/mxb-app`, and the About line now
  reads "mxb-app" rather than the old product name.
- MediaFire mods were mis-detected as auto-installable because the host label is
  written "Media Fire" (with a space) — downloads are now classified by **URL**,
  so blocked hosts correctly open in the browser instead of failing.

### Changed
- Navigation moved from top tabs to the left **sidebar**; the theme toggle moved
  from the title bar into Settings → Appearance. **Setup** is now a single step.
- Clearer download UI: one **official one-click** option; other links are labeled
  (a dedicated-**server** build is called out as "not needed for normal play"
  rather than "mirror"); the **Import** step only appears when a blocked host is
  used.
- Enabled **text selection** and added a **Copy** button on error messages.

### Removed
- MUI, Emotion, and all per-component SCSS; the top-tab `Header`, the `Footer`,
  and the old `LoginPage`/theme are replaced by the sidebar shell, Settings, and
  a token-based Tailwind theme.

## 2026-07-12

### Added
- **Release CI** (`.github/workflows/release.yml`): tagging `v*` (or a manual
  dispatch) builds Windows + macOS bundles with `tauri-action` and attaches the
  installers to a draft GitHub Release.
- **Import a file**: for hosts that block in-app downloads, open the download in
  the browser then import the downloaded file — the app extracts and places it
  into the right folder just like a normal install (`import_file` command).
- Download retries and full error-cause reporting on failed installs.

### Fixed
- Diagnosed installs failing with "error sending request for url …":
  **MediaFire's download CDN blocks all non-browser TLS clients** (verified
  across rustls, native-tls/SChannel, curl and Python — only real browsers get
  through). No TLS backend can bypass it, so MediaFire/Mega now fall back to
  browser download + Import. Auto-installable hosts (**Google Drive**, direct
  links) are shown first as the one-click option.

### Changed
- README: added Download, build-status badge, and Releases (how to cut one)
  sections.
- Renamed the app to **MXB App by Frost** (window title, title bar, header).
- Replaced the macOS traffic-light window buttons with **clean Windows-style
  controls** (minimize / maximize / close, red close hover).
- The Library now scans **recursively** and lists every installed `.pkz` with
  its sub-folder, so tracks/bikes nested inside folders show up.
- Kept **rustls** TLS (native-tls's SChannel failed the handshake on Windows).

## 2026-07-06

### Added
- **Browse & search** mods from mxb-mods.com in-app, with category filters,
  **"Load more" pagination**, and a mod detail page with an image gallery.
- **Add to Library**: one-click download → extract → place into the MX Bikes
  folder, with live progress. Resolves MediaFire and Google Drive links
  (including large-file virus-scan confirmation); extracts `.zip`/`.7z`/`.rar`
  and places `.pkz` files.
- Multiple download hosts on a page are shown as a primary "Add to Library"
  button plus per-host mirrors.
- **Bikes** mod type alongside Tracks, via a type switcher in the header;
  Browse, install, and Library are all per-type.
- **"In library" badges** on browse cards and the detail page (fuzzy name match
  against installed files).
- Loading skeletons, an error "Retry" button, and persisted light/dark theme.
- HTTP timeouts (30s API, 15s connect) for resilience.
- Swappable `ModSource` trait in the Rust backend (mxb-mods.com implementation
  via the WordPress REST API + download-page HTML parsing).
- Native folder picker for choosing the MX Bikes path during setup.
- Rust unit tests for REST/HTML parsing and download-link resolution.
- `CHANGELOG.md` and a real `README.md`.

### Changed
- **Upgraded Tauri v1 → v2** (config schema, capabilities/permissions, plugin
  system; `shell` + `dialog` plugins).
- **Converted the frontend from JavaScript to TypeScript** (typed API layer and
  shared types mirroring the Rust structs).
- Rebranded the app to **Frost** (was "MXBMM" / "The MXB App").
- Install placement is generalized to a per-type subfolder (`mods/tracks`,
  `mods/bikes`), configurable in one place in the frontend.
- Config now lives in the OS app-config dir instead of a cwd-relative
  `.config.json`.
- The Library is a proper per-type grid with manual refresh.
- Updated dependencies (MUI 6, React 18.3, Vite 5, current Tauri 2 stack).

### Removed
- Unused dependencies: Mantine (`@mantine/*`, `postcss-preset-mantine`), `axios`,
  and `path-browserify`.
- Dead/broken `src-tauri/src/config.rs` (replaced with a working config module)
  and a stale `.config.json` dev artifact.
