# MXB App

[![Release](https://github.com/Frostn1/mxb-app/actions/workflows/release.yml/badge.svg)](https://github.com/Frostn1/mxb-app/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/Frostn1/mxb-app?sort=semver&label=release)](https://github.com/Frostn1/mxb-app/releases)
[![Release date](https://img.shields.io/github/release-date/Frostn1/mxb-app?label=released)](https://github.com/Frostn1/mxb-app/releases)
[![Downloads](https://img.shields.io/github/downloads/Frostn1/mxb-app/total?label=downloads)](https://github.com/Frostn1/mxb-app/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20x64-0078D6)](#development)

**MXB App** is a desktop mod manager for [MX Bikes](https://mx-bikes.com/). It
replaces the tedious manual install dance — open mxb-mods.com, follow the link,
download from MediaFire, unzip, and move files into the right folder — with a
single flow:

> **Search a mod → open its page → click _Add to Library_ → done.**

MXB App downloads the mod, extracts it, and drops the track files into your MX
Bikes `mods/tracks` folder automatically.

Track mods are supported today; more mod types are planned.

## Download

Grab the latest installer from the
[**Releases**](https://github.com/Frostn1/mxb-app/releases) page:

- **Windows** — `.msi` or `.exe` (recommended; MX Bikes runs on Windows).
- **macOS** (Apple Silicon) — `.dmg`, for working on the download/extract UI.

Builds are unsigned, so Windows SmartScreen / macOS Gatekeeper will warn on
first launch — choose _Run anyway_ / right-click _Open_.

## How it works

- **Catalog** comes from [mxb-mods.com](https://mxb-mods.com) via its public
  WordPress REST API (search, listings, images), behind a swappable `ModSource`
  trait in the Rust backend.
- **Downloads** are resolved per host — MediaFire and Google Drive today, direct
  links as-is. Mega isn't supported yet (open the page to grab it manually).
- **Archives**: `.zip` and `.7z` are extracted natively; `.pkz` files are placed
  as-is. (`.rar` is not supported yet.)

## Tech stack

- [Tauri 2](https://tauri.app/) (Rust backend)
- [React 18](https://react.dev/) + [TypeScript](https://www.typescriptlang.org/)
  + [Vite](https://vitejs.dev/)
- [Tailwind CSS](https://tailwindcss.com/) + [shadcn/ui](https://ui.shadcn.com/)
  (Radix primitives) for UI, [lucide](https://lucide.dev/) icons,
  [Sonner](https://sonner.emilkowal.ski/) toasts, and
  [Swiper](https://swiperjs.com/) for galleries

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

> MX Bikes is Windows-only, so downloading into a real game install is a
> Windows workflow. The cross-platform download/extract logic can be built and
> tested on any OS.

## Releases

Releases are built in CI by
[`.github/workflows/release.yml`](.github/workflows/release.yml) — it compiles
Windows and macOS bundles and attaches them to a GitHub Release.

To cut a release, bump the version in `package.json`, `src-tauri/tauri.conf.json`
and `src-tauri/Cargo.toml`, then push a matching tag:

```sh
git tag v0.2.0
git push origin v0.2.0
```

The workflow produces a **draft** release with the installers attached — review
it in the Releases tab and publish when ready. You can also trigger a build
without tagging via **Actions → Release → Run workflow**.

## Roadmap

Features coming next:

- **Liveries** — browse and install **bike liveries** and **rider gear/kit**
  (helmet, jersey, pants, boots), installed into each bike's `paints` folder and
  the rider folders, with previews so you can see a livery before installing it.
- **Auto-update** — the app updates itself in the background: it checks for a new
  release on launch, downloads it, and installs on restart, so you're always on
  the latest version without re-downloading the installer.
- More mod types (assets, sounds, wheels, …) — the `ModSource` trait and category
  ids already generalize beyond tracks.
- An injected DLL that reads your in-game track list and one-click-installs the
  tracks you're missing, then refreshes the game library.
