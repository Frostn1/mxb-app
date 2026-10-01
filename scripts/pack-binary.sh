#!/usr/bin/env bash
#
# Pack the compiled Windows app binary with UPX, before Tauri bundles it into the installer, so
# the shipped .exe is compressed and harder to read at rest — the last of the binary hardening,
# on top of the stripped/LTO'd release profile and the runtime anti-debug guard.
#
# Wired as each app's Tauri `beforeBundleCommand`, so it runs after the build and before the
# bundler picks the binary up. It is deliberately narrow and defensive:
#
#   * Gated on MXB_PACK=1 — the release workflows set it on the Windows leg only. A local
#     `tauri build`, a fork, and the macOS/Linux legs all leave it unset and it no-ops. UPX
#     breaks macOS notarization and buys nothing inside an AppImage's own compression, so it is
#     Windows-only by design, not by omission.
#   * Never fails the build. A missing upx, an already-packed binary, or a upx error warns and
#     returns 0 — a release that ships an unpacked binary is far better than no release.
#
# Note the trade-off, decided at the deployment level: a UPX-packed exe is more likely to trip
# Windows SmartScreen / antivirus heuristics. Turn MXB_PACK off in the workflow to ship unpacked.
set -uo pipefail

[ "${MXB_PACK:-0}" = "1" ] || { echo "pack-binary: MXB_PACK not set — skipping"; exit 0; }

case "$(uname -s)" in
  *NT*|*MINGW*|*MSYS*|*CYGWIN*) ;;                       # a Windows runner (Git Bash / MSYS)
  *) echo "pack-binary: not a Windows build — skipping"; exit 0 ;;
esac

if ! command -v upx >/dev/null 2>&1; then
  echo "::warning::pack-binary: upx not found on PATH — shipping the binary unpacked"
  exit 0
fi

# The workspace target dir, however this script was invoked (Tauri runs it from the app's
# src-tauri directory; the fallback resolves it from this script's own location).
root="$(git rev-parse --show-toplevel 2>/dev/null || (cd "$(dirname "$0")/.." && pwd))"
target="$root/target/release"

shopt -s nullglob
packed=0
for exe in "$target"/*.exe; do
  # Only the finished app binaries live at the top of target/release; deps and build scripts
  # sit under deps/ and build/. Skip anything already packed so a re-run is harmless.
  if upx -t "$exe" >/dev/null 2>&1; then
    echo "pack-binary: $(basename "$exe") already packed"
    packed=1
    continue
  fi
  echo "pack-binary: packing $(basename "$exe")"
  if upx --best --lzma "$exe"; then
    packed=1
  else
    echo "::warning::pack-binary: upx failed on $(basename "$exe") — leaving it unpacked"
  fi
done

[ "$packed" = "1" ] || echo "::warning::pack-binary: no app binary found in $target to pack"
exit 0
