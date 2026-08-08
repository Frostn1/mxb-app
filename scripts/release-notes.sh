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

# A build with no changelog section (a `workflow_dispatch` test build, tagged
# `v<run_number>`) still gets the download guidance — it just has nothing to announce.
if section="$("$here/changelog-section.sh" "$TAG")" && [ -n "$section" ]; then
  heading="$("$here/changelog-section.sh" "$TAG" --heading)"
  # `## 2026-08-07 — v0.7.0 — Six languages, …` → the part after the version.
  name="$(printf '%s' "$heading" | awk -F'—' '
    NF >= 3 { s = $3; for (i = 4; i <= NF; i++) s = s "—" $i
              gsub(/^[ \t]+|[ \t]+$/, "", s); print s }')"
  title="What's new in ${TAG%%-*}"
  [ -n "$name" ] && title="$title — $name"
  cat <<EOF
## $title

$section

---

EOF
fi

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
