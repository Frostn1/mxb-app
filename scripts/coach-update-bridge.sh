#!/usr/bin/env bash
#
# Point pre-move MXB Coach installs at a release that now lives in another repository.
#
#   scripts/coach-update-bridge.sh coach-v0.1.17
#
# MXB Coach releases from Frostn1/mxb-coach since 0.1.17, but every install from 0.1.3 to
# 0.1.16 shipped an updater that lists *this* repo's releases and picks the newest `coach-v`
# tag carrying a `latest.json`. That is baked into binaries already on people's machines.
#
# So this publishes a release here under the original `coach-v` tag carrying nothing but the
# signed `latest.json` from the real release over there. The manifest's own download URLs
# point at mxb-coach, which is public, so an old install updates straight to the new build —
# and that build looks in the right place from then on.
#
# Always a pre-release: this repo's `releases/latest` is MXB App's updater endpoint and must
# never resolve to a coach build. Asserted below rather than assumed.
#
# Reads the source release with COACH_REPO_TOKEN when it is set (a fine-grained PAT cannot
# read a repo outside its scope), and writes here with GH_TOKEN.

set -euo pipefail

TAG="${1:-}"
if [ -z "$TAG" ]; then
  echo "usage: $0 <coach-v tag>" >&2
  exit 2
fi
case "$TAG" in
  coach-v*) ;;
  *) echo "not a coach tag: $TAG" >&2; exit 2 ;;
esac

VERSION="${TAG#coach-v}"
SRC_REPO="Frostn1/mxb-coach"
SRC_TAG="v$VERSION"
DST_REPO="Frostn1/mxb-app"

dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT

GH_TOKEN="${COACH_REPO_TOKEN:-${GH_TOKEN:-}}" \
  gh release download "$SRC_TAG" -R "$SRC_REPO" --pattern latest.json --dir "$dir"

cat > "$dir/body.md" <<EOF
**MXB Coach $VERSION is at [Frostn1/mxb-coach](https://github.com/$SRC_REPO/releases/tag/$SRC_TAG).**
The download and what's new in it are both there.

This page only exists so copies of MXB Coach installed before the move still find the update.
There is nothing to download here.
EOF

title="MXB Coach $VERSION — moved to mxb-coach"

if gh release view "$TAG" -R "$DST_REPO" >/dev/null 2>&1; then
  gh release upload "$TAG" "$dir/latest.json" -R "$DST_REPO" --clobber
  gh release edit "$TAG" -R "$DST_REPO" --prerelease --title "$title" --notes-file "$dir/body.md"
else
  gh release create "$TAG" "$dir/latest.json" -R "$DST_REPO" \
    --prerelease --title "$title" --notes-file "$dir/body.md"
fi

# A coach release that came out as `latest` would be handed to every MXB App install as its
# next update. Fail the job rather than leave that standing.
if [ "$(gh release view "$TAG" -R "$DST_REPO" --json isPrerelease -q .isPrerelease)" != "true" ]; then
  echo "::error::$TAG is not a pre-release in $DST_REPO — MXB App's updater endpoint is at risk"
  exit 1
fi

echo "bridged $TAG → $SRC_REPO@$SRC_TAG"
