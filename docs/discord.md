# The Discord server

The server is the **MXB Secure** home. MXB Secure is the umbrella the products sit under —
the MXB App, Frost's Studio and the GUID lock — so the layout is a category per product with
the shared community channels around them, and every product announces its own releases in
its own channel.

That last part is the reason for most of what follows. One release channel for three
products means a studio build lands in front of manager users who cannot install it, and a
beta lands in front of players whose updater will never offer it. Four release channels and
four webhooks keep each announcement in front of the people it is for.

## The layout

| Category | Channels |
| --- | --- |
| **MXB SECURE** | `welcome`, `announcements`, `rules`, `status` |
| **MXB APP** | `app-releases`, `app-beta`, `app-help`, `app-bugs` |
| **FROST'S STUDIO** | `studio-releases`, `studio-beta`, `studio-help`, `studio-showcase` |
| **MXB SECURE — LOCK** | `secure-releases`, `secure-help`, `creators` |
| **COMMUNITY** | `general`, `paints-and-tracks`, `servers`, `off-topic` |

The five release channels are read-only for `@everyone` — they carry one automated message
per release and a conversation in them buries it. Each one's topic points at the help
channel in the same category, which is where the conversation goes instead.

## Building it

[`scripts/discord-server.mjs`](../scripts/discord-server.mjs) holds that table and applies
it. It matches everything **by name**, which makes it safe to run against the server as it
already is: channels that exist are adopted and moved under the right category, channels
that are missing are created, and anything not in the layout is left exactly where it is.

**It never deletes.** A channel it does not recognise keeps its name, its place and its
messages, and is listed at the end of the run as left alone.

### One-time setup

1. At <https://discord.com/developers/applications>, **New Application** → **Bot** →
   **Reset Token**, and keep the token. It is a bot token, not a webhook URL: a webhook can
   only post messages, it cannot create a channel.
2. Invite the bot, replacing `<APP_ID>` with the application's ID:

   ```
   https://discord.com/oauth2/authorize?client_id=<APP_ID>&scope=bot&permissions=536871984
   ```

   That number is View Channels + Manage Channels + Manage Server + Manage Webhooks —
   Manage Server only for `--rename-guild`, Manage Webhooks only for `--webhooks`.
3. In **Server Settings → Roles**, drag the bot's role above the roles whose channels it
   has to touch. A bot with the right permission still cannot edit a channel belonging to a
   role above its own.
4. Turn on **Developer Mode** (User Settings → Advanced) and right-click the server →
   **Copy Server ID**.

### Running it

```sh
export DISCORD_BOT_TOKEN='...'
export DISCORD_GUILD_ID='...'

node scripts/discord-server.mjs                        # prints the plan, changes nothing
node scripts/discord-server.mjs --apply --rename-guild # does it
node scripts/discord-server.mjs --apply --webhooks     # creates the webhooks, prints the URLs
```

The first form is the one to run first: it prints every create, move and topic change it
would make, and the list of channels it would leave alone. Nothing is sent but the reads.

| Flag | What it adds |
| --- | --- |
| `--apply` | Actually write. Without it the script only reads and prints. |
| `--rename-guild` | Also rename the server to **MXB Secure**. Separate because every member sees it happen. |
| `--webhooks` | Create each release channel's webhook and print its URL. **The URLs are credentials** — anyone holding one can post as the app. Never pass this in CI, where the output is a log. |
| `--enforce-permissions` | Also apply the read-only overwrite to release channels that already existed. Off by default so a channel someone tuned by hand is not overwritten by a default. |

Running it a second time prints `Nothing to change`.

## The webhooks

`--webhooks` prints each URL next to the name of the secret it belongs in:

| Secret | Channel | Repository | Set by |
| --- | --- | --- | --- |
| `DISCORD_WEBHOOK_URL` | `#app-releases` | `Frostn1/mxb-app` | [`release.yml`](../.github/workflows/release.yml) |
| `DISCORD_BETA_WEBHOOK_URL` | `#app-beta` | `Frostn1/mxb-app` | [`release.yml`](../.github/workflows/release.yml) |
| `DISCORD_STUDIO_WEBHOOK_URL` | `#studio-releases` | `Frostn1/mxb-app` | [`release-studio.yml`](../.github/workflows/release-studio.yml) |
| `DISCORD_STUDIO_BETA_WEBHOOK_URL` | `#studio-beta` | `Frostn1/mxb-app` | [`release-studio.yml`](../.github/workflows/release-studio.yml) |
| `DISCORD_SECURE_WEBHOOK_URL` | `#secure-releases` | `Frostn1/mxbsecure-web` | that repo's `deploy.yml` |

```sh
gh secret set DISCORD_STUDIO_WEBHOOK_URL -R Frostn1/mxb-app
gh secret set DISCORD_SECURE_WEBHOOK_URL -R Frostn1/mxbsecure-web
```

A missing secret is a **warning, not a failure**: the release is built and published either
way, and only the announcement is skipped. No webhook ever falls back to another — not a
beta to a release channel, not one product to another — because an announcement in the wrong
channel is worse than one nobody sent.

## What each product announces

**MXB App** and **Frost's Studio** are tagged releases, both announced by
[`scripts/notify-discord.sh`](../scripts/notify-discord.sh), which composes the embed from
the tag's `CHANGELOG.md` section — the same section the release page is built from, so the
two cannot drift. `--app` picks the product, and with it the webhook pair, the icon, the
changelog filter and the repository the release is read from:

```sh
scripts/notify-discord.sh v0.14.2 --print                 # the manager, rendered not sent
scripts/notify-discord.sh v0.1.6 --app studio --print     # the studio
```

The studio needs both of its repositories: the release lives in `Frostn1/frost-studio`, and
the changelog and icon live here. That is also why its job passes `STUDIO_RELEASE_TOKEN` as
`GH_TOKEN` — `GITHUB_TOKEN` cannot read a release in another repository.

The changelog filter is not decoration. One `CHANGELOG.md` records both products and their
version numbers run independently, so `v0.14.0` will one day match a section of each.
`changelog-section.sh --product "Frost's Studio"` is what keeps them apart.

**mxbsecure.com** has no tags — every push to `main` deploys — so its announcer lives in the
`mxbsecure-web` repo and reads the top `## <date>` section of that repo's changelog instead.
It sends only when the push **added** that heading, so the channel gets one message per
shipped set of changes rather than one per commit.

## Changing the layout

Edit `LAYOUT` at the top of [`scripts/discord-server.mjs`](../scripts/discord-server.mjs)
and re-run. Adding a channel or a category is a plain addition. Note what the `key` field is
for: it is the stable handle, so **renaming a channel means changing `name` and leaving
`key` alone**. Changing `key` instead makes the script treat it as a channel it has never
seen, and create a second one next to the first.

Adding a fourth product means a category here and a `case` branch in
`notify-discord.sh` — nothing else in either script changes.
