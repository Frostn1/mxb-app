#!/usr/bin/env bash
#
# Compose the GitHub Release body for a tag.
#
#   scripts/release-notes.sh <tag> [--app=NAME]
#
# What's new first — the tag's CHANGELOG section, so the release page says what shipped
# instead of only which file to download — then the download guidance. Used by
# .github/workflows/release.yml and release-coach.yml, which pass the result to tauri-action
# as `releaseBody`, so this file is the one place either half is worded.
#
#   --app=NAME  compose another product's notes: its changelog sections are read by that
#               name (`MXB Coach v0.1.17`), and NAME is what the page calls the app. Without
#               it, the MXB App's.

set -euo pipefail

TAG=""
APP=""
for arg in "$@"; do
  case "$arg" in
    --app=*) APP="${arg#--app=}" ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) TAG="$arg" ;;
  esac
done

if [ -z "$TAG" ]; then
  echo "usage: $0 <tag> [--app=NAME]" >&2
  exit 2
fi

here="$(cd "$(dirname "$0")" && pwd)"

# Which product's changelog sections to read. Passed to every changelog-section.sh call
# below, so a Coach v0.1.17 can't come out reading the MXB App's v0.1.17 notes.
section() {
  if [ -n "$APP" ]; then
    "$here/changelog-section.sh" "$@" --app="$APP"
  else
    "$here/changelog-section.sh" "$@"
  fi
}

# A suffixed tag (`v0.7.0-beta.1`) is a beta build of the version it names. Say so at the
# top, because the one thing a tester needs to know is how it reaches them.
case "$TAG" in
  *-*)
    if [ -n "$APP" ]; then
      cat <<EOF
> [!NOTE]
> **This is a beta build of ${TAG%%-*}, for testing.** $APP installs it for you while
> Beta updates is on in Settings, which it is unless you turned it off. You can also
> download the installer below.

EOF
    else
      cat <<EOF
> [!NOTE]
> **This is a beta build of ${TAG%%-*}, for testing.** To get it in MXB App, turn on
> Beta updates in Settings → About, or download the installer below. The full release
> follows once it's been checked over.

EOF
    fi
    ;;
esac

# `## 2026-08-07 — v0.7.0 — Six languages, …` → the part after the version.
release_name() {
  section "$1" --heading | awk -F'—' '
    NF >= 3 { s = $3; for (i = 4; i <= NF; i++) s = s "—" $i
              gsub(/^[ \t]+|[ \t]+$/, "", s); print s }'
}

# One version's section under its own "What's new in …" heading.
section_block() {
  local ver="$1" lead="$2" body name title
  body="$(section "$ver")" || return 1
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
#
# The manager only. The coach ships a beta at every patch, so repeating 0.1.0 under each of
# them would bury what actually changed.
if [ -z "$APP" ]; then
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
fi

# First run instructions, for an app new enough that most people downloading it have never
# opened it. The manager's own page doesn't need this.
if [ "$APP" = "MXB Coach" ]; then
  cat <<'EOF'
## First time here?

1. Install MXB Coach — on Windows that's the `.exe` below.
2. Open **Settings** and press **Install** to add the recorder to MX Bikes. Restart the game
   if it's open.
3. Ride a few laps, ideally in Testing. Every stint on track becomes a session.
4. Open **Sessions**, pick the session and press **Review** on a lap. It's compared with your
   fastest whole lap on that track.

The recorder only saves your laps. It changes nothing in the game.

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
