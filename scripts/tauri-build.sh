#!/usr/bin/env bash
#
# The build tauri-action runs on Linux — `tauriScript` in .github/workflows/release.yml.
#
#   scripts/tauri-build.sh build [args...]
#
# The same `tauri build` every other platform gets, plus the one thing that has to happen
# in between the bundler writing the AppImage and the action collecting it: the bundled
# libwayland comes out (scripts/appimage-drop-bundled-wayland.sh, which says why), and the
# updater signature is taken again over the bytes that will actually ship. Doing it here
# rather than after the release is published means the action uploads the stripped image
# and writes `latest.json` from the matching signature, so nothing downstream can drift.
#
# Windows and macOS don't go through this — the workflow leaves `tauriScript` empty there
# and the action detects the package manager as before.

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(dirname "$here")"
cd "$root"

# `npx tauri` is the `tauri` script in package.json without npm's round trip through a
# shell — which matters below, where the bundle's path has a space in it.
npx tauri "$@"

# `tauri dev` and the rest produce no bundles to fix up.
if [ "${1:-}" != "build" ]; then
  exit 0
fi

shopt -s nullglob
# Second pattern for a `--target`ed build, which bundles under the triple instead.
images=(
  src-tauri/target/release/bundle/appimage/*.AppImage
  src-tauri/target/*/release/bundle/appimage/*.AppImage
)
if [ "${#images[@]}" -eq 0 ]; then
  echo "$0: no AppImage in this build — nothing to fix up"
  exit 0
fi

for image in "${images[@]}"; do
  bash "$here/appimage-drop-bundled-wayland.sh" "$image"

  # The bundler signed the file the updater is about to be told to trust, and those bytes
  # have changed. Without this every Linux self-update fails signature verification.
  if [ -f "$image.sig" ]; then
    echo "$(basename "$image"): signing again over the rebuilt image"
    npx tauri signer sign "$image"
  fi
done
