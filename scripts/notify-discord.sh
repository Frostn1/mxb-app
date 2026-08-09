#!/usr/bin/env bash
#
# Announce a published release to Discord.
#
#   scripts/notify-discord.sh <tag> [--print]
#
# Reads the release from GitHub and the matching section out of CHANGELOG.md, then posts
# a single embed to the webhook for that kind of release. `--print` dumps the payload to
# stdout instead of sending, so the message can be eyeballed without spamming the channel.
#
# A suffixed tag (`v0.8.0-beta.2`) is a beta: same embed, but it goes to the beta channel,
# is titled and coloured as a beta, and opens by saying the updater won't hand it to you.
#
# Env:
#   DISCORD_WEBHOOK_URL       webhook for a full release.
#   DISCORD_BETA_WEBHOOK_URL  webhook for a beta. Never falls back to the one above.
#                             Either being empty -> warn and exit 0, so a missing secret
#                             never fails a release that otherwise built and published fine.
#   GH_TOKEN                  passed through to `gh` (the workflow hands it GITHUB_TOKEN).
#   REPO                      owner/name. Defaults to GITHUB_REPOSITORY, then Frostn1/mxb-app.

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

# A suffixed tag (`v0.8.0-beta.2`) is a beta build of the version it names — the same test
# release.yml uses to publish it as a pre-release, and release-notes.sh to head the release
# page with a tester's note. It's announced in the beta channel: the people who asked for it
# want to hear straight away, while everyone else should hear about the version once, when
# the updater can actually hand it to them.
case "$TAG" in
  *-*) IS_BETA=1 ;;
  *)   IS_BETA=0 ;;
esac

if [ "$IS_BETA" -eq 1 ]; then
  WEBHOOK_VAR="DISCORD_BETA_WEBHOOK_URL"
  WEBHOOK="${DISCORD_BETA_WEBHOOK_URL:-}"
else
  WEBHOOK_VAR="DISCORD_WEBHOOK_URL"
  WEBHOOK="${DISCORD_WEBHOOK_URL:-}"
fi

# No falling back to the other webhook when one is missing: a beta announced in the release
# channel is worse than a beta nobody announced, and the release channel is the one every
# player watches.
if [ "$PRINT_ONLY" -eq 0 ] && [ -z "$WEBHOOK" ]; then
  echo "::warning::$WEBHOOK_VAR is not set — skipping the Discord announcement."
  exit 0
fi

here="$(cd "$(dirname "$0")" && pwd)"
section="$here/changelog-section.sh"

meta="$(gh release view "$TAG" -R "$REPO" --json name,body,url,publishedAt,assets)"
rel_url="$(jq -r '.url' <<<"$meta")"
published="$(jq -r '.publishedAt' <<<"$meta")"

# --- title -------------------------------------------------------------------------
# Release headings look like `## 2026-08-06 — v0.6.1 — Rider tab gear slots`; the trailing
# segment is the human name for the release and makes a much better title than the tag.
heading="$("$section" "$TAG" --heading || true)"
subtitle="$(printf '%s' "$heading" | awk -F'—' '
  NF >= 3 {
    s = $3
    for (i = 4; i <= NF; i++) s = s "—" $i
    gsub(/^[ \t]+|[ \t]+$/, "", s)
    print s
  }')"

title="MXB App $TAG"
[ -n "$subtitle" ] && title="$title — $subtitle"
# `changelog-section.sh` reads a beta against the section for the version it's a build of, so
# a beta borrows that release's headline once it's written and simply has none before then.
[ "$IS_BETA" -eq 1 ] && title="🧪 Beta — $title"

# --- description -------------------------------------------------------------------
# Discord caps embed descriptions at 4096 characters; stop short of that so the footer
# link always has room. A release big enough to blow the cap gets its headlines instead
# of the first N characters — every item still gets named, which a hard cut can't
# promise, and the detail is one click away on the release page.
#
# `--fold` because the changelog hard-wraps mid-sentence for readability in the file and
# Discord renders those newlines literally.
limit=3500
more="

[Full notes, with the detail →]($rel_url)"

