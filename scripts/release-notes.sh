#!/usr/bin/env bash
#
# Compose the GitHub Release body for a tag.
#
#   scripts/release-notes.sh <tag>
#
# What's new first — the tag's CHANGELOG section, so the release page says what shipped
# instead of only which file to download — then the download guidance. Used by
# .github/workflows/release.yml, which passes the result to tauri-action as `releaseBody`,
# so this file is the one place either half is worded.

set -euo pipefail

TAG="${1:-}"
if [ -z "$TAG" ]; then
  echo "usage: $0 <tag>" >&2
  exit 2
fi

here="$(cd "$(dirname "$0")" && pwd)"

# A suffixed tag (`v0.7.0-beta.1`) is a beta build of the version it names. Say so at the
# top, because the one thing a tester needs to know is that nothing will arrive on its own.
case "$TAG" in
  *-*)
    cat <<EOF
> [!NOTE]
> **This is a beta build of ${TAG%%-*}, for testing.** Installed copies of MXB App won't
> be offered it by the updater — download the installer below to try it. The full release
> follows once it's been checked over.

EOF
    ;;
esac

# `## 2026-08-07 — v0.7.0 — Six languages, …` → the part after the version.
release_name() {
  "$here/changelog-section.sh" "$1" --heading | awk -F'—' '
    NF >= 3 { s = $3; for (i = 4; i <= NF; i++) s = s "—" $i
              gsub(/^[ \t]+|[ \t]+$/, "", s); print s }'
}

# One version's section under its own "What's new in …" heading.
section_block() {
  local ver="$1" lead="$2" body name title
  body="$("$here/changelog-section.sh" "$ver")" || return 1
  [ -n "$body" ] || return 1
  name="$(release_name "$ver")"
  title="$lead ${ver%%-*}"
  [ -n "$name" ] && title="$title — $name"
  printf '## %s\n\n%s\n\n---\n\n' "$title" "$body"
}

# A build with no changelog section (a `workflow_dispatch` test build, tagged
# `v<run_number>`) still gets the download guidance — it just has nothing to announce.
section_block "$TAG" "What's new in" || true

# A patch is the same app as the `.0` it patches, so its page repeats that release's notes:
# someone landing on v0.8.1 came for MXB App, not for the two lines that changed since v0.8.0,
# and the features are what tells them whether they want it. Discord is deliberately left
# alone — `notify-discord.sh` reads only the tag's own section, because re-dumping a whole
# feature list into a chat channel for a patch is noise.
version="${TAG#v}"
version="${version%%-*}"
case "$version" in
  *.*.*)
    patch="${version##*.}"
    minor_zero="${version%.*}.0"
    if [ "$patch" != "0" ]; then
      section_block "v$minor_zero" "Everything new in" || true
    fi
    ;;
esac

cat <<'EOF'
## Which file do I download?

**Windows — download the `.exe`.** That's the installer, and it's what almost everyone
needs. Run it and you're done.

<details>
<summary>macOS and Linux</summary>

- **macOS (Apple Silicon)** — the `.dmg`.
- **Linux** — the `.AppImage` works on any distro: download, `chmod +x`, run it. Prefer
  your package manager? `.deb` for Debian/Ubuntu, `.rpm` for Fedora. Note MX Bikes itself
  runs through Proton on Linux.

</details>

The `.sig` and `.tar.gz` files are used by the in-app updater — you don't need to download
those.
EOF
