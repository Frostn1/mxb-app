#!/usr/bin/env bash
#
# Take the bundled libwayland back out of a built AppImage.
#
#   scripts/appimage-drop-bundled-wayland.sh <path/to/App.AppImage>
#
# linuxdeploy pulls libwayland-client, -cursor, -egl and -server in as dependencies of GTK
# and WebKit, so our AppImage carries Ubuntu 22.04's copies and puts them ahead of the
# host's on the library path. The host's Mesa loads libwayland itself, gets ours instead,
# and can't create an EGL display — which is the window that comes up and never paints.
# The AppImage excludelist names libwayland-client for exactly this reason (mesa#11316);
# the linuxdeploy build Tauri downloads carries an older copy of that list and bundles it
# anyway.
#
# Nothing the app sets can get around it. The loader has resolved these libraries before
# `main` runs, and the AppImage's own AppRun hook already exports `GDK_BACKEND=x11` — which
# is why running under XWayland never cured it. Deleting them is the fix: the app then uses
# the same libwayland every other GTK program on the machine uses.
#
# Rebuilt rather than repacked with appimagetool: `--appimage-offset` hands us the exact
# runtime the bundler shipped and the squashfs superblock its compression, so what comes out
# is the same AppImage with four fewer libraries in it, and a release build needs no tool it
# didn't already have — only squashfs-tools.
#
# The `.sig` beside the image is over the old bytes, so whatever signs for the updater has
# to sign again afterwards. scripts/tauri-build.sh, where this runs from, does.

set -euo pipefail

IMAGE="${1:-}"
if [ -z "$IMAGE" ]; then
  echo "usage: $0 <path/to/App.AppImage>" >&2
  exit 2
fi
if [ ! -f "$IMAGE" ]; then
  echo "$0: no such file: $IMAGE" >&2
  exit 1
fi

# The host has to own these, because the host's Mesa is what loads them. Everything else
# linuxdeploy bundled is the app's own business and stays.
PATTERN='libwayland-*.so*'

for tool in mksquashfs unsquashfs; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "$0: $tool not found — install squashfs-tools" >&2
    exit 1
  fi
done

IMAGE="$(cd "$(dirname "$IMAGE")" && pwd)/$(basename "$IMAGE")"
name="$(basename "$IMAGE")"
chmod +x "$IMAGE"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The runtime unpacks into ./squashfs-root of wherever it's called from, so call it from
# somewhere disposable. `--appimage-extract` needs no FUSE, which CI hasn't got.
( cd "$work" && "$IMAGE" --appimage-extract >/dev/null )
root="$work/squashfs-root"

mapfile -t bundled < <(find "$root" -name "$PATTERN" | sort)
if [ "${#bundled[@]}" -eq 0 ]; then
  echo "$name: no bundled libwayland — nothing to drop"
  exit 0
fi
for lib in "${bundled[@]}"; do
  echo "$name: dropping ${lib#"$root"/}"
  rm -f "$lib"
done

# Keep the runtime and the compression the bundler chose rather than deciding either here:
# an AppImage whose runtime can't read its own filesystem is a worse bug than the one this
# is fixing.
offset="$("$IMAGE" --appimage-offset)"
head -c "$offset" "$IMAGE" >"$work/runtime"
superblock="$(unsquashfs -o "$offset" -s "$IMAGE")"
comp="$(awk '/^Compression/ { print $2 }' <<<"$superblock")"
block="$(awk '/^Block size/ { print $3 }' <<<"$superblock")"
mksquashfs "$root" "$work/filesystem" \
  -root-owned -noappend -no-progress \
  -comp "${comp:-zstd}" -b "${block:-131072}" >/dev/null
cat "$work/runtime" "$work/filesystem" >"$work/rebuilt"
chmod +x "$work/rebuilt"

# Prove the rebuilt image before it replaces anything: the runtime has to be able to read
# the filesystem back, and what we deleted has to be gone from it.
rm -rf "$root"
( cd "$work" && ./rebuilt --appimage-extract >/dev/null )
if [ -n "$(find "$root" -name "$PATTERN" -print -quit)" ]; then
  echo "$0: libwayland survived the rebuild of $name" >&2
  exit 1
fi
if [ ! -x "$root/AppRun" ]; then
  echo "$0: the rebuild of $name came out without a runnable AppRun" >&2
  exit 1
fi

mv "$work/rebuilt" "$IMAGE"
chmod +x "$IMAGE"
echo "$name: rebuilt without the bundled libwayland ($(stat -c %s "$IMAGE") bytes)"
