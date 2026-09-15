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
#   APP_NAME                  the product this release is of. Defaults to the mod manager;
#                             a second app's workflow passes its own, and its changelog
#                             sections are then read by that name (`Frost's Studio v0.1.6`).
#   ICON_URL                  the embed thumbnail. Defaults to the manager's icon at $TAG.
#   SLUG                      the product on mxbsecure.com and get.mxbsecure.com. Defaults to app.

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
# Never put an apostrophe in this default: inside `${...}` bash reads it as an opening quote
# and dies far below with `unexpected EOF`. v0.14.0-beta.3 announced nothing because of it.
APP_NAME="${APP_NAME:-MXB App}"

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
# Another product's sections carry its name before the version; the MXB App's don't.
app_arg=""
[ "$APP_NAME" != "MXB App" ] && app_arg="--app=$APP_NAME"

heading="$("$section" "$TAG" ${app_arg:+"$app_arg"} --heading || true)"
subtitle="$(printf '%s' "$heading" | awk -F'—' '
  NF >= 3 {
    s = $3
    for (i = 4; i <= NF; i++) s = s "—" $i
    gsub(/^[ \t]+|[ \t]+$/, "", s)
    print s
  }')"

title="$APP_NAME $TAG"
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
  lede="**Beta build of ${TAG%%-*} — for testing.** To get it in the app, turn on Beta updates in Settings → About, or grab an installer below. Tell us what breaks, and the full release follows.

"
  limit=$(( limit - ${#lede} ))
fi

body="$("$section" "$TAG" ${app_arg:+"$app_arg"} --fold || true)"

if [ -n "$body" ] && [ "${#body}" -gt "$limit" ]; then
  short="$("$section" "$TAG" ${app_arg:+"$app_arg"} --summary || true)"
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

# The product's page on mxbsecure.com goes under the notes; the 3500 cap leaves room for it.
SLUG="${SLUG:-app}"
body="$body

[More about $APP_NAME →](https://mxbsecure.com/$SLUG)"

# --- download links ------------------------------------------------------------------
# Matched after the `finalize` job's rename, so these are the pretty `MXB-App-0.6.1-x64.exe`
# names rather than Tauri's `MXB.App_0.6.1_x64-setup.exe`.
win="$(jq -r '[.assets[] | select(.name | test("\\.exe$"))] | first | .url // empty' <<<"$meta")"
mac="$(jq -r '[.assets[] | select(.name | test("\\.dmg$"))] | first | .url // empty' <<<"$meta")"
# The AppImage is the portable one that works on any distro, so it's the Linux link
# worth putting in a chat message; .deb/.rpm are a click away on the release page.
lin="$(jq -r '[.assets[] | select(.name | test("\\.AppImage$"))] | first | .url // empty' <<<"$meta")"
win_name="${win##*/}" mac_name="${mac##*/}" lin_name="${lin##*/}"

# A release links through get.mxbsecure.com, which serves the product's latest release, and
# that is this one when the post goes out. A beta is never "latest" there, so it keeps GitHub's.
if [ "$IS_BETA" -eq 0 ]; then
  win="${win:+https://get.mxbsecure.com/$SLUG/windows}"
  mac="${mac:+https://get.mxbsecure.com/$SLUG/mac}"
  lin="${lin:+https://get.mxbsecure.com/$SLUG/linux}"
fi

icon="${ICON_URL:-https://raw.githubusercontent.com/$REPO/$TAG/apps/manager/src-tauri/icons/icon.png}"
# Every product announces as mxbsecure, the brand that releases them; the thumbnail stays the product's.
# Always from mxb-app: $REPO is frost-studio for a Studio release, which doesn't carry the image.
avatar="https://raw.githubusercontent.com/Frostn1/mxb-app/main/docs/brand/mxbsecure-m-512.png"

# Amber down the side of a beta instead of the usual black, and a footer that says so — the
# two announcements sit in different channels, but plenty of people watch both.
color=723724   # 0x0B0B0C, the mxbsecure black
footer="$APP_NAME • GitHub Releases"
if [ "$IS_BETA" -eq 1 ]; then
  color=15246141   # 0xE8A33D
  footer="$APP_NAME • Beta • GitHub Releases"
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
  --arg winname "$win_name" \
  --arg macname "$mac_name" \
  --arg linname "$lin_name" \
  --argjson color "$color" \
  --arg footer "$footer" \
  '{
    username: "mxbsecure",
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
          value: ("[" + $winname + "](" + $win + ")"),
          inline: true
        }] else [] end)
        +
        (if $mac != "" then [{
          name: "⬇ macOS (Apple Silicon)",
          value: ("[" + $macname + "](" + $mac + ")"),
          inline: true
        }] else [] end)
        +
        (if $lin != "" then [{
          name: "⬇ Linux (AppImage)",
          value: ("[" + $linname + "](" + $lin + ")"),
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
