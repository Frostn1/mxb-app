#!/usr/bin/env bash
#
# Print one release's section out of CHANGELOG.md.
#
#   scripts/changelog-section.sh <tag|version> [--heading] [--fold|--summary]
#
# Default output is the section's body — its `### Added` / `### Fixed` groups, verbatim.
#   --heading  print the section's heading line instead of the body.
#   --fold     join each bullet's hard-wrapped continuation lines back onto one line.
#              The file wraps for readability; Discord renders those newlines literally.
#   --summary  --fold, then cut each bullet down to its bold headline. Every item still
#              appears — that's the point — but a release with twenty of them fits in a
#              chat message. Implies --fold.
#
# A pre-release tag (`v0.7.0-beta.1`) reads the section for the version it's a candidate
# for (`v0.7.0`), since the changelog records releases, not the builds leading up to one.
#
# Exits 1 with no output when that version has no section yet (an untagged build, or a
# tag cut before the changelog was written) — callers fall back to their own wording.

set -euo pipefail

TAG=""
MODE="body"
FOLD=0
SUMMARY=0
for arg in "$@"; do
  case "$arg" in
    --heading) MODE="heading" ;;
    --fold) FOLD=1 ;;
    --summary) SUMMARY=1; FOLD=1 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) TAG="$arg" ;;
  esac
done

if [ -z "$TAG" ]; then
  echo "usage: $0 <tag|version> [--heading] [--fold|--summary]" >&2
  exit 2
fi

if ! root="$(git rev-parse --show-toplevel 2>/dev/null)" || [ -z "$root" ]; then
  root="$(cd "$(dirname "$0")/.." && pwd)"
fi
changelog="$root/CHANGELOG.md"
[ -f "$changelog" ] || exit 1

heading_for() {
  # Escape regex metacharacters so `0.6.1` can't match `0x6y1`.
  local ver_re
  ver_re="$(printf '%s' "$1" | sed 's/[].[^$*\\/]/\\&/g')"
  awk -v re="^## .*[[:space:]]v${ver_re}([[:space:]]|\$)" '$0 ~ re { print; exit }' "$changelog"
}

VERSION="${TAG#v}"
heading="$(heading_for "$VERSION")"
if [ -z "$heading" ] && [ "${VERSION%%-*}" != "$VERSION" ]; then
  # `0.7.0-beta.1` has no section of its own; fall back to `0.7.0`.
  VERSION="${VERSION%%-*}"
  heading="$(heading_for "$VERSION")"
fi
[ -n "$heading" ] || exit 1

if [ "$MODE" = "heading" ]; then
  printf '%s\n' "$heading"
  exit 0
fi

body="$(awk -v h="$heading" '
  $0 == h              { seen = 1; next }
  !seen                { next }
  seen && /^## /       { exit }
                       { print }
' "$changelog" | sed -e '/./,$!d')"

if [ "$FOLD" -eq 1 ]; then
  body="$(printf '%s\n' "$body" | awk '
    function flush() { if (buf != "") { print buf; buf = "" } }
    /^[[:space:]]*$/     { flush(); print ""; next }
    /^[[:space:]]*[-*] / { flush(); buf = $0; next }
    /^#/                 { flush(); print; next }
    buf != ""            { line = $0; sub(/^[[:space:]]+/, "", line); buf = buf " " line; next }
                         { flush(); print }
    END                  { flush() }
  ')"
fi

if [ "$SUMMARY" -eq 1 ]; then
  body="$(printf '%s\n' "$body" | awk '
    # A sub-bullet elaborates the item above it, so it goes with the detail — and so does
    # a continuation paragraph, which --fold leaves indented rather than joining to its
    # bullet. Both are the explanation, which is exactly what a summary drops.
    /^[[:space:]]+/                     { next }
    # Keep the bold headline the bullet opens with; drop the explanation after it.
    match($0, /^- \*\*[^*]+\*\*/)       { print substr($0, 1, RLENGTH); next }
                                        { print }
  ')"
fi

# Trim trailing blank lines.
printf '%s\n' "$body" | awk 'NF { for (i = 0; i < held; i++) print ""; held = 0; print; next } { held++ }'
