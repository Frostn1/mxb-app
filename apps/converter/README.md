# Frost Converter

FBX to EDF for MX Bikes. The same converter and options as the Convert tab in Frost's Studio, as a
small Windows app with a command line.

## Quick start

1. Download `Frost-Converter-<version>-portable.exe` (or the installer) from the
   [releases page](https://github.com/Frostn1/frost-converter/releases/latest).
2. Open it and drop FBX files or folders on the window.
3. Press **Convert**. Each `.edf` is written beside its FBX, or into one folder if you pick one.

A bike (an FBX with `steer`, `fsusp` or `rsusp` parts) is written as `model.edf` with its `.hrc`
files, and a shadow FBX as `shadow_model.edf`. The report under each file lists what was skipped
and why, the `.shd` files written, and textures with a colour key or alpha blend flag.

Settings are remembered between runs. No account is needed.

## Command line

```
frost-converter.exe bike.fbx -o out\
frost-converter.exe "C:\mods\My Bike" --hrc always
frost-converter.exe helmet.fbx --whole --no-shd --json
```

Run `frost-converter.exe --help` for every option. The defaults match the window's. The exit code
is 0 when every file converted, 1 when one failed and 2 for a bad command line.

A `.bat` file waits for each conversion. In PowerShell, add `| Out-Host` so the line waits too:

```
.\frost-converter.exe bike.fbx -o out | Out-Host
```

## Building

The converter's engine is private (`crates/fbx2edf`); without it the app builds but cannot
convert. With it synced in:

```
bun install
bun run tauri:converter dev
```

Releases are built by `.github/workflows/release-converter.yml` on `converter-v*` tags.
