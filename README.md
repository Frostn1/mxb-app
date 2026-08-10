# MXB App

[![Release](https://github.com/Frostn1/mxb-app/actions/workflows/release.yml/badge.svg)](https://github.com/Frostn1/mxb-app/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/Frostn1/mxb-app?sort=semver&label=release)](https://github.com/Frostn1/mxb-app/releases)
[![Release date](https://img.shields.io/github/release-date/Frostn1/mxb-app?label=released)](https://github.com/Frostn1/mxb-app/releases)
[![Downloads](https://img.shields.io/github/downloads/Frostn1/mxb-app/total?label=downloads)](https://github.com/Frostn1/mxb-app/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D6)](#development)

**MXB App** is a desktop mod manager for [MX Bikes](https://mx-bikes.com/). It
replaces the tedious manual install dance — open mxb-mods.com, follow the link,
download from MediaFire, unzip, and move files into the right folder — with a
single flow:

> **Search a mod → open its page → click _Add to Library_ → done.**

MXB App downloads the mod, extracts it, and drops the files into the matching MX
Bikes `mods` folder automatically.

Tracks, bikes, rider gear, paints, sounds, model swaps and riding-style
animations are all recognised — and anything can be installed by dropping it on
the window, sorted by what the archive holds rather than by what its title says.

## Download

Grab the latest installer from the
[**Releases**](https://github.com/Frostn1/mxb-app/releases) page:

- **Windows** — `.exe` NSIS installer (recommended; MX Bikes runs on Windows).
- **macOS** (Apple Silicon) — `.dmg`; Play launches the game through a CrossOver,
  Whisky or Wine bottle.
- **Linux** — `.AppImage`, `.deb` and `.rpm`, for playing under Proton (SteamOS
  included).

Builds are unsigned, so Windows SmartScreen / macOS Gatekeeper will warn on
first launch — choose _Run anyway_ / right-click _Open_.

You only install once: the app checks for new releases on launch (and every 6
hours), then downloads and installs them on restart.

## How it works

- **Catalog** comes from [mxb-mods.com](https://mxb-mods.com) via its public
  WordPress REST API (search, listings, images), behind a swappable `ModSource`
  trait in the Rust backend.
- **Downloads** are resolved per host — MediaFire, Google Drive and MEGA, direct
  links as-is. MEGA *folder* links are the exception (open the page to grab
  those manually).
- **Archives**: `.zip`, `.7z` and `.rar` are extracted natively; already-packaged
  `.pkz` / `.pnt` files are placed as-is.
- **Live reload**: a debounced watcher on `<modsPath>/mods` signals FrostMod to
  reload the game when mods are added — including ones installed outside the app.
- **Paint studio**: builds a `.pnt` from `.tga`/`.png` sheets, and unpacks an
  existing paint back into editable sheets that keep the texture names the model
  binds — so a livery made anywhere can be packed, installed and previewed here.
- **Designer**: draws the livery itself. Image and text layers, a brush, gradient,
  fill and shapes, every stroke landing on the 2D sheet and on the 3D model at the
  same time. Because the model is right there, the editor knows the geometry it is
  painting for: a reference underlay shows the paint you started from and the
  model's own UV islands beneath your work, hovering the sheet names the piece of
  bodywork under the cursor, and a layer can be fitted to a part and clipped to its
  outline — so an image covers the shroud and stops at the seam. Save writes the
  packed `.pnt` the game reads.
- **Self-update**: `tauri-plugin-updater` against the `latest.json` published with
  each release; signature-verified, installs on restart.
- **Supporters**: Settings → Supporters credits the people who bought a coffee on
  [Buy Me a Coffee](https://buymeacoffee.com/). The names come from
  [`supporters.json`](supporters.json) on `main`, fetched at runtime and cached —
  adding somebody there reaches installed copies without a release, and an offline
  launch still shows the last list it saw.

## Tech stack

- [Tauri 2](https://tauri.app/) (Rust backend)
- [React 18](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/)
  + [Vite](https://vitejs.dev/)
- [Tailwind CSS](https://tailwindcss.com/) + [shadcn/ui](https://ui.shadcn.com/)
  (Radix primitives) for UI, [lucide](https://lucide.dev/) icons,
  [Sonner](https://sonner.emilkowal.ski/) toasts, and
  [Swiper](https://swiperjs.com/) for galleries
- [three.js](https://threejs.org/) via
  [React Three Fiber](https://r3f.docs.pmnd.rs/) + drei for the 3D rider and bike
  previews

## Development

Prerequisites: [Node.js](https://nodejs.org/) 18+ and the
[Rust toolchain](https://www.rust-lang.org/tools/install), plus the
[Tauri system dependencies](https://tauri.app/start/prerequisites/) for your OS.

```sh
npm install          # install frontend dependencies
npm run tauri dev    # run the desktop app (Vite + Rust)
```

Other scripts:

```sh
npm run dev          # Vite dev server only (frontend; Tauri commands unavailable)
npm run build        # typecheck + build the frontend
npm run typecheck    # tsc --noEmit
npm run lint         # eslint
npm run tauri build  # produce a production desktop bundle
```

Rust backend (from `src-tauri/`):

```sh
cargo check          # typecheck the Rust
cargo test           # unit tests (REST/HTML parsing, download resolution)
```

> MX Bikes is a Windows game, so a real install is a Windows one — but the app
> launches it on macOS through a CrossOver, Whisky or Wine bottle, and on Linux
> under Proton. The cross-platform logic builds and tests on any OS.

### Building with the shop catalog

The **Shop** tab has two halves. **My purchases** signs in to
[mxbikes-shop.com](https://mxbikes-shop.com) with the user's own account and installs what
they have already bought — it needs no build-time credential. **Catalog** browses the store's
public listing, and that half needs an API credential supplied by the store. Copy the example
file and fill it in:

```sh
cp .env.local.example .env.local   # gitignored; never commit it
```

The store authenticates with a single custom header, so `MXB_SHOP_API_HEADER` is the header's
*name* and `MXB_SHOP_API_KEY` is its value.

`src-tauri/build.rs` reads the file at compile time and bakes the values into the Rust binary
— they are deliberately not Vite env vars, which get inlined into the JS bundle and would
ship the key to anyone who unzips the app. Setting the same names in the environment
overrides the file, which is how CI supplies them.

**Building without it is fully supported**: the Catalog tab simply doesn't appear, the Shop
opens straight on My purchases, and nothing else changes. That's what forks build. Official
releases get the values from the `MXB_SHOP_API_HEADER` and `MXB_SHOP_API_KEY` repository
secrets — **without those secrets, released builds ship with no catalog.**

The catalog is browse-only: it shows what the store sells and links out to the product page.
Buying happens on the store's own site. Installing something already bought goes through My
purchases, which downloads the file and hands it to the same review sheet a drag-and-drop
uses, so it lands by what the archive contains rather than by what its title suggests.

## Releases

Releases are built in CI by
[`.github/workflows/release.yml`](.github/workflows/release.yml) — it compiles
Windows, macOS and Linux bundles and attaches them to a GitHub Release.

Write the version's `CHANGELOG.md` section **before** tagging. The release body
and the Discord announcement are both composed from it by
[`scripts/changelog-section.sh`](scripts/changelog-section.sh), which finds a
section by the `v<version>` in its heading — so work still sitting under an
"Unreleased" heading ships notes that never mention it.

Then make sure `package.json`, `src-tauri/tauri.conf.json` and
`src-tauri/Cargo.toml` all carry the version, and push a matching tag:

```sh
git tag -a v0.9.1 -m "v0.9.1 — what it's called"
git push origin v0.9.1
```

A **suffixed** tag — `v0.9.1-beta.3`, `v0.9.1-rc.2` — builds the same three
platforms but publishes as a **pre-release**: GitHub keeps it off
`releases/latest`, which is the endpoint the in-app updater reads, so existing
installs are never offered it, and it's announced in the beta Discord channel
rather than the release one. Tagging the plain `v0.9.1` afterwards is what ships
it to everyone. The workflow decides both of those purely from the `-` in the tag,
so neither is set by hand.

The workflow **publishes** the release with the installers attached
(`releaseDraft: false`), renames the bundles to `MXB-App-<ver>-<arch>.<ext>` and
patches `latest.json` so self-update keeps verifying.

A tag can also be created from the GitHub web UI — **Releases → Draft a new
release → Create new tag on publish** — which is the way to cut one without a
terminal. **Actions → Release → Run workflow** is *not*: a `workflow_dispatch`
build tags itself `v<run number>`, leaves the running app without its version, and
skips the announcement. It's for testing that a build compiles, not for shipping.

## Roadmap

Features coming next:

- Reading your in-game track list through FrostMod (which already handles the
  live reload) to one-click-install the tracks you're missing.
- A 3D preview for GP Bikes. Building a `.pnt` is title-agnostic and already
  works there; only the preview needs part bindings GP Bikes hasn't got yet, so
  the Studio says so plainly rather than showing an empty stage.
