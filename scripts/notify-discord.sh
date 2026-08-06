#!/usr/bin/env bash
#
# Announce a published release to Discord.
#
#   scripts/notify-discord.sh <tag> [--print]
#
# Reads the release from GitHub and the matching section out of CHANGELOG.md, then posts
# a single embed to the webhook in $DISCORD_WEBHOOK_URL. `--print` dumps the payload to
# stdout instead of sending, so the message can be eyeballed without spamming the channel.
#
# Env:
#   DISCORD_WEBHOOK_URL  webhook to post to. Empty -> warn and exit 0, so a missing secret
#                        never fails a release that otherwise built and published fine.
#   GH_TOKEN             passed through to `gh` (the workflow hands it GITHUB_TOKEN).
#   REPO                 owner/name. Defaults to GITHUB_REPOSITORY, then Frostn1/mxb-app.

set -euo pipefail

TAG=""
PRINT_ONLY=0
for arg in "$@"; do
  case "$arg" in
    --print) PRINT_ONLY=1 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) TAG="$arg" ;;
  esac
done

if [ -z "$TAG" ]; then
  echo "usage: $0 <tag> [--print]" >&2
  exit 2
fi

REPO="${REPO:-${GITHUB_REPOSITORY:-Frostn1/mxb-app}}"
WEBHOOK="${DISCORD_WEBHOOK_URL:-}"

if [ "$PRINT_ONLY" -eq 0 ] && [ -z "$WEBHOOK" ]; then
  echo "::warning::DISCORD_WEBHOOK_URL is not set — skipping the Discord announcement."
  exit 0
fi

if ! root="$(git rev-parse --show-toplevel 2>/dev/null)" || [ -z "$root" ]; then
  root="$(cd "$(dirname "$0")/.." && pwd)"
fi
changelog="$root/CHANGELOG.md"

VERSION="${TAG#v}"
# Escape regex metacharacters so `0.6.1` can't match `0x6y1`.
ver_re="$(printf '%s' "$VERSION" | sed 's/[].[^$*\\/]/\\&/g')"
head_re="^## .*[[:space:]]v${ver_re}([[:space:]]|\$)"

meta="$(gh release view "$TAG" -R "$REPO" --json name,body,url,publishedAt,assets)"
rel_url="$(jq -r '.url' <<<"$meta")"
published="$(jq -r '.publishedAt' <<<"$meta")"

# --- title -------------------------------------------------------------------------
# Release headings look like `## 2026-08-06 — v0.6.1 — Rider tab gear slots`; the trailing
# segment is the human name for the release and makes a much better title than the tag.
heading=""
subtitle=""
if [ -f "$changelog" ]; then
  heading="$(awk -v re="$head_re" '$0 ~ re { print; exit }' "$changelog")"
  subtitle="$(printf '%s' "$heading" | awk -F'—' '
    NF >= 3 {
      s = $3
      for (i = 4; i <= NF; i++) s = s "—" $i
      gsub(/^[ \t]+|[ \t]+$/, "", s)
      print s
    }')"
fi

title="MXB App $TAG"
[ -n "$subtitle" ] && title="$title — $subtitle"

# --- description -------------------------------------------------------------------
# The changelog hard-wraps mid-sentence for readability in the file; Discord renders those
# newlines literally, so fold continuation lines back onto their bullet before sending.
body=""
if [ -n "$heading" ]; then
  body="$(awk -v re="$head_re" '
    $0 ~ re              { seen = 1; next }
    !seen                { next }
    seen && /^## /       { exit }
                         { print }
  ' "$changelog" | awk '
    function flush() { if (buf != "") { print buf; buf = "" } }
    /^[[:space:]]*$/     { flush(); print ""; next }
    /^[[:space:]]*[-*] / { flush(); buf = $0; next }
    /^#/                 { flush(); print; next }
    buf != ""            { line = $0; sub(/^[[:space:]]+/, "", line); buf = buf " " line; next }
                         { flush(); print }
    END                  { flush() }
  ' | sed -e '/./,$!d')"
fi

# Trim trailing blank lines.
body="$(printf '%s' "$body" | awk 'NF { blanks = 0; for (i = 0; i < held; i++) print ""; held = 0; print; next } { held++ }')"

if [ -z "$body" ]; then
  body="A new version is out — see the release page for details."
fi

# Discord caps embed descriptions at 4096 characters; leave room for the footer link.
limit=3500
if [ "${#body}" -gt "$limit" ]; then
  body="${body:0:$limit}…

[Full changelog →]($rel_url)"
fi

# --- download links ------------------------------------------------------------------
# Matched after the `finalize` job's rename, so these are the pretty `MXB-App-0.6.1-x64.exe`
# names rather than Tauri's `MXB.App_0.6.1_x64-setup.exe`.
win="$(jq -r '[.assets[] | select(.name | test("\\.exe$"))] | first | .url // empty' <<<"$meta")"
mac="$(jq -r '[.assets[] | select(.name | test("\\.dmg$"))] | first | .url // empty' <<<"$meta")"

icon="https://raw.githubusercontent.com/$REPO/$TAG/src-tauri/icons/icon.png"
avatar="https://raw.githubusercontent.com/$REPO/main/src-tauri/icons/icon.png"

payload="$(jq -n \
  --arg title "$title" \
  --arg url "$rel_url" \
  --arg desc "$body" \
  --arg icon "$icon" \
  --arg avatar "$avatar" \
  --arg ts "$published" \
  --arg win "$win" \
  --arg mac "$mac" \
  '{
    username: "MXB App",
    avatar_url: $avatar,
    allowed_mentions: { parse: [] },
    embeds: [{
      title: $title,
      url: $url,
      description: $desc,
      color: 10276076,
      thumbnail: { url: $icon },
      timestamp: $ts,
      footer: { text: "MXB App • GitHub Releases" },
      fields: (
        (if $win != "" then [{
          name: "⬇ Windows",
          value: ("[" + ($win | split("/") | last) + "](" + $win + ")"),
          inline: true
        }] else [] end)
        +
        (if $mac != "" then [{
          name: "⬇ macOS (Apple Silicon)",
          value: ("[" + ($mac | split("/") | last) + "](" + $mac + ")"),
          inline: true
        }] else [] end)
      )
    }]
  }')"

if [ "$PRINT_ONLY" -eq 1 ]; then
  printf '%s\n' "$payload"
  exit 0
fi

# --fail-with-body so a Discord-side rejection lands in the job log instead of passing
# silently with a 400 body nobody reads.
curl --fail-with-body -sS -X POST \
  -H 'Content-Type: application/json' \
  -d "$payload" \
  "$WEBHOOK"

echo "Announced $TAG to Discord."