# A beta says what it is before it says what's in it: the one thing a tester needs to know is
# that nothing will arrive on its own. Same fact release-notes.sh leads the release page with,
# so the two can't drift. It spends part of the cap, so the ladder below measures what's left.
lede=""
if [ "$IS_BETA" -eq 1 ]; then
  lede="**Beta build of ${TAG%%-*} — for testing.** Installed copies won't be offered it by the updater; grab an installer below to try it. Tell us what breaks, and the full release follows.

"
  limit=$(( limit - ${#lede} ))
fi

body="$("$section" "$TAG" --fold || true)"

if [ -n "$body" ] && [ "${#body}" -gt "$limit" ]; then
  short="$("$section" "$TAG" --summary || true)"
  [ -n "$short" ] && body="$short$more"
fi

# Headlines alone still over the cap (a genuinely enormous release) — now it's a trim.
if [ "${#body}" -gt "$limit" ]; then
  body="${body:0:$limit}…$more"
fi

# A beta cut before its section is written is normal — it's the build the notes get written
# from — so it gets the lede and a pointer rather than a claim about what's in it.
if [ -z "$body" ]; then
  if [ "$IS_BETA" -eq 1 ]; then
    body="See the release page for what went into this build."
  else
    body="A new version is out — see the release page for details."
  fi
fi

body="$lede$body"

# --- download links ------------------------------------------------------------------
# Matched after the `finalize` job's rename, so these are the pretty `MXB-App-0.6.1-x64.exe`
# names rather than Tauri's `MXB.App_0.6.1_x64-setup.exe`.
win="$(jq -r '[.assets[] | select(.name | test("\\.exe$"))] | first | .url // empty' <<<"$meta")"
mac="$(jq -r '[.assets[] | select(.name | test("\\.dmg$"))] | first | .url // empty' <<<"$meta")"
# The AppImage is the portable one that works on any distro, so it's the Linux link
# worth putting in a chat message; .deb/.rpm are a click away on the release page.
lin="$(jq -r '[.assets[] | select(.name | test("\\.AppImage$"))] | first | .url // empty' <<<"$meta")"

icon="https://raw.githubusercontent.com/$REPO/$TAG/src-tauri/icons/icon.png"
avatar="https://raw.githubusercontent.com/$REPO/main/src-tauri/icons/icon.png"

# Amber down the side of a beta instead of the usual blue, and a footer that says so — the
# two announcements sit in different channels, but plenty of people watch both.
color=10276076
footer="MXB App • GitHub Releases"
if [ "$IS_BETA" -eq 1 ]; then
  color=15246141   # 0xE8A33D
  footer="MXB App • Beta • GitHub Releases"
fi

payload="$(jq -n \
  --arg title "$title" \
  --arg url "$rel_url" \
  --arg desc "$body" \
  --arg icon "$icon" \
  --arg avatar "$avatar" \
  --arg ts "$published" \
  --arg win "$win" \
  --arg mac "$mac" \
  --arg lin "$lin" \
  --argjson color "$color" \
  --arg footer "$footer" \
  '{
    username: "MXB App",
    avatar_url: $avatar,
    allowed_mentions: { parse: [] },
    embeds: [{
      title: $title,
      url: $url,
      description: $desc,
      color: $color,
      thumbnail: { url: $icon },
      timestamp: $ts,
      footer: { text: $footer },
      fields: (
        (if $win != "" then [{
          name: "⬇ Windows — start here",
          value: ("[" + ($win | split("/") | last) + "](" + $win + ")"),
          inline: true
        }] else [] end)
        +
        (if $mac != "" then [{
          name: "⬇ macOS (Apple Silicon)",
          value: ("[" + ($mac | split("/") | last) + "](" + $mac + ")"),
          inline: true
        }] else [] end)
        +
        (if $lin != "" then [{
          name: "⬇ Linux (AppImage)",
          value: ("[" + ($lin | split("/") | last) + "](" + $lin + ")"),
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

if [ "$IS_BETA" -eq 1 ]; then
  echo "Announced beta $TAG to the beta Discord channel."
else
  echo "Announced $TAG to Discord."
fi
