# Changelog

## 2026-07-13

### Added
- **FrostMod live-reload integration**: when you add a mod, the app now signals a
  running [FrostMod](https://github.com/Frostn1/frostmod) to re-scan the mods
  folder so new tracks/bikes appear in-game without a restart. Works by setting
  FrostMod's own `Local\FrostModReload` Windows event (the same trigger as
  pressing **R** in its console) — no changes to FrostMod required. The mod
  detail view shows whether FrostMod picked it up live or isn't running, and new
  `frostmod_reload` / `frostmod_running` commands back a manual trigger + status.

### Fixed
- MediaFire mods were mis-detected as auto-installable because the host label is
  written "Media Fire" (with a space) — downloads are now classified by **URL**,
  so blocked hosts correctly open in the browser instead of failing.

### Changed
- Clearer download UI: one **official one-click** option; other links are labeled
  (a dedicated-**server** build is called out as "not needed for normal play"
  rather than "mirror"); the **Import** step only appears when a blocked host is
  used.
- Enabled **text selection** and added a **Copy** button on error messages.

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
