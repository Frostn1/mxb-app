# Frost

**Frost** is a desktop mod manager for [MX Bikes](https://mx-bikes.com/). It
replaces the tedious manual install dance — open mxb-mods.com, follow the link,
download from MediaFire, unzip, and move files into the right folder — with a
single flow:

> **Search a mod → open its page → click _Add to Library_ → done.**

Frost downloads the mod, extracts it, and drops the track files into your MX
Bikes `mods/tracks` folder automatically.

Track mods are supported today; more mod types are planned.

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
- [MUI](https://mui.com/) for UI, [Swiper](https://swiperjs.com/) for galleries

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

## Roadmap

- More mod types (bikes, assets, …) — the `ModSource` trait and category ids
  already generalize beyond tracks.
- An injected DLL that reads your in-game track list and one-click-installs the
  tracks you're missing, then refreshes the game library.
