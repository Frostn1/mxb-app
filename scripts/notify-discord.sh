#!/usr/bin/env bash
#
# Announce a published release to Discord.
#
#   scripts/notify-discord.sh <tag> [--app <manager|studio>] [--print]
#
# Reads the release from GitHub and the matching section out of CHANGELOG.md, then posts
# a single embed to the webhook for that kind of release. `--print` dumps the payload to
# stdout instead of sending, so the message can be eyeballed without spamming the channel.
#
# A suffixed tag (`v0.8.0-beta.2`) is a beta: same embed, but it goes to the beta channel,
# is titled and coloured as a beta, and opens by saying the updater won't hand it to you.
#
# `--app` picks the product, and with it four channels rather than two: the Discord server
# is the MXB Secure home and each product has its own releases and beta channel under its
# own category, so a studio release never lands in the channel manager users watch. It also
# picks the icon, the changelog product filter and the repository the release is read from,
# which for the studio is a different repository from the one this script lives in.
# Defaults to `manager`, so the manager's existing call needs no argument.
#
# Env:
#   DISCORD_WEBHOOK_URL              manager, full release.
#   DISCORD_BETA_WEBHOOK_URL         manager, beta.
#   DISCORD_STUDIO_WEBHOOK_URL       studio, full release.
#   DISCORD_STUDIO_BETA_WEBHOOK_URL  studio, beta.
#                             A beta webhook never falls back to the release one, and no
#                             product's ever falls back to another's. Whichever is needed
#                             being empty -> warn and exit 0, so a missing secret never
#                             fails a release that otherwise built and published fine.
#                             `scripts/discord-server.mjs --webhooks` creates all four.
#   GH_TOKEN                  passed through to `gh`. Must be able to read the release in
#                             the product's release repository — for the studio that is
#                             frost-studio, which GITHUB_TOKEN cannot read.
#   REPO                      the RELEASE repository, owner/name. Defaults per product.
#   SOURCE_REPO               where the icons and CHANGELOG.md live. Defaults to
#                             GITHUB_REPOSITORY, then Frostn1/mxb-app.
#   APP_NAME                  overrides the product's display name.

set -euo pipefail

TAG=""
PRINT_ONLY=0
APP="manager"
want_app=0
for arg in "$@"; do
  if [ "$want_app" -eq 1 ]; then
    APP="$arg"
    want_app=0
    continue
  fi
  case "$arg" in
    --print) PRINT_ONLY=1 ;;
    --app) want_app=1 ;;
    -*) echo "unknown option: $arg" >&2; exit 2 ;;
    *) TAG="$arg" ;;
  esac
done

if [ "$want_app" -eq 1 ]; then
  echo "--app needs a product" >&2
  exit 2
fi

if [ -z "$TAG" ]; then
  echo "usage: $0 <tag> [--app <manager|studio>] [--print]" >&2
  exit 2
fi

# Everything that differs between the two products, in one place. Adding a third means
# adding a branch here and a category to scripts/discord-server.mjs — nothing below changes.
#
#   DEFAULT_REPO    where the release is published, which is not where this script lives.
#   SOURCE_PREFIX   the tag in the source repo: the studio is tagged `studio-v0.1.6` here
#                   and published as `v0.1.6` over there, and the icon is fetched from here.
#   ICON_PATH       the app's own icon, so the embeds are told apart at a glance.
#   PRODUCT         the changelog filter. One CHANGELOG.md records both products and their
#                   versions run independently, so the version alone is not enough.
case "$APP" in
  manager)
    DEFAULT_APP_NAME="MXB App"
    DEFAULT_REPO="Frostn1/mxb-app"
    SOURCE_PREFIX=""
    ICON_PATH="apps/manager/src-tauri/icons/icon.png"
    # Empty: the manager's sections are headed `v0.14.2` with no product in them, and a
    # filter it cannot match would find nothing.
    PRODUCT=""
    RELEASE_HOOK_VAR="DISCORD_WEBHOOK_URL"
    BETA_HOOK_VAR="DISCORD_BETA_WEBHOOK_URL"
    ;;
  studio)
    # Never put an apostrophe in a `${...}` default: bash reads it as an opening quote and
    # dies far below with `unexpected EOF`. v0.14.0-beta.3 announced nothing because of it.
    # These are plain assignments, so the apostrophe here is safe.
    DEFAULT_APP_NAME="Frost's Studio"
    DEFAULT_REPO="Frostn1/frost-studio"
    SOURCE_PREFIX="studio-"
    ICON_PATH="apps/studio/src-tauri/icons/icon.png"
    PRODUCT="Frost's Studio"
    RELEASE_HOOK_VAR="DISCORD_STUDIO_WEBHOOK_URL"
    BETA_HOOK_VAR="DISCORD_STUDIO_BETA_WEBHOOK_URL"
    ;;
  *)
    echo "unknown product: $APP (expected manager or studio)" >&2
    exit 2
    ;;
esac

REPO="${REPO:-$DEFAULT_REPO}"
SOURCE_REPO="${SOURCE_REPO:-${GITHUB_REPOSITORY:-Frostn1/mxb-app}}"
APP_NAME="${APP_NAME:-$DEFAULT_APP_NAME}"

# `--product ""` would be an empty argument the option parser reads as the name, so the
# filter is passed as an array that is empty when there is nothing to filter on.
product_filter=()
[ -n "$PRODUCT" ] && product_filter=(--product "$PRODUCT")

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
  WEBHOOK_VAR="$BETA_HOOK_VAR"
else
  WEBHOOK_VAR="$RELEASE_HOOK_VAR"
fi
# Read the variable the product's table named. `:-` so an unset secret is empty rather than
# an error under `set -u`, which is what makes the skip below reachable.
WEBHOOK="${!WEBHOOK_VAR:-}"

# No falling back to any other webhook when one is missing: a beta announced in the release
# channel is worse than a beta nobody announced, the release channel is the one every player
# watches, and a studio release in the manager's channel is a release most of its readers
# cannot install.
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
heading="$("$section" "$TAG" "${product_filter[@]+"${product_filter[@]}"}" --heading || true)"
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

body="$("$section" "$TAG" "${product_filter[@]+"${product_filter[@]}"}" --fold || true)"

if [ -n "$body" ] && [ "${#body}" -gt "$limit" ]; then
  short="$("$section" "$TAG" "${product_filter[@]+"${product_filter[@]}"}" --summary || true)"
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

# From SOURCE_REPO, not REPO: the studio's releases live in frost-studio, which holds the
# built installers and no source, so its icon is not there to link to. The tag is the source
# repo's own (`studio-v0.1.6`), which pins the icon to the commit the release was built from.
icon="https://raw.githubusercontent.com/$SOURCE_REPO/$SOURCE_PREFIX$TAG/$ICON_PATH"
avatar="https://raw.githubusercontent.com/$SOURCE_REPO/main/$ICON_PATH"

# Amber down the side of a beta instead of the usual blue, and a footer that says so — the
# two announcements sit in different channels, but plenty of people watch both.
color=10276076
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
  --argjson color "$color" \
  --arg footer "$footer" \
  --arg appname "$APP_NAME" \
  '{
    username: $appname,
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
